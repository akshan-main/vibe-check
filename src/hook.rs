use crate::config::{load_config, load_state, save_state, Mode, State};
use crate::diff::{analyze_diff, check_file_risks, compute_risk_level, hash_string};
use crate::git::{git_cmd, is_git_repo, resolve_project_dir, HookPayload};
use crate::quiz::{
    build_explain_reason, build_reason, collect_working_context, detect_primary_category,
};
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn compute_weak_area_hint(categories_path: &Path) -> Option<String> {
    let content = fs::read_to_string(categories_path).ok()?;
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content).ok()?;

    let mut worst: Option<(&str, f64, u64, u64)> = None; // (name, accuracy, correct, total)
    for (cat, val) in &map {
        if let Some(arr) = val.as_array() {
            let total = arr.first().and_then(|v| v.as_u64()).unwrap_or(0);
            let correct = arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0);
            if total >= 3 {
                let accuracy = correct as f64 / total as f64;
                if accuracy < 0.6 && worst.map(|(_, a, _, _)| accuracy < a).unwrap_or(true) {
                    worst = Some((cat.as_str(), accuracy, correct, total));
                }
            }
        }
    }

    worst.map(|(cat, accuracy, correct, total)| {
        format!(
            "This developer struggles with {} ({}/{} correct, {:.0}% accuracy). \
             When the diff touches this area, prefer questions that build understanding of it.",
            cat,
            correct,
            total,
            accuracy * 100.0
        )
    })
}

#[derive(Serialize)]
struct BlockDecision {
    decision: String,
    reason: String,
}

pub(crate) fn run_hook() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let payload: HookPayload = serde_json::from_str(&input).unwrap_or_default();

    if payload.hook_event_name.as_deref() != Some("Stop") {
        return Ok(());
    }

    if payload.stop_hook_active.unwrap_or(false) {
        return Ok(());
    }

    if let Some(ref msg) = payload.last_assistant_message {
        if msg.contains("[vibecheck:done]") {
            return Ok(());
        }
    }

    let project_dir = resolve_project_dir(&payload)?;

    let config = load_config(&project_dir);
    if !config.enabled.unwrap_or(true) {
        return Ok(());
    }

    if !is_git_repo(&project_dir) {
        return Ok(());
    }

    let status = git_cmd(&project_dir, &["status", "--porcelain"])?;
    let has_tracked_changes = status
        .lines()
        .any(|l| !l.trim().is_empty() && !l.starts_with("??"));
    if !has_tracked_changes {
        return Ok(());
    }

    let state_dir = project_dir.join(".claude").join(".vibecheck");
    fs::create_dir_all(&state_dir)?;
    let state_path = state_dir.join("state.json");
    let state = load_state(&state_path);

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    if state.snoozed_until > 0 && now < state.snoozed_until {
        return Ok(());
    }

    let min_gap = config.min_seconds_between_quizzes.unwrap_or(0);
    if min_gap > 0 && now.saturating_sub(state.last_quiz_at) < min_gap {
        return Ok(());
    }

    let max_chars = config.max_diff_chars.unwrap_or(2000);
    let ctx = collect_working_context(&project_dir, max_chars)?;

    if ctx.raw_diff.trim().is_empty() {
        return Ok(());
    }

    let diff_hash = hash_string(&ctx.raw_diff);
    if diff_hash == state.last_diff_hash {
        return Ok(());
    }

    let files_line = if ctx.files.is_empty() {
        "(unknown files)".to_string()
    } else {
        ctx.files.join(", ")
    };

    let mut summary = analyze_diff(&ctx.raw_diff);
    check_file_risks(&ctx.files, &mut summary);
    let risk = compute_risk_level(&summary);

    let new_state = State {
        last_quiz_at: now,
        last_diff_hash: diff_hash,
        snoozed_until: 0,
        total_quizzes: state.total_quizzes,
        total_correct: state.total_correct,
        streak: state.streak,
    };
    save_state(&state_path, &new_state);

    let mode = config.mode.clone().unwrap_or(Mode::VibeCoder);
    let track_progress = config.track_progress.unwrap_or(false);

    let category = detect_primary_category(&summary);
    let categories_path = state_dir.join("categories.json");
    let weak_area_hint = compute_weak_area_hint(&categories_path);

    let state_path_str = state_path.to_string_lossy().to_string();
    let config_path = project_dir.join(".claude").join("vibecheck.json");
    let config_path_str = config_path.to_string_lossy().to_string();
    let reason = if config.hook_action.as_deref() == Some("explain") {
        build_explain_reason(
            &files_line,
            &ctx.diff,
            &ctx.non_diff_files,
            &state_path_str,
            &config_path_str,
            &mode,
            risk,
        )
    } else {
        build_reason(
            &files_line,
            &ctx.diff,
            &ctx.non_diff_files,
            &state_path_str,
            &config_path_str,
            &mode,
            risk,
            config.difficulty.as_deref(),
            track_progress,
            &state,
            category,
            weak_area_hint.as_deref(),
        )
    };

    let decision = BlockDecision {
        decision: "block".to_string(),
        reason,
    };
    println!("{}", serde_json::to_string(&decision)?);

    Ok(())
}
