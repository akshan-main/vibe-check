use crate::config::{Config, Mode, State};
use crate::diff::{dedup_file_list, truncate_diff, DiffSummary, RiskLevel};
use crate::git::{git_cmd, git_cmd_with_timeout};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DifficultyLevel {
    L1Recall,
    L2Comprehension,
    L3Verification,
    L4Safety,
}

impl std::fmt::Display for DifficultyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DifficultyLevel::L1Recall => write!(f, "L1 Recall"),
            DifficultyLevel::L2Comprehension => write!(f, "L2 Comprehension"),
            DifficultyLevel::L3Verification => write!(f, "L3 Verification"),
            DifficultyLevel::L4Safety => write!(f, "L4 Safety"),
        }
    }
}

/// Resolve the difficulty level based on mode, risk, optional override, and state (for learning mode).
pub(crate) fn resolve_difficulty(
    mode: &Mode,
    risk: RiskLevel,
    difficulty_override: Option<&str>,
    state: &State,
) -> DifficultyLevel {
    // Manual override takes precedence
    if let Some(ovr) = difficulty_override {
        return match ovr {
            "beginner" => DifficultyLevel::L1Recall,
            "advanced" => DifficultyLevel::L4Safety,
            _ => resolve_mode_difficulty(mode, risk, state),
        };
    }
    resolve_mode_difficulty(mode, risk, state)
}

fn resolve_mode_difficulty(mode: &Mode, risk: RiskLevel, state: &State) -> DifficultyLevel {
    match mode {
        Mode::VibeCoder => DifficultyLevel::L1Recall,
        Mode::Developer => match risk {
            RiskLevel::Low => DifficultyLevel::L2Comprehension,
            RiskLevel::Medium => DifficultyLevel::L2Comprehension,
            RiskLevel::High => DifficultyLevel::L3Verification,
        },
        Mode::Hardcore => DifficultyLevel::L4Safety,
        Mode::Learning => {
            if state.total_quizzes == 0 {
                return DifficultyLevel::L1Recall;
            }
            let accuracy = (state.total_correct as f64) / (state.total_quizzes as f64).max(1.0);
            if accuracy < 0.4 {
                DifficultyLevel::L1Recall
            } else if accuracy < 0.6 {
                DifficultyLevel::L2Comprehension
            } else if accuracy < 0.8 {
                DifficultyLevel::L3Verification
            } else {
                DifficultyLevel::L4Safety
            }
        }
    }
}

fn mode_prompt(mode: &Mode, difficulty: DifficultyLevel, risk: RiskLevel) -> String {
    let mode_instruction = match mode {
        Mode::VibeCoder => {
            "MODE: VIBE CODER - Keep it light and friendly. Ask what changed in plain language. \
             The goal is awareness, not stress. Use casual tone."
        }
        Mode::Developer => {
            "MODE: DEVELOPER - Be objective and verification-driven. Ask questions that a \
             professional developer should know about their own changes. Focus on correctness \
             and intent."
        }
        Mode::Hardcore => {
            "MODE: HARDCORE - Maximum rigor. Ask about failure modes, edge cases, security \
             implications, or data integrity risks. The question should make the developer \
             pause and think carefully."
        }
        Mode::Learning => {
            "MODE: LEARNING - Educational and encouraging. After the answer, explain the \
             concept with a concrete example. Help the developer build understanding, \
             not just test it."
        }
    };

    let difficulty_instruction = match difficulty {
        DifficultyLevel::L1Recall => {
            "DIFFICULTY: L1 RECALL - Ask what changed. The developer should be able to answer \
             from memory of what they just built. Keep wrong answers clearly wrong."
        }
        DifficultyLevel::L2Comprehension => {
            "DIFFICULTY: L2 COMPREHENSION - Ask why it changed. The developer should understand \
             the purpose and intent behind the change, not just what it does."
        }
        DifficultyLevel::L3Verification => {
            "DIFFICULTY: L3 VERIFICATION - Ask what test or check would prove this works. \
             The developer should be thinking about how to verify their change is correct."
        }
        DifficultyLevel::L4Safety => {
            "DIFFICULTY: L4 SAFETY - Ask about failure modes, rollback, security, or edge cases. \
             What happens when things go wrong? What assumptions could break?"
        }
    };

    let risk_note = match risk {
        RiskLevel::High => {
            "RISK: HIGH - This change touches sensitive areas (auth, APIs, dependencies, \
             infrastructure, or database). Weight your question toward the blast radius."
        }
        RiskLevel::Medium => {
            "RISK: MEDIUM - This change involves error handling, concurrency, caching, or \
             parsing. Consider subtle correctness issues."
        }
        RiskLevel::Low => "RISK: LOW - Routine change. Keep the question proportional.",
    };

    format!(
        "{}\n\n{}\n\n{}",
        mode_instruction, difficulty_instruction, risk_note
    )
}

