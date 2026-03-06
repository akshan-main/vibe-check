mod config;
mod diff;
mod git;
mod hook;
mod quiz;
mod stats;
mod team;

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() <= 1 {
        if hook::run_hook().is_err() {
            std::process::exit(0);
        }
        return;
    }

    let result = match args[1].as_str() {
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        "--version" | "-V" | "version" => {
            print_version();
            Ok(())
        }
        "init" => git::init_git_hook(),
        "remove" => git::remove_git_hook(),
        "quiz" => run_quiz(&args[2..]),
        "explain" => run_explain(&args[2..]),
        "ci" => run_ci(&args[2..]),
        "team" => team::run_team(&args[2..]),
        "mode" => run_mode(&args[2..]),
        "stats" => run_stats(),
        "record" => run_record(&args[2..]),
        "doctor" => run_doctor(),
        other => {
            eprintln!("unknown command: {}", other);
            eprintln!("run 'vibecheck --help' for usage");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn print_help() {
    println!("vibecheck {}", env!("CARGO_PKG_VERSION"));
    println!("Reality checks for vibe coders. One question per diff.\n");
    println!("USAGE:");
    println!("  vibecheck              Claude Code Stop hook (reads stdin)");
    println!("  vibecheck quiz         Quiz from uncommitted changes");
    println!("  vibecheck quiz --commit  Quiz from latest commit");
    println!("  vibecheck explain      Explain changes without a quiz");
    println!("  vibecheck mode         Show current mode");
    println!("  vibecheck mode <name>  Set mode (vibe_coder, developer, hardcore, learning)");
    println!("  vibecheck ci             CI mode: quiz for PR comments");
    println!("  vibecheck ci --base <ref> --head <ref>");
    println!("  vibecheck stats        Show your quiz stats and weak areas");
    println!("  vibecheck record --correct          Record a correct answer");
    println!("  vibecheck record --wrong            Record a wrong answer");
    println!("  vibecheck record --correct --category security   Record with category");
    println!("  vibecheck init         Install git post-commit hook");
    println!("  vibecheck remove       Remove git post-commit hook");
    println!("  vibecheck doctor       Diagnose your VibeCheck setup");
    println!("  vibecheck team init    Start a team leaderboard for this project");
    println!("  vibecheck team join    Register yourself on the team");
    println!("  vibecheck team         Show the team leaderboard");
    println!("  vibecheck team reset   Reset your own stats");
    println!("  vibecheck --help       Show this help");
    println!("  vibecheck --version    Show version\n");
    println!("EXAMPLES:");
    println!("  vibecheck quiz | pbcopy     Copy quiz to clipboard");
    println!("  vibecheck quiz | llm        Pipe to any LLM CLI");
    println!("  vibecheck explain | llm     Get a plain-language explanation");
    println!("  vibecheck ci | gh pr comment --body-file -");
    println!("  vibecheck init              Auto-quiz after every commit");
    println!("  vibecheck team init         Start tracking team stats\n");
    println!("WORKS WITH:");
    println!("  Claude Code, Cursor, Windsurf, OpenClaw, PicoClaw,");
    println!("  NanoClaw, Cline, Aider, or any AI tool that reads text.\n");
    println!("CONFIG: .claude/vibecheck.json or ~/.claude/vibecheck.json");
    println!("DOCS:   https://github.com/akshan-main/vibe-check");
}

fn print_version() {
    println!("vibecheck {}", env!("CARGO_PKG_VERSION"));
}

fn run_quiz(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = git::find_git_root()?;
    let cfg = config::load_config(&project_dir);
    let max_chars = cfg.max_diff_chars.unwrap_or(2000);
    let use_commit = args.iter().any(|a| a == "--commit");

    let ctx = if use_commit {
        quiz::collect_commit_context(&project_dir, max_chars)?
    } else {
        quiz::collect_working_context(&project_dir, max_chars)?
    };

    if ctx.raw_diff.trim().is_empty() {
        eprintln!("no changes to quiz on");
        return Ok(());
    }

    let state_dir = project_dir.join(".claude").join(".vibecheck");
    let state = config::load_state(&state_dir.join("state.json"));

    let mut summary = diff::analyze_diff(&ctx.raw_diff);
    diff::check_file_risks(&ctx.files, &mut summary);
    let risk = diff::compute_risk_level(&summary);
    quiz::output_quiz_context(&ctx, &summary, &cfg, risk, &state);

    Ok(())
}

fn run_ci(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = git::find_git_root()?;
    let cfg = config::load_config(&project_dir);
    let max_chars = cfg.max_diff_chars.unwrap_or(4000);

    let base = team::parse_flag_value(args, "--base")
        .or_else(|| env::var("GITHUB_BASE_REF").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "HEAD~1".to_string());

    let head = team::parse_flag_value(args, "--head")
        .or_else(|| env::var("GITHUB_HEAD_REF").ok().filter(|s| !s.is_empty()))
        .or_else(|| env::var("GITHUB_SHA").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "HEAD".to_string());

    let (raw_diff, files) = match git::git_diff_between(&project_dir, &base, &head) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    if raw_diff.trim().is_empty() && files.is_empty() {
        eprintln!("no changes between {} and {}", base, head);
        return Ok(());
    }

    let non_diff_files = quiz::detect_non_diff_files(&files, &raw_diff);
    let truncated_diff = diff::truncate_diff(&raw_diff, max_chars);

    let ctx = quiz::QuizContext {
        diff: truncated_diff,
        raw_diff,
        files,
        commit_msg: String::new(),
        non_diff_files,
    };

    let mut summary = diff::analyze_diff(&ctx.raw_diff);
    diff::check_file_risks(&ctx.files, &mut summary);
    let risk = diff::compute_risk_level(&summary);

    quiz::output_ci_context(&ctx, &summary, &cfg, risk, &base, &head);

    Ok(())
}

fn run_explain(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = git::find_git_root()?;
    let cfg = config::load_config(&project_dir);
    let max_chars = cfg.max_diff_chars.unwrap_or(2000);
    let use_commit = args.iter().any(|a| a == "--commit");

    let ctx = if use_commit {
        quiz::collect_commit_context(&project_dir, max_chars)?
    } else {
        quiz::collect_working_context(&project_dir, max_chars)?
    };

    if ctx.raw_diff.trim().is_empty() && ctx.non_diff_files.is_empty() {
        eprintln!("no changes to explain");
        return Ok(());
    }

    let mut summary = diff::analyze_diff(&ctx.raw_diff);
    diff::check_file_risks(&ctx.files, &mut summary);
    let risk = diff::compute_risk_level(&summary);
    quiz::output_explain_context(&ctx, &summary, &cfg, risk);

    Ok(())
}

fn run_stats() -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = git::find_git_root()
        .unwrap_or_else(|_| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let state_dir = project_dir.join(".claude").join(".vibecheck");
    let state = config::load_state(&state_dir.join("state.json"));

    println!("vibecheck stats\n");
    println!(
        "Overall: {}/{} correct ({:.0}%)",
        state.total_correct,
        state.total_quizzes,
        if state.total_quizzes > 0 {
            state.total_correct as f64 / state.total_quizzes as f64 * 100.0
        } else {
            0.0
        }
    );
    println!("Streak: {}\n", state.streak);

    let categories_path = state_dir.join("categories.json");
    if !categories_path.exists() {
        println!("No category data yet.");
        println!("Enable trackProgress in .claude/vibecheck.json to start tracking.");
        return Ok(());
    }

    let content = fs::read_to_string(&categories_path)?;
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content)?;

    let mut entries: Vec<(String, u64, u64)> = map
        .iter()
        .filter_map(|(cat, val)| {
            let arr = val.as_array()?;
            let total = arr.first()?.as_u64()?;
            let correct = arr.get(1)?.as_u64()?;
            if total > 0 {
                Some((cat.clone(), total, correct))
            } else {
                None
            }
        })
        .collect();

    if entries.is_empty() {
        println!("No category data yet.");
        return Ok(());
    }

    // Sort worst-first
    entries.sort_by(|a, b| {
        let acc_a = a.2 as f64 / a.1 as f64;
        let acc_b = b.2 as f64 / b.1 as f64;
        acc_a
            .partial_cmp(&acc_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!("By category:");
    println!("  {:<20} {:>6}  {:>8}", "Category", "Score", "Quizzes");
    println!("  {}", "-".repeat(38));

    for (cat, total, correct) in &entries {
        let pct = *correct as f64 / *total as f64 * 100.0;
        let flag = if pct < 60.0 && *total >= 3 {
            " <-- weak"
        } else {
            ""
        };
        println!(
            "  {:<20} {:>5.0}%  {:>3}/{:<3}{}",
            cat, pct, correct, total, flag
        );
    }
    println!();

    Ok(())
}

fn run_record(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let correct = if args.iter().any(|a| a == "--correct") {
        true
    } else if args.iter().any(|a| a == "--wrong") {
        false
    } else {
        eprintln!("usage: vibecheck record --correct [--category <name>]");
        eprintln!("       vibecheck record --wrong [--category <name>]");
        std::process::exit(1);
    };

    let category = team::parse_flag_value(args, "--category");
    let project_dir = git::find_git_root()?;

    match stats::record_answer(&project_dir, correct, category.as_deref()) {
        Ok(result) => {
            let cat_display = result
                .category
                .as_deref()
                .map(|c| format!(" ({})", c))
                .unwrap_or_default();
            println!(
                "Recorded: {}{} | {}/{} overall, streak: {}",
                if correct { "correct" } else { "wrong" },
                cat_display,
                result.total_correct,
                result.total_quizzes,
                result.streak,
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn run_mode(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = git::find_git_root()?;
    let cfg = config::load_config(&project_dir);
    let current = cfg.mode.unwrap_or_default();

    if args.is_empty() {
        println!("Current mode: {}\n", current);
        println!("Available modes:");
        println!("  vibe_coder  Flow-first, light questions (default)");
        println!("  developer   Objective, verification-driven, risk-scaled");
        println!("  hardcore    Maximum rigor, always hardest questions");
        println!("  learning    Adaptive difficulty based on your accuracy\n");
        println!("Set with: vibecheck mode <name>");
        return Ok(());
    }

    let input = args[0].to_lowercase();
    let new_mode = match config::Mode::from_str_strict(&input) {
        Some(m) => m,
        None => {
            eprintln!(
                "unknown mode: '{}'\nvalid modes: {}",
                args[0],
                config::Mode::all_names().join(", ")
            );
            std::process::exit(1);
        }
    };
    config::set_mode(&project_dir, &new_mode)?;
    println!("Mode set to: {}", new_mode);
    Ok(())
}

fn run_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("vibecheck doctor\n");

    let project_dir = git::find_git_root()
        .unwrap_or_else(|_| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let project_config = project_dir.join(".claude").join("vibecheck.json");
    let home_config = env::var("HOME")
        .map(|h| PathBuf::from(h).join(".claude").join("vibecheck.json"))
        .ok();

    if project_config.exists() {
        println!("Config: {} (found)", project_config.display());
    } else if let Some(ref hc) = home_config {
        if hc.exists() {
            println!("Config: {} (found, global)", hc.display());
        } else {
            println!(
                "Config: {} (not found, using defaults)",
                project_config.display()
            );
        }
    } else {
        println!(
            "Config: {} (not found, using defaults)",
            project_config.display()
        );
    }

    let is_repo = git::is_git_repo(&project_dir);
    println!("Git repo: {}", if is_repo { "yes" } else { "no" });

    let team_dir = project_dir.join(".vibecheck-team");
    let team_active = team_dir.join("team.json").exists();
    println!(
        "Team mode: {}",
        if team_active {
            "active"
        } else {
            "not configured"
        }
    );

    let hook_path = project_dir.join(".git").join("hooks").join("post-commit");
    let hook_installed = hook_path.exists()
        && fs::read_to_string(&hook_path)
            .map(|c| c.contains("vibecheck"))
            .unwrap_or(false);
    println!(
        "Post-commit hook: {}",
        if hook_installed {
            "installed"
        } else {
            "not installed"
        }
    );

    let cfg = config::load_config(&project_dir);
    println!("\nConfig values:");
    println!("  enabled: {}", cfg.enabled.unwrap_or(true));
    println!("  mode: {}", cfg.mode.clone().unwrap_or_default());
    println!(
        "  minSecondsBetweenQuizzes: {}",
        cfg.min_seconds_between_quizzes.unwrap_or(0)
    );
    println!("  maxDiffChars: {}", cfg.max_diff_chars.unwrap_or(2000));
    println!(
        "  difficulty: {}",
        cfg.difficulty
            .as_deref()
            .unwrap_or("auto (based on mode + risk)")
    );
    println!("  trackProgress: {}", cfg.track_progress.unwrap_or(false));
    println!(
        "  hookAction: {}",
        cfg.hook_action.as_deref().unwrap_or("quiz")
    );

    Ok(())
}
