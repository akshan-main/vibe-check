use crate::git::git_cmd;
use crate::stats::verify_chain;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
pub(crate) struct TeamConfig {
    pub(crate) name: String,
    pub(crate) created_at: u64,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub(crate) struct TeamMember {
    pub(crate) name: String,
    pub(crate) email_hash: String,
    pub(crate) total_quizzes: u64,
    pub(crate) total_correct: u64,
    pub(crate) current_streak: u64,
    pub(crate) best_streak: u64,
    pub(crate) weekly_correct: u64,
    pub(crate) weekly_total: u64,
    pub(crate) week_start: u64,
    pub(crate) last_quiz_at: u64,
    pub(crate) joined_at: u64,
    #[serde(default)]
    pub(crate) chain_hash: String,
    #[serde(default)]
    pub(crate) chain_length: u64,
}

pub(crate) fn run_team(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = crate::git::find_git_root()?;
    let team_dir = project_dir.join(".vibecheck-team");

    let subcommand = args.first().map(|s| s.as_str()).unwrap_or("");

    match subcommand {
        "init" => team_init(&team_dir, args),
        "join" => team_join(&team_dir, &project_dir, args),
        "reset" => team_reset(&team_dir, &project_dir),
        "" | "stats" => team_stats(&team_dir),
        other => {
            eprintln!("unknown team command: {}", other);
            eprintln!("usage: vibecheck team [init|join|stats|reset]");
            std::process::exit(1);
        }
    }
}

pub(crate) fn team_init(
    team_dir: &Path,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if team_dir.join("team.json").exists() {
        println!("Team already initialized in .vibecheck-team/");
        println!("Run 'vibecheck team join' to register yourself.");
        return Ok(());
    }

    let team_name =
        parse_flag_value(args, "--name").unwrap_or_else(|| "VibeCheck Team".to_string());

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    fs::create_dir_all(team_dir.join("members"))?;

    let config = TeamConfig {
        name: team_name.clone(),
        created_at: now,
    };

    fs::write(
        team_dir.join("team.json"),
        serde_json::to_string_pretty(&config)?,
    )?;

    println!("Team '{}' initialized.", team_name);
    println!("Directory: .vibecheck-team/");
    println!();
    println!("Next steps:");
    println!("  1. Run 'vibecheck team join' to register yourself");
    println!("  2. Commit .vibecheck-team/ to git so your team can see it");
    println!("  3. Each team member runs 'vibecheck team join'");
    println!("  4. Enable progress tracking in .claude/vibecheck.json:");
    println!("     {{\"trackProgress\": true}}");
    println!();
    println!("View leaderboard: vibecheck team");

    Ok(())
}

fn team_join(
    team_dir: &Path,
    project_dir: &Path,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if !team_dir.join("team.json").exists() {
        eprintln!("No team found. Run 'vibecheck team init' first.");
        std::process::exit(1);
    }

    let git_email = git_cmd(project_dir, &["config", "user.email"])?;
    let email = git_email.trim();
    if email.is_empty() {
        eprintln!(
            "Could not detect git user.email. Set it with: git config user.email you@example.com"
        );
        std::process::exit(1);
    }

    let email_hash = short_hash(email);
    let member_path = team_dir
        .join("members")
        .join(format!("{}.json", email_hash));

    if member_path.exists() {
        let existing: TeamMember = serde_json::from_str(&fs::read_to_string(&member_path)?)?;
        println!("Already on the team as '{}'.", existing.name);
        println!("View leaderboard: vibecheck team");
        return Ok(());
    }

    let display_name = parse_flag_value(args, "--name").unwrap_or_else(|| {
        git_cmd(project_dir, &["config", "user.name"])
            .map(|n| n.trim().to_string())
            .unwrap_or_else(|_| email.split('@').next().unwrap_or("unknown").to_string())
    });

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let member = TeamMember {
        name: display_name.clone(),
        email_hash: email_hash.clone(),
        week_start: now,
        joined_at: now,
        ..Default::default()
    };

    fs::create_dir_all(team_dir.join("members"))?;
    fs::write(&member_path, serde_json::to_string_pretty(&member)?)?;

    println!("Joined as '{}' ({})", display_name, email_hash);
    println!();
    println!("Make sure trackProgress is enabled in .claude/vibecheck.json:");
    println!("  {{\"trackProgress\": true}}");
    println!();
    println!("Your quiz results will appear on the team leaderboard.");
    println!("Commit .vibecheck-team/ to share with your team.");

    Ok(())
}

pub(crate) fn team_stats(team_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !team_dir.join("team.json").exists() {
        eprintln!("No team found. Run 'vibecheck team init' first.");
        std::process::exit(1);
    }

    let config: TeamConfig =
        serde_json::from_str(&fs::read_to_string(team_dir.join("team.json"))?)?;

    let members_dir = team_dir.join("members");
    let mut members = Vec::new();

    if members_dir.is_dir() {
        for entry in fs::read_dir(&members_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(member) = serde_json::from_str::<TeamMember>(&content) {
                        members.push(member);
                    }
                }
            }
        }
    }

    if members.is_empty() {
        println!("{}", config.name);
        println!();
        println!("No members yet. Run 'vibecheck team join' to register.");
        return Ok(());
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let week_seconds: u64 = 7 * 24 * 3600;

    let mut scored: Vec<(f64, TeamMember)> = members
        .into_iter()
        .map(|mut m| {
            if now.saturating_sub(m.week_start) > week_seconds {
                m.weekly_correct = 0;
                m.weekly_total = 0;
                m.week_start = now;
            }
            let score = if m.total_quizzes > 0 {
                (m.total_correct as f64 / m.total_quizzes as f64) * 100.0
            } else {
                0.0
            };
            (score, m)
        })
        .collect();

    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.total_quizzes.cmp(&a.1.total_quizzes))
    });

    println!("{}", config.name);
    println!("{}", "=".repeat(config.name.len().max(44)));
    println!();

    println!(
        " {:<3} {:<14} {:>6} {:>8} {:>11}",
        "#", "Name", "Score", "Streak", "This Week"
    );
    println!(" {}", "-".repeat(46));

    let mut team_weekly_total: u64 = 0;

    for (i, (score, m)) in scored.iter().enumerate() {
        let streak_display = if m.current_streak > 0 {
            format!("{}", m.current_streak)
        } else {
            "0".to_string()
        };

        let weekly_display = if m.weekly_total > 0 {
            format!("{}/{}", m.weekly_correct, m.weekly_total)
        } else {
            "-".to_string()
        };

        let verified = verify_chain(m);
        let name_display = if m.name.len() > 14 {
            format!("{}...", &m.name[..11])
        } else {
            m.name.clone()
        };
        let verified_flag = if !verified && m.total_quizzes > 0 {
            " [unverified]"
        } else {
            ""
        };

        println!(
            " {:<3} {:<14} {:>5.0}% {:>8} {:>11}{}",
            i + 1,
            name_display,
            score,
            streak_display,
            weekly_display,
            verified_flag,
        );

        team_weekly_total += m.weekly_total;
    }

    println!(" {}", "-".repeat(46));

    let team_avg: f64 = scored.iter().map(|(s, _)| s).sum::<f64>() / scored.len() as f64;
    println!(
        " Team average: {:.0}%  |  {} quizzes this week",
        team_avg, team_weekly_total
    );
    println!();

    Ok(())
}