fn mode_after_answer(mode: &Mode) -> &'static str {
    match mode {
        Mode::VibeCoder => {
            "AFTER ANSWER: Keep it brief and encouraging. 2-3 sentences explaining the correct answer. No lecturing."
        }
        Mode::Developer => {
            "AFTER ANSWER: Be direct. Explain what the correct behavior is and why. If wrong, explain the gap in understanding."
        }
        Mode::Hardcore => {
            "AFTER ANSWER: If correct, suggest one follow-up risk to think about. If wrong, explain the failure mode they missed and its real-world impact."
        }
        Mode::Learning => {
            "AFTER ANSWER: Explain the concept with a concrete example from the diff. If wrong, walk through the reasoning step by step. Encourage progress."
        }
    }
}

fn mode_question_formulas(mode: &Mode) -> &'static str {
    match mode {
        Mode::VibeCoder => {
            r#"QUESTION FORMULA - pick one:
  * "What does your app do differently now?"
  * "A user opens [feature]. What do they see that's new?"
  * "Before this change [X happened]. What happens now?"
  * "You just shipped this. What's the one thing users will notice?"
Keep it casual. One obvious correct answer, three clearly wrong."#
        }
        Mode::Developer => {
            r#"QUESTION FORMULA - pick one:
  * "What was the intent behind this change?"
  * "If this change has a bug, what's the most likely symptom a user would report?"
  * "This change assumes [X]. What breaks if that assumption is wrong?"
  * "What would you check first to verify this works correctly in production?"
Wrong answers should be plausible misreadings of the intent."#
        }
        Mode::Hardcore => {
            r#"QUESTION FORMULA - pick one:
  * "What's the worst thing that happens if this change fails silently?"
  * "This change introduces a new failure mode. What is it?"
  * "Under what realistic conditions does this change produce incorrect results?"
  * "What data could be lost or corrupted if this change races with [concurrent operation]?"
  * "If this change shipped broken, what's the first thing users would notice?"
Every wrong answer should be a real concern that just doesn't apply to THIS specific change."#
        }
        Mode::Learning => {
            r#"QUESTION FORMULA - pick one:
  * "This change solves a specific problem. What problem?"
  * "Why was the old behavior insufficient? What did users experience before?"
  * "What would go wrong for users if you reverted this change?"
  * "What's the user-facing difference between the old and new behavior?"
Wrong answers should represent common misconceptions. After answering, you'll get an explanation."#
        }
    }
}

pub(crate) struct QuizContext {
    pub(crate) raw_diff: String,
    pub(crate) diff: String,
    pub(crate) files: Vec<String>,
    pub(crate) commit_msg: String,
    pub(crate) non_diff_files: Vec<String>,
}

pub(crate) fn detect_primary_category(summary: &DiffSummary) -> &'static str {
    if summary.has_security_changes {
        return "security";
    }
    if summary.has_migration_changes {
        return "migrations";
    }
    if summary.has_api_changes {
        return "api";
    }
    if summary.has_dependency_changes || summary.has_infra_changes {
        return "infrastructure";
    }
    if summary.has_concurrency_changes {
        return "concurrency";
    }
    if summary.has_error_handling_changes {
        return "error_handling";
    }
    if summary.has_parsing_changes {
        return "parsing";
    }
    if summary.has_cache_changes {
        return "caching";
    }
    "general"
}

pub(crate) fn detect_non_diff_files(files: &[String], raw_diff: &str) -> Vec<String> {
    files
        .iter()
        .filter(|f| crate::diff::is_binary_file(f) || !raw_diff.contains(&format!("b/{}", f)))
        .cloned()
        .collect()
}

pub(crate) fn collect_working_context(
    dir: &Path,
    max_chars: usize,
) -> Result<QuizContext, Box<dyn std::error::Error>> {
    use std::thread;

    let d1 = dir.to_path_buf();
    let d2 = dir.to_path_buf();
    let d3 = dir.to_path_buf();
    let d4 = dir.to_path_buf();

    let h1 = thread::spawn(move || git_cmd_with_timeout(&d1, &["diff", "--unified=3"], 3));
    let h2 =
        thread::spawn(move || git_cmd_with_timeout(&d2, &["diff", "--staged", "--unified=3"], 3));
    let h3 = thread::spawn(move || git_cmd_with_timeout(&d3, &["diff", "--name-only"], 3));
    let h4 =
        thread::spawn(move || git_cmd_with_timeout(&d4, &["diff", "--staged", "--name-only"], 3));

    let unstaged_diff = h1.join().map_err(|_| "thread panic")??;
    let staged_diff = h2.join().map_err(|_| "thread panic")??;
    let unstaged_files = h3.join().map_err(|_| "thread panic")??;
    let staged_files = h4.join().map_err(|_| "thread panic")??;

    let raw_diff = format!("{}{}", unstaged_diff, staged_diff);
    let files = dedup_file_list(&unstaged_files, &staged_files);
    let non_diff_files = detect_non_diff_files(&files, &raw_diff);

    Ok(QuizContext {
        diff: truncate_diff(&raw_diff, max_chars),
        raw_diff,
        files,
        commit_msg: String::new(),
        non_diff_files,
    })
}

