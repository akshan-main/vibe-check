use crate::config::{load_config, load_state, save_state};
use crate::team::{get_team_context, TeamMember};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct RecordResult {
    pub(crate) total_quizzes: u64,
    pub(crate) total_correct: u64,
    pub(crate) streak: u64,
    pub(crate) category: Option<String>,
}

pub(crate) fn record_answer(
    project_dir: &Path,
    correct: bool,
    category: Option<&str>,
) -> Result<RecordResult, Box<dyn std::error::Error>> {
    let config = load_config(project_dir);
    if !config.track_progress.unwrap_or(false) {
        return Err(
            "tracking disabled. Set \"trackProgress\": true in .claude/vibecheck.json".into(),
        );
    }

    let state_dir = project_dir.join(".claude").join(".vibecheck");
    fs::create_dir_all(&state_dir)?;
    let state_path = state_dir.join("state.json");

    // Update personal stats
    let mut state = load_state(&state_path);
    state.total_quizzes += 1;
    if correct {
        state.total_correct += 1;
        state.streak += 1;
    } else {
        state.streak = 0;
    }
    save_state(&state_path, &state);

    // Update category stats
    if let Some(cat) = category {
        let categories_path = state_dir.join("categories.json");
        let mut map: serde_json::Map<String, serde_json::Value> = if categories_path.exists() {
            let content = fs::read_to_string(&categories_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            serde_json::Map::new()
        };

        let entry = map
            .entry(cat.to_string())
            .or_insert_with(|| serde_json::json!([0, 0]));
        if let Some(arr) = entry.as_array_mut() {
            if arr.len() >= 2 {
                let total = arr[0].as_u64().unwrap_or(0) + 1;
                let correct_count = arr[1].as_u64().unwrap_or(0) + if correct { 1 } else { 0 };
                arr[0] = serde_json::json!(total);
                arr[1] = serde_json::json!(correct_count);
            }
        }

        fs::write(&categories_path, serde_json::to_string_pretty(&map)?)?;
    }

    // Update team stats if team mode is active
    if let Some((email_hash, member_path)) = get_team_context(project_dir) {
        let content = fs::read_to_string(&member_path)?;
        let mut member: TeamMember = serde_json::from_str(&content)?;

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        member.total_quizzes += 1;
        if correct {
            member.total_correct += 1;
            member.current_streak += 1;
        } else {
            member.current_streak = 0;
        }
        member.best_streak = member.best_streak.max(member.current_streak);

        // Weekly stats with auto-reset
        let week_seconds: u64 = 7 * 24 * 3600;
        if now.saturating_sub(member.week_start) > week_seconds {
            member.weekly_correct = 0;
            member.weekly_total = 0;
            member.week_start = now;
        }
        member.weekly_total += 1;
        if correct {
            member.weekly_correct += 1;
        }
        member.last_quiz_at = now;

        // Update hash chain
        let correct_bit = if correct { "1" } else { "0" };
        let prev_hash = if member.chain_hash.is_empty() {
            "genesis"
        } else {
            &member.chain_hash
        };
        let chain_input = format!("{}:{}:{}:{}", prev_hash, correct_bit, now, email_hash);
        let mut hasher = Sha256::new();
        hasher.update(chain_input.as_bytes());
        member.chain_hash = format!("{:x}", hasher.finalize());
        member.chain_length += 1;

        fs::write(&member_path, serde_json::to_string_pretty(&member)?)?;
    }

    Ok(RecordResult {
        total_quizzes: state.total_quizzes,
        total_correct: state.total_correct,
        streak: state.streak,
        category: category.map(|c| c.to_string()),
    })
}

pub(crate) fn verify_chain(member: &TeamMember) -> bool {
    if member.chain_length == 0 {
        // No records yet - chain_hash should be empty or "genesis"
        return member.chain_hash.is_empty() || member.chain_hash == "genesis";
    }
    // If chain_length > 0, chain_hash must be a non-empty hex string (64 chars for SHA-256)
    if member.chain_hash.is_empty() || member.chain_hash == "genesis" {
        return false;
    }
    // Basic integrity: hash should be 64 hex chars
    member.chain_hash.len() == 64 && member.chain_hash.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verify_chain_genesis() {
        let member = TeamMember {
            chain_hash: String::new(),
            chain_length: 0,
            ..Default::default()
        };
        assert!(verify_chain(&member));

        let member = TeamMember {
            chain_hash: "genesis".to_string(),
            chain_length: 0,
            ..Default::default()
        };
        assert!(verify_chain(&member));
    }

    #[test]
    fn verify_chain_valid_hash() {
        let member = TeamMember {
            chain_hash: "a".repeat(64),
            chain_length: 5,
            ..Default::default()
        };
        assert!(verify_chain(&member));
    }

    #[test]
    fn verify_chain_tampered_empty() {
        let member = TeamMember {
            chain_hash: String::new(),
            chain_length: 3,
            ..Default::default()
        };
        assert!(!verify_chain(&member));
    }

    #[test]
    fn verify_chain_tampered_genesis_with_records() {
        let member = TeamMember {
            chain_hash: "genesis".to_string(),
            chain_length: 1,
            ..Default::default()
        };
        assert!(!verify_chain(&member));
    }

    #[test]
    fn verify_chain_tampered_bad_hash() {
        let member = TeamMember {
            chain_hash: "not_a_valid_hash".to_string(),
            chain_length: 2,
            ..Default::default()
        };
        assert!(!verify_chain(&member));
    }

    #[test]
    fn record_answer_personal_stats() {
        let dir = std::env::temp_dir().join("vibecheck_record_test");
        let _ = fs::remove_dir_all(&dir);
        let config_dir = dir.join(".claude");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("vibecheck.json"),
            r#"{"trackProgress": true}"#,
        )
        .unwrap();

        let result = record_answer(&dir, true, Some("security")).unwrap();
        assert_eq!(result.total_quizzes, 1);
        assert_eq!(result.total_correct, 1);
        assert_eq!(result.streak, 1);
        assert_eq!(result.category, Some("security".to_string()));

        // Check state.json
        let state = load_state(&dir.join(".claude").join(".vibecheck").join("state.json"));
        assert_eq!(state.total_quizzes, 1);
        assert_eq!(state.total_correct, 1);
        assert_eq!(state.streak, 1);

        // Check categories.json
        let cats: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(
                dir.join(".claude")
                    .join(".vibecheck")
                    .join("categories.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(cats["security"][0], 1);
        assert_eq!(cats["security"][1], 1);

        // Record a wrong answer
        let result = record_answer(&dir, false, Some("security")).unwrap();
        assert_eq!(result.total_quizzes, 2);
        assert_eq!(result.total_correct, 1);
        assert_eq!(result.streak, 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_answer_tracking_disabled() {
        let dir = std::env::temp_dir().join("vibecheck_record_disabled_test");
        let _ = fs::remove_dir_all(&dir);
        let config_dir = dir.join(".claude");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("vibecheck.json"),
            r#"{"trackProgress": false}"#,
        )
        .unwrap();

        let result = record_answer(&dir, true, None);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