fn team_reset(team_dir: &Path, project_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !team_dir.join("team.json").exists() {
        eprintln!("No team found.");
        std::process::exit(1);
    }

    let git_email = git_cmd(project_dir, &["config", "user.email"])?;
    let email_hash = short_hash(git_email.trim());
    let member_path = team_dir
        .join("members")
        .join(format!("{}.json", email_hash));

    if !member_path.exists() {
        eprintln!("You're not on this team. Run 'vibecheck team join' first.");
        std::process::exit(1);
    }

    let mut member: TeamMember = serde_json::from_str(&fs::read_to_string(&member_path)?)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    member.total_quizzes = 0;
    member.total_correct = 0;
    member.current_streak = 0;
    member.best_streak = 0;
    member.weekly_correct = 0;
    member.weekly_total = 0;
    member.week_start = now;
    member.chain_hash = String::new();
    member.chain_length = 0;

    fs::write(&member_path, serde_json::to_string_pretty(&member)?)?;

    println!("Stats reset for '{}'.", member.name);

    Ok(())
}

pub(crate) fn short_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = format!("{:x}", hasher.finalize());
    result[..8].to_string()
}

pub(crate) fn parse_flag_value(args: &[String], flag: &str) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == flag {
            return args.get(i + 1).cloned();
        }
    }
    None
}