pub(crate) fn collect_commit_context(
    dir: &Path,
    max_chars: usize,
) -> Result<QuizContext, Box<dyn std::error::Error>> {
    use std::thread;

    let has_parent = git_cmd(dir, &["rev-parse", "--verify", "HEAD~1"]).is_ok();

    let d1 = dir.to_path_buf();
    let d2 = dir.to_path_buf();
    let d3 = dir.to_path_buf();

    let diff_args: Vec<String> = if has_parent {
        vec!["diff".into(), "HEAD~1..HEAD".into(), "--unified=3".into()]
    } else {
        vec![
            "show".into(),
            "--format=".into(),
            "--unified=3".into(),
            "HEAD".into(),
        ]
    };
    let files_args: Vec<String> = if has_parent {
        vec!["diff".into(), "HEAD~1..HEAD".into(), "--name-only".into()]
    } else {
        vec![
            "show".into(),
            "--format=".into(),
            "--name-only".into(),
            "HEAD".into(),
        ]
    };

    let h1 = thread::spawn(move || {
        let args: Vec<&str> = diff_args.iter().map(|s| s.as_str()).collect();
        git_cmd_with_timeout(&d1, &args, 3)
    });
    let h2 = thread::spawn(move || {
        let args: Vec<&str> = files_args.iter().map(|s| s.as_str()).collect();
        git_cmd_with_timeout(&d2, &args, 3)
    });
    let h3 = thread::spawn(move || git_cmd_with_timeout(&d3, &["log", "-1", "--pretty=%s"], 3));

    let raw_diff = h1.join().map_err(|_| "thread panic")??;
    let files_raw = h2.join().map_err(|_| "thread panic")??;
    let commit_msg = h3.join().map_err(|_| "thread panic")??;

    let files: Vec<String> = files_raw
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .take(10)
        .collect();
    let non_diff_files = detect_non_diff_files(&files, &raw_diff);

    Ok(QuizContext {
        diff: truncate_diff(&raw_diff, max_chars),
        raw_diff,
        files,
        commit_msg,
        non_diff_files,
    })
}

pub(crate) fn output_quiz_context(
    ctx: &QuizContext,
    summary: &DiffSummary,
    config: &Config,
    risk: RiskLevel,
    state: &State,
) {
    let mode = config.mode.clone().unwrap_or_default();
    let difficulty = resolve_difficulty(&mode, risk, config.difficulty.as_deref(), state);

    println!("# VibeCheck\n");
    println!(
        "Mode: {} | Risk: {} | Difficulty: {}\n",
        mode, risk, difficulty
    );

    if !ctx.commit_msg.is_empty() {
        println!("Commit: {}\n", ctx.commit_msg.trim());
    }

    if !ctx.files.is_empty() {
        println!("Changed files: {}\n", ctx.files.join(", "));
    }

    if !ctx.non_diff_files.is_empty() {
        println!(
            "Also changed (no diff available): {}\n",
            ctx.non_diff_files.join(", ")
        );
    }

    let mut notes = Vec::new();
    if !summary.functions_added.is_empty() {
        if summary.functions_added.len() <= 3 {
            notes.push(format!("New: {}", summary.functions_added.join(", ")));
        } else {
            notes.push(format!("{} functions added", summary.functions_added.len()));
        }
    }
    if !summary.functions_removed.is_empty() {
        if summary.functions_removed.len() <= 3 {
            notes.push(format!("Removed: {}", summary.functions_removed.join(", ")));
        } else {
            notes.push(format!(
                "{} functions removed",
                summary.functions_removed.len()
            ));
        }
    }
    if summary.has_error_handling_changes {
        notes.push("Error handling changed".to_string());
    }
    if summary.has_security_changes {
        notes.push("Security-related changes".to_string());
    }
    if summary.has_api_changes {
        notes.push("API/routing changes".to_string());
    }
    notes.push(format!(
        "+{} -{} lines",
        summary.lines_added, summary.lines_removed
    ));

    println!("Summary:");
    for note in &notes {
        println!("  - {}", note);
    }
    println!();

    println!("```diff");
    println!("{}", ctx.diff);
    println!("```\n");

    let mode_instructions = mode_prompt(&mode, difficulty, risk);
    let formulas = mode_question_formulas(&mode);
    println!("## Quiz Instructions\n");
    println!("{}\n", mode_instructions);
    println!("Create ONE multiple-choice question (A/B/C/D) about this diff.\n");
    println!("{}\n", formulas);

    if risk == RiskLevel::High {
        println!(
            "NOTE: HIGH RISK change. Consider asking a follow-up question after the first answer to verify operational understanding.\n"
        );
    }

    println!("---\n");
    println!("Pipe this to your AI tool:");
    println!("  vibecheck quiz | pbcopy        # copy to clipboard");
    println!("  vibecheck quiz | llm           # pipe to LLM CLI");
    println!("  vibecheck quiz > .quiz.md      # save for your AI to read");
}

pub(crate) fn output_explain_context(
    ctx: &QuizContext,
    summary: &DiffSummary,
    config: &Config,
    risk: RiskLevel,
) {
    let mode = config.mode.clone().unwrap_or_default();

    println!("# VibeCheck Explain\n");
    println!("Mode: {} | Risk: {}\n", mode, risk);

    if !ctx.commit_msg.is_empty() {
        println!("Commit: {}\n", ctx.commit_msg.trim());
    }

    if !ctx.files.is_empty() {
        println!("Changed files: {}\n", ctx.files.join(", "));
    }

    if !ctx.non_diff_files.is_empty() {
        println!(
            "Also changed (no diff available): {}\n",
            ctx.non_diff_files.join(", ")
        );
    }

    let mut notes = Vec::new();
    if !summary.functions_added.is_empty() {
        if summary.functions_added.len() <= 3 {
            notes.push(format!("New: {}", summary.functions_added.join(", ")));
        } else {
            notes.push(format!("{} functions added", summary.functions_added.len()));
        }
    }
    if !summary.functions_removed.is_empty() {
        if summary.functions_removed.len() <= 3 {
            notes.push(format!("Removed: {}", summary.functions_removed.join(", ")));
        } else {
            notes.push(format!(
                "{} functions removed",
                summary.functions_removed.len()
            ));
        }
    }
    notes.push(format!(
        "+{} -{} lines",
        summary.lines_added, summary.lines_removed
    ));

    println!("Summary:");
    for note in &notes {
        println!("  - {}", note);
    }
    println!();

    println!("```diff");
    println!("{}", ctx.diff);
    println!("```\n");

    println!("## Explain These Changes\n");
    println!("Analyze the diff above and explain:\n");
    println!("1. **What changed** - In plain language, what does this code do differently now?");
    println!("2. **Why it matters** - What user-visible or system behavior does this affect?");
    println!(
        "3. **Risk surface** - Given the risk level ({}), what should the developer watch for?",
        risk
    );
    if !ctx.non_diff_files.is_empty() {
        println!(
            "4. **Non-diff files** - {} changed but have no diff. Note any implications.",
            ctx.non_diff_files.join(", ")
        );
    }
    println!("\nBe concise. Use concrete terms from the code. No quiz question.\n");
    println!("---\n");
    println!("Pipe this to your AI tool:");
    println!("  vibecheck explain | pbcopy     # copy to clipboard");
    println!("  vibecheck explain | llm        # pipe to LLM CLI");
}

pub(crate) fn output_ci_context(
    ctx: &QuizContext,
    summary: &DiffSummary,
    config: &Config,
    risk: RiskLevel,
    base_ref: &str,
    head_ref: &str,
) {
    let mode = config.mode.clone().unwrap_or_default();
    let state = crate::config::State::default();
    let difficulty = resolve_difficulty(&mode, risk, config.difficulty.as_deref(), &state);

    println!("## VibeCheck\n");
    println!(
        "**Diff**: `{}..{}` | **Risk**: {} | **Mode**: {} | **Difficulty**: {}\n",
        base_ref, head_ref, risk, mode, difficulty
    );

    if !ctx.files.is_empty() {
        println!("**Changed files**: {}\n", ctx.files.join(", "));
    }

    if !ctx.non_diff_files.is_empty() {
        println!(
            "**Also changed (no diff)**: {}\n",
            ctx.non_diff_files.join(", ")
        );
    }

    let mut notes = Vec::new();
    if !summary.functions_added.is_empty() {
        if summary.functions_added.len() <= 3 {
            notes.push(format!("New: {}", summary.functions_added.join(", ")));
        } else {
            notes.push(format!("{} functions added", summary.functions_added.len()));
        }
    }
    if !summary.functions_removed.is_empty() {
        if summary.functions_removed.len() <= 3 {
            notes.push(format!("Removed: {}", summary.functions_removed.join(", ")));
        } else {
            notes.push(format!(
                "{} functions removed",
                summary.functions_removed.len()
            ));
        }
    }
    if summary.has_error_handling_changes {
        notes.push("Error handling changed".to_string());
    }
    if summary.has_security_changes {
        notes.push("Security-related changes".to_string());
    }
    if summary.has_api_changes {
        notes.push("API/routing changes".to_string());
    }
    notes.push(format!(
        "+{} -{} lines",
        summary.lines_added, summary.lines_removed
    ));

    println!("<details>");
    println!("<summary>Diff summary ({})</summary>\n", notes.join(" | "));
    println!("```diff");
    println!("{}", ctx.diff);
    println!("```\n");
    println!("</details>\n");

    let mode_instructions = mode_prompt(&mode, difficulty, risk);
    let formulas = mode_question_formulas(&mode);
    println!("### Quiz Instructions\n");
    println!("{}\n", mode_instructions);
    println!("Create ONE multiple-choice question (A/B/C/D) about the most impactful change in this diff.\n");
    println!("{}\n", formulas);

    if risk == RiskLevel::High {
        println!("**HIGH RISK**: Consider asking a follow-up question after the first answer.\n");
    }

    println!("---");
    println!(
        "*Generated by [vibecheck](https://github.com/akshan-main/vibe-check). The PR author should answer this question before merge.*"
    );
}