pub(crate) fn get_team_context(project_dir: &Path) -> Option<(String, PathBuf)> {
    let team_dir = project_dir.join(".vibecheck-team");
    if !team_dir.join("team.json").exists() {
        return None;
    }

    let git_email = git_cmd(project_dir, &["config", "user.email"]).ok()?;
    let email_hash = short_hash(git_email.trim());
    let member_path = team_dir
        .join("members")
        .join(format!("{}.json", email_hash));

    if !member_path.exists() {
        return None;
    }

    Some((email_hash, member_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_hash_deterministic() {
        let h1 = short_hash("test@example.com");
        let h2 = short_hash("test@example.com");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 8);
    }

    #[test]
    fn short_hash_different_inputs() {
        let h1 = short_hash("alice@example.com");
        let h2 = short_hash("bob@example.com");
        assert_ne!(h1, h2);
    }

    #[test]
    fn parse_flag_value_found() {
        let args = vec![
            "init".to_string(),
            "--name".to_string(),
            "My Team".to_string(),
        ];
        assert_eq!(
            parse_flag_value(&args, "--name"),
            Some("My Team".to_string())
        );
    }

    #[test]
    fn parse_flag_value_not_found() {
        let args = vec!["init".to_string()];
        assert_eq!(parse_flag_value(&args, "--name"), None);
    }

    #[test]
    fn team_config_serialization() {
        let config = TeamConfig {
            name: "Test Team".to_string(),
            created_at: 12345,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: TeamConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test Team");
        assert_eq!(parsed.created_at, 12345);
    }

    #[test]
    fn team_member_serialization() {
        let member = TeamMember {
            name: "Alice".to_string(),
            email_hash: "a1b2c3d4".to_string(),
            total_quizzes: 10,
            total_correct: 8,
            current_streak: 3,
            best_streak: 5,
            weekly_correct: 2,
            weekly_total: 3,
            week_start: 100,
            last_quiz_at: 200,
            joined_at: 50,
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&member).unwrap();
        let parsed: TeamMember = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.total_quizzes, 10);
        assert_eq!(parsed.total_correct, 8);
        assert_eq!(parsed.current_streak, 3);
        assert_eq!(parsed.best_streak, 5);
        assert_eq!(parsed.weekly_correct, 2);
        assert_eq!(parsed.weekly_total, 3);
    }

    #[test]
    fn team_init_creates_structure() {
        let dir = std::env::temp_dir().join("vibecheck_team_init_test");
        let _ = fs::remove_dir_all(&dir);
        let team_dir = dir.join(".vibecheck-team");

        let result = team_init(
            &team_dir,
            &[
                "init".to_string(),
                "--name".to_string(),
                "Test Squad".to_string(),
            ],
        );
        assert!(result.is_ok());
        assert!(team_dir.join("team.json").exists());
        assert!(team_dir.join("members").is_dir());

        let config: TeamConfig =
            serde_json::from_str(&fs::read_to_string(team_dir.join("team.json")).unwrap()).unwrap();
        assert_eq!(config.name, "Test Squad");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn team_stats_empty_team() {
        let dir = std::env::temp_dir().join("vibecheck_team_stats_empty");
        let _ = fs::remove_dir_all(&dir);
        let team_dir = dir.join(".vibecheck-team");
        fs::create_dir_all(team_dir.join("members")).unwrap();

        let config = TeamConfig {
            name: "Empty Team".to_string(),
            created_at: 12345,
        };
        fs::write(
            team_dir.join("team.json"),
            serde_json::to_string(&config).unwrap(),
        )
        .unwrap();

        let result = team_stats(&team_dir);
        assert!(result.is_ok());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn team_stats_with_members() {
        let dir = std::env::temp_dir().join("vibecheck_team_stats_members");
        let _ = fs::remove_dir_all(&dir);
        let team_dir = dir.join(".vibecheck-team");
        fs::create_dir_all(team_dir.join("members")).unwrap();

        let config = TeamConfig {
            name: "Stat Team".to_string(),
            created_at: 12345,
        };
        fs::write(
            team_dir.join("team.json"),
            serde_json::to_string(&config).unwrap(),
        )
        .unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let member1 = TeamMember {
            name: "Alice".to_string(),
            email_hash: "aaaa1111".to_string(),
            total_quizzes: 10,
            total_correct: 8,
            current_streak: 3,
            best_streak: 5,
            weekly_correct: 2,
            weekly_total: 3,
            week_start: now,
            last_quiz_at: now,
            joined_at: now - 1000,
            ..Default::default()
        };
        let member2 = TeamMember {
            name: "Bob".to_string(),
            email_hash: "bbbb2222".to_string(),
            total_quizzes: 5,
            total_correct: 3,
            current_streak: 0,
            best_streak: 2,
            weekly_correct: 1,
            weekly_total: 2,
            week_start: now,
            last_quiz_at: now,
            joined_at: now - 500,
            ..Default::default()
        };

        fs::write(
            team_dir.join("members").join("aaaa1111.json"),
            serde_json::to_string_pretty(&member1).unwrap(),
        )
        .unwrap();
        fs::write(
            team_dir.join("members").join("bbbb2222.json"),
            serde_json::to_string_pretty(&member2).unwrap(),
        )
        .unwrap();

        let result = team_stats(&team_dir);
        assert!(result.is_ok());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn team_reset_clears_stats() {
        let dir = std::env::temp_dir().join("vibecheck_team_reset_test");
        let _ = fs::remove_dir_all(&dir);
        let team_dir = dir.join(".vibecheck-team");
        fs::create_dir_all(team_dir.join("members")).unwrap();

        let member = TeamMember {
            name: "Charlie".to_string(),
            email_hash: "cccc3333".to_string(),
            total_quizzes: 20,
            total_correct: 15,
            current_streak: 5,
            best_streak: 10,
            weekly_correct: 3,
            weekly_total: 4,
            week_start: 100,
            last_quiz_at: 200,
            joined_at: 50,
            ..Default::default()
        };
        let member_path = team_dir.join("members").join("cccc3333.json");
        fs::write(&member_path, serde_json::to_string_pretty(&member).unwrap()).unwrap();

        let mut loaded: TeamMember =
            serde_json::from_str(&fs::read_to_string(&member_path).unwrap()).unwrap();
        loaded.total_quizzes = 0;
        loaded.total_correct = 0;
        loaded.current_streak = 0;
        loaded.best_streak = 0;
        loaded.weekly_correct = 0;
        loaded.weekly_total = 0;
        fs::write(&member_path, serde_json::to_string_pretty(&loaded).unwrap()).unwrap();

        let reloaded: TeamMember =
            serde_json::from_str(&fs::read_to_string(&member_path).unwrap()).unwrap();
        assert_eq!(reloaded.total_quizzes, 0);
        assert_eq!(reloaded.total_correct, 0);
        assert_eq!(reloaded.current_streak, 0);
        assert_eq!(reloaded.best_streak, 0);
        assert_eq!(reloaded.name, "Charlie");

        let _ = fs::remove_dir_all(&dir);
    }
}