pub(crate) fn build_explain_reason(
    files_line: &str,
    diff_snippet: &str,
    non_diff_files: &[String],
    state_path: &str,
    config_path: &str,
    mode: &Mode,
    risk: RiskLevel,
) -> String {
    let non_diff_section = if non_diff_files.is_empty() {
        String::new()
    } else {
        format!(
            "\nAlso changed (binary/no diff): {}",
            non_diff_files.join(", ")
        )
    };

    format!(
        r#"You just finished the main task. Now give a quick VibeCheck explanation.

IMPORTANT RULES:
- Do NOT use Edit, Write, or any code-modifying tools on PROJECT files. This is learning-only.
- Keep it quick for the user.
- You MAY use Bash ONLY for the specific commands shown below (snooze, disable).

STEP 1: Use AskUserQuestion to ask:
  question: "VibeCheck: quick walkthrough of what just changed?"
  header: "VibeCheck"
  options:
    - label: "Yes", description: "A quick explanation of your changes"
    - label: "No thanks", description: "Skip this time"
    - label: "Snooze 30m", description: "Don't ask again for 30 minutes"
    - label: "Disable", description: "Turn off VibeCheck for this repo"
  multiSelect: false

STEP 2: Handle the response:
- If "No thanks": say "Got it, skipping." then end with [vibecheck:done]
- If "Snooze 30m": Run this Bash command to persist the snooze, then say "Snoozed for 30 minutes." and end with [vibecheck:done]:
  python3 -c "import json,time,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()) if p.exists() else {{}}; d['snoozed_until']=int(time.time())+1800; p.write_text(json.dumps(d,indent=2))" '{state_path}'
- If "Disable": Run this Bash command to persist the disable, then say "VibeCheck disabled. Re-enable by setting enabled:true in .claude/vibecheck.json" and end with [vibecheck:done]:
  python3 -c "import json,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()) if p.exists() else {{}}; d['enabled']=False; p.write_text(json.dumps(d,indent=2))" '{config_path}'
- If "Yes": continue to STEP 3.

STEP 3: Explain the changes in plain language. Cover:
1. What changed - what does the code do differently now?
2. Why it matters - what user-visible or system behavior is affected?
3. Risk surface - given risk level {risk}, what edge cases or failure modes should the developer be aware of?

MODE: {mode} - match your tone to this mode (casual for vibe_coder, direct for developer, thorough for hardcore, educational for learning).

Keep it to 3-5 sentences. No quiz question. No multiple choice.

Then end your message with: [vibecheck:done]

CHANGE CONTEXT:
Changed files: {files_line}{non_diff_section}

Diff:
{diff_snippet}"#,
        state_path = state_path,
        config_path = config_path,
        risk = risk,
        mode = mode,
        files_line = files_line,
        non_diff_section = non_diff_section,
        diff_snippet = diff_snippet
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_reason(
    files_line: &str,
    diff_snippet: &str,
    non_diff_files: &[String],
    state_path: &str,
    config_path: &str,
    mode: &Mode,
    risk: RiskLevel,
    difficulty_override: Option<&str>,
    track_progress: bool,
    state: &State,
    team_member_path: Option<&str>,
    category: &str,
    categories_path: &str,
    weak_area_hint: Option<&str>,
) -> String {
    let difficulty = resolve_difficulty(mode, risk, difficulty_override, state);
    let mode_instructions = mode_prompt(mode, difficulty, risk);
    let after_answer = mode_after_answer(mode);
    let question_formulas = mode_question_formulas(mode);

    let weak_area_section = match weak_area_hint {
        Some(hint) => format!("\nWEAK AREA CONTEXT: {}\n", hint),
        None => String::new(),
    };

    let tracking_section = if track_progress {
        format!(
            "\nPROGRESS TRACKING: After the quiz is done (right before [vibecheck:done]), run this Bash command to update stats:\npython3 -c \"import json,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()) if p.exists() else {{}}; d['total_quizzes']=d.get('total_quizzes',0)+1; correct=sys.argv[2]=='1'; d['total_correct']=d.get('total_correct',0)+(1 if correct else 0); d['streak']=(d.get('streak',0)+1) if correct else 0; p.write_text(json.dumps(d,indent=2))\" '{}' {{CORRECT}}\nReplace {{CORRECT}} with 1 if the user answered correctly, 0 if wrong.\nThen show: \"Stats: {{total_correct}}/{{total_quizzes}} correct (streak: {{streak}})\"\nCurrent stats: {}/{} correct, streak: {}\n",
            state_path, state.total_correct, state.total_quizzes, state.streak
        )
    } else {
        String::new()
    };

    let category_tracking = if track_progress {
        format!(
            "\nCATEGORY TRACKING: Also update category stats by running this Bash command:\npython3 -c \"import json,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()) if p.exists() else {{}}; cat=sys.argv[2]; entry=d.get(cat,[0,0]); entry[0]+=1; entry[1]+=(1 if sys.argv[3]=='1' else 0); d[cat]=entry; p.write_text(json.dumps(d,indent=2))\" '{}' '{}' {{CORRECT}}\n",
            categories_path, category
        )
    } else {
        String::new()
    };

    let non_diff_section = if non_diff_files.is_empty() {
        String::new()
    } else {
        format!(
            "\nAlso changed (binary/no diff): {}",
            non_diff_files.join(", ")
        )
    };

    let team_section = if let Some(member_path) = team_member_path {
        format!(
            "\nTEAM MODE: After updating personal stats (or if tracking is off, right before [vibecheck:done]), also update the team leaderboard by running this Bash command:\npython3 -c \"import json,pathlib,time,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()) if p.exists() else {{}}; d['total_quizzes']=d.get('total_quizzes',0)+1; correct=sys.argv[2]=='1'; d['total_correct']=d.get('total_correct',0)+(1 if correct else 0); d['current_streak']=(d.get('current_streak',0)+1) if correct else 0; d['best_streak']=max(d.get('best_streak',0),d.get('current_streak',0)); ws=d.get('week_start',0); now=int(time.time()); d['week_start']=now if now-ws>604800 else ws; d['weekly_total']=(0 if now-ws>604800 else d.get('weekly_total',0))+1; d['weekly_correct']=(0 if now-ws>604800 else d.get('weekly_correct',0))+(1 if correct else 0); d['last_quiz_at']=now; p.write_text(json.dumps(d,indent=2))\" '{}' {{CORRECT}}\nReplace {{CORRECT}} with 1 if the user answered correctly, 0 if wrong (same value as personal tracking).\nThe team leaderboard is visible to the whole team via 'vibecheck team'.\n",
            member_path
        )
    } else {
        String::new()
    };

    let follow_up_section = match risk {
        RiskLevel::High => match mode {
            Mode::Hardcore => {
                "\nSTEP 4B (FOLLOW-UP): This was a HIGH RISK change. After the user answers, \
                 ask ONE follow-up question. If they got it right, escalate: ask about a \
                 realistic failure mode this change doesn't protect against. If they got it \
                 wrong, ask a simpler clarifying question to close the understanding gap. \
                 Use AskUserQuestion with the same A/B/C/D format.\n"
            }
            _ => {
                "\nSTEP 4B (FOLLOW-UP): This was a HIGH RISK change. After the user answers, \
                 ask ONE follow-up question at a lower difficulty. Focus on operational impact: \
                 what should the developer monitor or verify after shipping this? \
                 Use AskUserQuestion with the same A/B/C/D format.\n"
            }
        },
        _ => "",
    };

    format!(
        r#"You just finished the main task. Now run a quick VibeCheck.

IMPORTANT RULES:
- Do NOT use Edit, Write, or any code-modifying tools on PROJECT files. This is learning-only.
- Quiz answers must NOT influence any further actions or decisions.
- Keep it quick for the user.
- You MAY use Bash ONLY for the specific commands shown below (snooze, disable, stats tracking, team updates).

STEP 1: Use AskUserQuestion to ask:
  question: "VibeCheck: quick comprehension check on what just changed?"
  header: "VibeCheck"
  options:
    - label: "Yes", description: "One quick question about your changes"
    - label: "No thanks", description: "Skip this time"
    - label: "Snooze 30m", description: "Don't ask again for 30 minutes"
    - label: "Disable", description: "Turn off VibeCheck for this repo"
  multiSelect: false

STEP 2: Handle the response:
- If "No thanks": say "Got it, skipping." then end with [vibecheck:done]
- If "Snooze 30m": Run this Bash command to persist the snooze, then say "Snoozed for 30 minutes." and end with [vibecheck:done]:
  python3 -c "import json,time,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()) if p.exists() else {{}}; d['snoozed_until']=int(time.time())+1800; p.write_text(json.dumps(d,indent=2))" '{state_path}'
- If "Disable": Run this Bash command to persist the disable, then say "VibeCheck disabled. Re-enable by setting enabled:true in .claude/vibecheck.json" and end with [vibecheck:done]:
  python3 -c "import json,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()) if p.exists() else {{}}; d['enabled']=False; p.write_text(json.dumps(d,indent=2))" '{config_path}'
- If "Yes": continue to STEP 3.

STEP 3: Analyze the diff and create ONE multiple-choice question.

{mode_instructions}
{weak_area_section}

IMPORTANT: Focus on the MOST IMPORTANT change, not the largest one. A 2-line behavior change can matter more than 50 lines of boilerplate. Read the entire diff, use your understanding of the code and the product, and pick the single change that has the most meaningful impact on what users experience. Ignore formatting, imports, renaming, and refactors that don't change behavior.

Classify what happened:
- Was a feature ADDED? (new capability that didn't exist before)
- Was a feature CHANGED? (existing behavior now works differently)
- Was something REMOVED? (capability or safeguard that's now gone)
- Was it a FIX? (broken thing that now works)

Then ask a question that tests whether the developer understands the REAL-WORLD IMPACT of this specific change on their product.

{question_formulas}

NEVER ASK:
  * About code syntax, language features, or programming concepts
  * About which library or framework is used
  * Anything a developer would need to read code to answer
  * Generic questions unrelated to this specific diff

Format: exactly 4 options (labels "A", "B", "C", "D"), one correct. Ask via AskUserQuestion with header: "VibeCheck", multiSelect: false.

STEP 4: After the user answers:
{after_answer}
1. Explain the correct answer in plain language - what the product does now and why
2. If wrong: explain what they misunderstood about the change and what their answer would have meant for users
3. PROMPTING TIP: You have the full conversation context - you know what the user asked for and what you built. Compare those. If their prompt was vague and the implementation has gaps or surprises they might not expect, suggest a more specific prompt that would have covered those gaps. If their prompt was already detailed and the implementation matches well, say so. Don't fabricate issues.
{follow_up_section}
Then end your message with: [vibecheck:done]
{tracking_section}{category_tracking}{team_section}
CHANGE CONTEXT:
Changed files: {files_line}{non_diff_section}

Diff:
{diff_snippet}"#,
        files_line = files_line,
        diff_snippet = diff_snippet,
        non_diff_section = non_diff_section,
        state_path = state_path,
        config_path = config_path,
        mode_instructions = mode_instructions,
        question_formulas = question_formulas,
        after_answer = after_answer,
        tracking_section = tracking_section,
        category_tracking = category_tracking,
        weak_area_section = weak_area_section,
        follow_up_section = follow_up_section,
        team_section = team_section
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_reason_includes_diff() {
        let state = State::default();
        let reason = build_reason(
            "file.rs",
            "diff content",
            &[],
            "/tmp/state",
            "/tmp/config",
            &Mode::VibeCoder,
            RiskLevel::Low,
            None,
            false,
            &state,
            None,
            "general",
            "/tmp/cats",
            None,
        );
        assert!(reason.contains("diff content"));
        assert!(reason.contains("file.rs"));
    }

    #[test]
    fn build_reason_vibe_coder_mode() {
        let state = State::default();
        let reason = build_reason(
            "file.rs",
            "diff",
            &[],
            "/tmp/s",
            "/tmp/c",
            &Mode::VibeCoder,
            RiskLevel::Low,
            None,
            false,
            &state,
            None,
            "general",
            "/tmp/cats",
            None,
        );
        assert!(reason.contains("VIBE CODER"));
        assert!(reason.contains("L1 RECALL"));
    }

    #[test]
    fn build_reason_hardcore_mode() {
        let state = State::default();
        let reason = build_reason(
            "file.rs",
            "diff",
            &[],
            "/tmp/s",
            "/tmp/c",
            &Mode::Hardcore,
            RiskLevel::High,
            None,
            false,
            &state,
            None,
            "general",
            "/tmp/cats",
            None,
        );
        assert!(reason.contains("HARDCORE"));
        assert!(reason.contains("L4 SAFETY"));
    }

    #[test]
    fn build_reason_difficulty_override() {
        let state = State::default();
        let reason = build_reason(
            "file.rs",
            "diff",
            &[],
            "/tmp/s",
            "/tmp/c",
            &Mode::VibeCoder,
            RiskLevel::Low,
            Some("advanced"),
            false,
            &state,
            None,
            "general",
            "/tmp/cats",
            None,
        );
        assert!(reason.contains("L4 SAFETY"));
    }

    #[test]
    fn build_reason_with_tracking() {
        let state = State {
            total_quizzes: 5,
            total_correct: 3,
            streak: 2,
            ..Default::default()
        };
        let reason = build_reason(
            "file.rs",
            "diff",
            &[],
            "/tmp/s",
            "/tmp/c",
            &Mode::Developer,
            RiskLevel::Medium,
            None,
            true,
            &state,
            None,
            "general",
            "/tmp/cats",
            None,
        );
        assert!(reason.contains("PROGRESS TRACKING"));
        assert!(reason.contains("3/5"));
    }

    #[test]
    fn build_reason_with_team() {
        let state = State::default();
        let reason = build_reason(
            "file.rs",
            "diff",
            &[],
            "/tmp/s",
            "/tmp/c",
            &Mode::VibeCoder,
            RiskLevel::Low,
            None,
            false,
            &state,
            Some("/tmp/team/member.json"),
            "general",
            "/tmp/cats",
            None,
        );
        assert!(reason.contains("TEAM MODE"));
        assert!(reason.contains("/tmp/team/member.json"));
    }

    #[test]
    fn resolve_difficulty_vibe_coder_always_l1() {
        let state = State::default();
        assert_eq!(
            resolve_difficulty(&Mode::VibeCoder, RiskLevel::High, None, &state),
            DifficultyLevel::L1Recall
        );
        assert_eq!(
            resolve_difficulty(&Mode::VibeCoder, RiskLevel::Low, None, &state),
            DifficultyLevel::L1Recall
        );
    }

    #[test]
    fn resolve_difficulty_developer_risk_driven() {
        let state = State::default();
        assert_eq!(
            resolve_difficulty(&Mode::Developer, RiskLevel::Low, None, &state),
            DifficultyLevel::L2Comprehension
        );
        assert_eq!(
            resolve_difficulty(&Mode::Developer, RiskLevel::High, None, &state),
            DifficultyLevel::L3Verification
        );
    }

    #[test]
    fn resolve_difficulty_hardcore_always_l4() {
        let state = State::default();
        assert_eq!(
            resolve_difficulty(&Mode::Hardcore, RiskLevel::Low, None, &state),
            DifficultyLevel::L4Safety
        );
    }

    #[test]
    fn resolve_difficulty_learning_adaptive() {
        // No quizzes yet - L1
        let state = State::default();
        assert_eq!(
            resolve_difficulty(&Mode::Learning, RiskLevel::Low, None, &state),
            DifficultyLevel::L1Recall
        );

        // Low accuracy - L1
        let state = State {
            total_quizzes: 10,
            total_correct: 3,
            ..Default::default()
        };
        assert_eq!(
            resolve_difficulty(&Mode::Learning, RiskLevel::Low, None, &state),
            DifficultyLevel::L1Recall
        );

        // Medium accuracy - L2
        let state = State {
            total_quizzes: 10,
            total_correct: 5,
            ..Default::default()
        };
        assert_eq!(
            resolve_difficulty(&Mode::Learning, RiskLevel::Low, None, &state),
            DifficultyLevel::L2Comprehension
        );

        // Good accuracy - L3
        let state = State {
            total_quizzes: 10,
            total_correct: 7,
            ..Default::default()
        };
        assert_eq!(
            resolve_difficulty(&Mode::Learning, RiskLevel::Low, None, &state),
            DifficultyLevel::L3Verification
        );

        // High accuracy - L4
        let state = State {
            total_quizzes: 10,
            total_correct: 9,
            ..Default::default()
        };
        assert_eq!(
            resolve_difficulty(&Mode::Learning, RiskLevel::Low, None, &state),
            DifficultyLevel::L4Safety
        );
    }

    #[test]
    fn resolve_difficulty_override_wins() {
        let state = State::default();
        assert_eq!(
            resolve_difficulty(&Mode::VibeCoder, RiskLevel::Low, Some("advanced"), &state),
            DifficultyLevel::L4Safety
        );
        assert_eq!(
            resolve_difficulty(&Mode::Hardcore, RiskLevel::High, Some("beginner"), &state),
            DifficultyLevel::L1Recall
        );
    }
}
