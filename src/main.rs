use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct HookPayload {
    hook_event_name: Option<String>,
    stop_hook_active: Option<bool>,
    last_assistant_message: Option<String>,
    cwd: Option<String>,
    transcript_path: Option<String>,
}

#[derive(Deserialize)]
struct Config {
    enabled: Option<bool>,
    #[serde(rename = "minSecondsBetweenQuizzes")]
    min_seconds_between_quizzes: Option<u64>,
    #[serde(rename = "maxDiffChars")]
    max_diff_chars: Option<usize>,
    difficulty: Option<String>,
    #[serde(rename = "trackProgress")]
    track_progress: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: Some(true),
            min_seconds_between_quizzes: Some(900),
            max_diff_chars: Some(2000),
            difficulty: Some("normal".to_string()),
            track_progress: Some(false),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    last_quiz_at: u64,
    last_diff_hash: String,
    #[serde(default)]
    snoozed_until: u64,
    #[serde(default)]
    total_quizzes: u64,
    #[serde(default)]
    total_correct: u64,
    #[serde(default)]
    streak: u64,
}

#[derive(Serialize)]
struct BlockDecision {
    decision: String,
    reason: String,
}

struct QuizContext {
    raw_diff: String,
    diff: String,
    files: Vec<String>,
    commit_msg: String,
}

struct DiffSummary {
    lines_added: usize,
    lines_removed: usize,
    functions_added: Vec<String>,
    functions_removed: Vec<String>,
    has_error_handling_changes: bool,
    has_security_changes: bool,
    has_api_changes: bool,
}

// ---------------------------------------------------------------------------
// Entry point - CLI routing
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() <= 1 {
        // No args: Claude Code Stop hook mode (reads stdin)
        if run_hook().is_err() {
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
        "init" => init_git_hook(),
        "remove" => remove_git_hook(),
        "quiz" => run_quiz(&args[2..]),
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
    println!("  vibecheck init         Install git post-commit hook");
    println!("  vibecheck remove       Remove git post-commit hook");
    println!("  vibecheck --help       Show this help");
    println!("  vibecheck --version    Show version\n");
    println!("EXAMPLES:");
    println!("  vibecheck quiz | pbcopy     Copy quiz to clipboard");
    println!("  vibecheck quiz | llm        Pipe to any LLM CLI");
    println!("  vibecheck init              Auto-quiz after every commit\n");
    println!("WORKS WITH:");
    println!("  Claude Code, Cursor, Windsurf, OpenClaw, PicoClaw,");
    println!("  NanoClaw, Cline, Aider, or any AI tool that reads text.\n");
    println!("CONFIG: .claude/vibecheck.json or ~/.claude/vibecheck.json");
    println!("DOCS:   https://github.com/akshan-main/vibe-check");
}

fn print_version() {
    println!("vibecheck {}", env!("CARGO_PKG_VERSION"));
}

// ---------------------------------------------------------------------------
// Mode 1: Claude Code Stop hook (existing behavior)
// ---------------------------------------------------------------------------

fn run_hook() -> Result<(), Box<dyn std::error::Error>> {
    // Read stdin
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let payload: HookPayload = serde_json::from_str(&input).unwrap_or_default();

    // Guard: must be a Stop event
    if payload.hook_event_name.as_deref() != Some("Stop") {
        return Ok(());
    }

    // Guard: recursion prevention
    if payload.stop_hook_active.unwrap_or(false) {
        return Ok(());
    }

    // Guard: quiz already completed this turn
    if let Some(ref msg) = payload.last_assistant_message {
        if msg.contains("[vibecheck:done]") {
            return Ok(());
        }
    }

    // Resolve project directory
    let project_dir = resolve_project_dir(&payload)?;

    // Load config
    let config = load_config(&project_dir);
    if !config.enabled.unwrap_or(true) {
        return Ok(());
    }

    // Must be inside a git repo
    if !is_git_repo(&project_dir) {
        return Ok(());
    }

    // Must have uncommitted changes
    let status = git_cmd(&project_dir, &["status", "--porcelain"])?;
    let status = status.trim();
    if status.is_empty() {
        return Ok(());
    }

    // Throttle: check time since last quiz
    let state_dir = project_dir.join(".claude").join(".vibecheck");
    fs::create_dir_all(&state_dir)?;
    let state_path = state_dir.join("state.json");
    let state = load_state(&state_path);

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    // Check snooze
    if state.snoozed_until > 0 && now < state.snoozed_until {
        return Ok(());
    }

    let min_gap = config.min_seconds_between_quizzes.unwrap_or(900);
    if now.saturating_sub(state.last_quiz_at) < min_gap {
        return Ok(());
    }

    // Collect context using parallel git operations
    let max_chars = config.max_diff_chars.unwrap_or(2000);
    let ctx = collect_working_context(&project_dir, max_chars)?;

    if ctx.raw_diff.trim().is_empty() {
        return Ok(());
    }

    // Throttle: check if diff changed
    let diff_hash = hash_string(&ctx.raw_diff);
    if diff_hash == state.last_diff_hash {
        return Ok(());
    }

    let files_line = if ctx.files.is_empty() {
        "(unknown files)".to_string()
    } else {
        ctx.files.join(", ")
    };

    // Update state before outputting (so if Claude crashes, we don't re-quiz)
    let new_state = State {
        last_quiz_at: now,
        last_diff_hash: diff_hash,
        snoozed_until: 0,
        total_quizzes: state.total_quizzes,
        total_correct: state.total_correct,
        streak: state.streak,
    };
    save_state(&state_path, &new_state);

    // Extract user's original prompt from transcript
    let user_prompt = extract_user_prompt(payload.transcript_path.as_deref());

    let difficulty = config.difficulty.as_deref().unwrap_or("normal");
    let track_progress = config.track_progress.unwrap_or(false);

    // Build the instruction
    let state_path_str = state_path.to_string_lossy().to_string();
    let config_path = project_dir.join(".claude").join("vibecheck.json");
    let config_path_str = config_path.to_string_lossy().to_string();
    let reason = build_reason(
        &files_line,
        &ctx.diff,
        &state_path_str,
        &config_path_str,
        &user_prompt,
        difficulty,
        track_progress,
        &state,
    );

    // Output block decision
    let decision = BlockDecision {
        decision: "block".to_string(),
        reason,
    };
    println!("{}", serde_json::to_string(&decision)?);

    Ok(())
}

// ---------------------------------------------------------------------------
// Mode 2: Standalone quiz (works with any editor/AI tool)
// ---------------------------------------------------------------------------

fn run_quiz(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = find_git_root()?;
    let config = load_config(&project_dir);
    let max_chars = config.max_diff_chars.unwrap_or(2000);
    let use_commit = args.iter().any(|a| a == "--commit");

    let ctx = if use_commit {
        collect_commit_context(&project_dir, max_chars)?
    } else {
        collect_working_context(&project_dir, max_chars)?
    };

    if ctx.raw_diff.trim().is_empty() {
        eprintln!("no changes to quiz on");
        return Ok(());
    }

    let summary = analyze_diff(&ctx.raw_diff);
    output_quiz_context(&ctx, &summary, &config);

    Ok(())
}

// ---------------------------------------------------------------------------
// Mode 3: Git hook management
// ---------------------------------------------------------------------------

fn init_git_hook() -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = find_git_root()?;
    let hooks_dir = project_dir.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join("post-commit");

    let marker = "# vibecheck";
    let hook_cmd = "vibecheck quiz --commit 2>/dev/null || true";
    let block = format!("\n{}\n{}\n", marker, hook_cmd);

    if hook_path.exists() {
        let content = fs::read_to_string(&hook_path)?;
        if content.contains(marker) {
            println!("vibecheck hook already installed in .git/hooks/post-commit");
            return Ok(());
        }
        // Append to existing hook
        let mut f = fs::OpenOptions::new().append(true).open(&hook_path)?;
        use std::io::Write;
        write!(f, "{}", block)?;
    } else {
        fs::write(&hook_path, format!("#!/bin/sh\n{}", block))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
    }

    println!("Installed post-commit hook.");
    println!("VibeCheck will print quiz context after every commit.");
    println!("Works with any AI tool - pipe it, paste it, or let your tool read it.");
    println!("\nRemove with: vibecheck remove");

    Ok(())
}

fn remove_git_hook() -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = find_git_root()?;
    let hook_path = project_dir.join(".git").join("hooks").join("post-commit");

    if !hook_path.exists() {
        println!("no post-commit hook found");
        return Ok(());
    }

    let content = fs::read_to_string(&hook_path)?;
    let marker = "# vibecheck";

    if !content.contains(marker) {
        println!("vibecheck not found in post-commit hook");
        return Ok(());
    }

    // Remove the vibecheck block (marker line + command line)
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut skip_next = false;
    for line in &lines {
        if line.trim() == marker {
            skip_next = true;
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        new_lines.push(*line);
    }

    let new_content = new_lines.join("\n");

    // If only shebang left (or empty), remove the file entirely
    if new_content.trim().is_empty() || new_content.trim() == "#!/bin/sh" {
        fs::remove_file(&hook_path)?;
        println!("Removed post-commit hook.");
    } else {
        fs::write(&hook_path, new_content)?;
        println!("Removed vibecheck from post-commit hook.");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Parallel context collection (leverages Rust's zero-cost threads)
// ---------------------------------------------------------------------------

fn collect_working_context(
    dir: &Path,
    max_chars: usize,
) -> Result<QuizContext, Box<dyn std::error::Error>> {
    use std::thread;

    let d1 = dir.to_path_buf();
    let d2 = dir.to_path_buf();
    let d3 = dir.to_path_buf();
    let d4 = dir.to_path_buf();

    // Spawn 4 git commands concurrently on native OS threads
    let h1 = thread::spawn(move || {
        git_cmd_with_timeout(&d1, &["diff", "--unified=3"], 3).unwrap_or_default()
    });
    let h2 = thread::spawn(move || {
        git_cmd_with_timeout(&d2, &["diff", "--staged", "--unified=3"], 3).unwrap_or_default()
    });
    let h3 = thread::spawn(move || {
        git_cmd_with_timeout(&d3, &["diff", "--name-only"], 3).unwrap_or_default()
    });
    let h4 = thread::spawn(move || {
        git_cmd_with_timeout(&d4, &["diff", "--staged", "--name-only"], 3).unwrap_or_default()
    });

    let unstaged_diff = h1.join().map_err(|_| "thread panic")?;
    let staged_diff = h2.join().map_err(|_| "thread panic")?;
    let unstaged_files = h3.join().map_err(|_| "thread panic")?;
    let staged_files = h4.join().map_err(|_| "thread panic")?;

    let raw_diff = format!("{}{}", unstaged_diff, staged_diff);
    let files = dedup_file_list(&unstaged_files, &staged_files);

    Ok(QuizContext {
        diff: truncate_diff(&raw_diff, max_chars),
        raw_diff,
        files,
        commit_msg: String::new(),
    })
}

fn collect_commit_context(
    dir: &Path,
    max_chars: usize,
) -> Result<QuizContext, Box<dyn std::error::Error>> {
    use std::thread;

    let d1 = dir.to_path_buf();
    let d2 = dir.to_path_buf();
    let d3 = dir.to_path_buf();

    // 3 parallel git commands for committed changes
    let h1 = thread::spawn(move || {
        git_cmd_with_timeout(&d1, &["diff", "HEAD~1", "--unified=3"], 3).unwrap_or_default()
    });
    let h2 = thread::spawn(move || {
        git_cmd_with_timeout(&d2, &["diff", "HEAD~1", "--name-only"], 3).unwrap_or_default()
    });
    let h3 = thread::spawn(move || {
        git_cmd_with_timeout(&d3, &["log", "-1", "--pretty=%s"], 3).unwrap_or_default()
    });

    let raw_diff = h1.join().map_err(|_| "thread panic")?;
    let files_raw = h2.join().map_err(|_| "thread panic")?;
    let commit_msg = h3.join().map_err(|_| "thread panic")?;

    let files: Vec<String> = files_raw
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .take(10)
        .collect();

    Ok(QuizContext {
        diff: truncate_diff(&raw_diff, max_chars),
        raw_diff,
        files,
        commit_msg,
    })
}

// ---------------------------------------------------------------------------
// Diff analysis (Rust-native pattern detection on raw diff text)
// ---------------------------------------------------------------------------

fn analyze_diff(diff: &str) -> DiffSummary {
    let mut summary = DiffSummary {
        lines_added: 0,
        lines_removed: 0,
        functions_added: Vec::new(),
        functions_removed: Vec::new(),
        has_error_handling_changes: false,
        has_security_changes: false,
        has_api_changes: false,
    };

    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            summary.lines_added += 1;
            let content = &line[1..];

            if let Some(name) = extract_function_name(content) {
                summary.functions_added.push(name);
            }

            check_patterns(content, &mut summary);
        } else if line.starts_with('-') && !line.starts_with("---") {
            summary.lines_removed += 1;
            let content = &line[1..];

            if let Some(name) = extract_function_name(content) {
                summary.functions_removed.push(name);
            }

            check_patterns(content, &mut summary);
        }
    }

    summary
}

fn check_patterns(content: &str, summary: &mut DiffSummary) {
    let lower = content.to_lowercase();
    if lower.contains("error")
        || lower.contains("catch")
        || lower.contains("except")
        || lower.contains("panic")
        || lower.contains("unwrap")
    {
        summary.has_error_handling_changes = true;
    }
    if lower.contains("auth")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("csrf")
        || lower.contains("cors")
        || lower.contains("permission")
    {
        summary.has_security_changes = true;
    }
    if lower.contains("route")
        || lower.contains("endpoint")
        || lower.contains("/api")
        || lower.contains("handler")
        || lower.contains("middleware")
    {
        summary.has_api_changes = true;
    }
}

fn extract_function_name(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Language-agnostic function detection
    let patterns: &[(&str, &str)] = &[
        ("fn ", "("),       // Rust
        ("def ", "("),      // Python, Ruby
        ("function ", "("), // JavaScript
        ("func ", "("),     // Go
        ("sub ", "("),      // Perl
    ];

    for &(prefix, suffix) in patterns {
        if let Some(start) = trimmed.find(prefix) {
            let after = &trimmed[start + prefix.len()..];
            if let Some(end) = after.find(suffix) {
                let name = after[..end].trim();
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    return Some(name.to_string());
                }
            }
        }
    }

    // JS arrow functions: const name = (...) =>
    if trimmed.contains("=>") {
        if let Some(eq_pos) = trimmed.find('=') {
            let before = trimmed[..eq_pos].trim();
            // Extract last word before =
            if let Some(name) = before.split_whitespace().last() {
                if name.chars().all(|c| c.is_alphanumeric() || c == '_') && name.len() > 1 {
                    return Some(name.to_string());
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Quiz output (structured context for any AI tool)
// ---------------------------------------------------------------------------

fn output_quiz_context(ctx: &QuizContext, summary: &DiffSummary, config: &Config) {
    let difficulty = config.difficulty.as_deref().unwrap_or("normal");

    println!("# VibeCheck\n");

    if !ctx.commit_msg.is_empty() {
        println!("Commit: {}\n", ctx.commit_msg.trim());
    }

    if !ctx.files.is_empty() {
        println!("Changed files: {}\n", ctx.files.join(", "));
    }

    // Structured summary from diff analysis
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

    let difficulty_note = match difficulty {
        "beginner" => {
            "Ask about the most obvious, surface-level change the user would notice."
        }
        "advanced" => "Ask about edge cases, security implications, or subtle behavior changes.",
        _ => "Ask about the most important change and its real-world impact on users.",
    };

    println!("## Quiz Instructions\n");
    println!("Create ONE multiple-choice question (A/B/C/D) about this diff.");
    println!("{}", difficulty_note);
    println!("Test whether the developer knows what their PRODUCT does now,");
    println!("not how the code works. The question should be answerable by");
    println!("someone who understands the product but hasn't read the code.\n");

    println!("---\n");
    println!("Pipe this to your AI tool:");
    println!("  vibecheck quiz | pbcopy        # copy to clipboard");
    println!("  vibecheck quiz | llm           # pipe to LLM CLI");
    println!("  vibecheck quiz > .quiz.md      # save for your AI to read");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_git_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = env::current_dir()?;
    let output = git_cmd(&dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim()))
}

fn resolve_project_dir(payload: &HookPayload) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Priority: $CLAUDE_PROJECT_DIR > payload.cwd > current dir
    if let Ok(dir) = env::var("CLAUDE_PROJECT_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Ok(p);
        }
    }
    if let Some(ref cwd) = payload.cwd {
        let p = PathBuf::from(cwd);
        if p.is_dir() {
            return Ok(p);
        }
    }
    Ok(env::current_dir()?)
}

fn load_config(project_dir: &Path) -> Config {
    // Try project-level first, then global
    let project_config = project_dir.join(".claude").join("vibecheck.json");
    if let Some(cfg) = read_config_file(&project_config) {
        return cfg;
    }
    if let Ok(home) = env::var("HOME") {
        let global_config = PathBuf::from(home).join(".claude").join("vibecheck.json");
        if let Some(cfg) = read_config_file(&global_config) {
            return cfg;
        }
    }
    Config::default()
}

fn read_config_file(path: &Path) -> Option<Config> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn is_git_repo(dir: &Path) -> bool {
    git_cmd(dir, &["rev-parse", "--is-inside-work-tree"]).is_ok()
}

fn git_cmd(dir: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let child = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err("git command failed".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_cmd_with_timeout(dir: &Path, args: &[&str], timeout_secs: u64) -> Option<String> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    // Simple timeout: try wait, give up after duration
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    let mut out = String::new();
                    if let Some(mut stdout) = child.stdout.take() {
                        stdout.read_to_string(&mut out).ok();
                    }
                    return Some(out);
                }
                return None;
            }
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

fn dedup_file_list(unstaged: &str, staged: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();

    for line in unstaged.lines().chain(staged.lines()) {
        let f = line.trim();
        if !f.is_empty() && seen.insert(f.to_string()) {
            files.push(f.to_string());
            if files.len() >= 10 {
                break;
            }
        }
    }
    files
}

fn hash_string(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn truncate_diff(diff: &str, max_chars: usize) -> String {
    if diff.len() <= max_chars {
        diff.to_string()
    } else {
        let truncated: String = diff.chars().take(max_chars).collect();
        format!("{}\n\n[diff truncated]", truncated)
    }
}

fn extract_user_prompt(transcript_path: Option<&str>) -> String {
    let path = match transcript_path {
        Some(p) => p,
        None => return String::new(),
    };

    // Expand ~ to home directory
    let expanded = if path.starts_with("~/") {
        if let Ok(home) = env::var("HOME") {
            format!("{}{}", home, &path[1..])
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };

    let content = match fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // JSONL format: each line is a JSON object. Find the first user message.
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val.get("role").and_then(|r| r.as_str()) == Some("user") {
                // Extract text content
                if let Some(content) = val.get("content") {
                    if let Some(text) = content.as_str() {
                        let truncated: String = text.chars().take(500).collect();
                        return truncated;
                    }
                    // content might be an array of blocks
                    if let Some(arr) = content.as_array() {
                        for block in arr {
                            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                    let truncated: String = text.chars().take(500).collect();
                                    return truncated;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    String::new()
}

fn load_state(path: &Path) -> State {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(path: &Path, state: &State) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, json);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_reason(
    files_line: &str,
    diff_snippet: &str,
    state_path: &str,
    config_path: &str,
    user_prompt: &str,
    difficulty: &str,
    track_progress: bool,
    state: &State,
) -> String {
    let prompt_section = if user_prompt.is_empty() {
        String::new()
    } else {
        format!("\nUser's original prompt:\n{}\n", user_prompt)
    };

    let difficulty_instruction = match difficulty {
        "beginner" => "\nDIFFICULTY: BEGINNER - Ask about the most obvious, surface-level change. What did the feature add or change that a user would immediately notice? Keep the question simple and the wrong answers clearly wrong.\n",
        "advanced" => "\nDIFFICULTY: ADVANCED - Ask about edge cases, security implications, data flow, or subtle behavior changes that are easy to miss. The wrong answers should be plausible even to someone paying close attention.\n",
        _ => "",
    };

    let tracking_section = if track_progress {
        format!(
            "\nPROGRESS TRACKING: After the quiz is done (right before [vibecheck:done]), run this Bash command to update stats:\npython3 -c \"import json,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()) if p.exists() else {{}}; d['total_quizzes']=d.get('total_quizzes',0)+1; correct=sys.argv[2]=='1'; d['total_correct']=d.get('total_correct',0)+(1 if correct else 0); d['streak']=(d.get('streak',0)+1) if correct else 0; p.write_text(json.dumps(d,indent=2))\" '{}' {{CORRECT}}\nReplace {{CORRECT}} with 1 if the user answered correctly, 0 if wrong.\nThen show: \"Stats: {{total_correct}}/{{total_quizzes}} correct (streak: {{streak}})\"\nCurrent stats: {}/{} correct, streak: {}\n",
            state_path, state.total_correct, state.total_quizzes, state.streak
        )
    } else {
        String::new()
    };

    format!(
        r#"You just finished the main task. Now run a quick VibeCheck.

IMPORTANT RULES:
- Do NOT use Edit, Write, or any code-modifying tools on PROJECT files. This is learning-only.
- Quiz answers must NOT influence any further actions or decisions.
- Keep it under 10 seconds for the user.
- You MAY use Bash ONLY for the specific snooze/disable commands shown below.

STEP 1: Use AskUserQuestion to ask:
  question: "VibeCheck: quick 10-second comprehension check on what just changed?"
  header: "VibeCheck"
  options:
    - label: "Yes (10s)", description: "One quick question about your changes"
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
- If "Yes (10s)": continue to STEP 3.

STEP 3: Analyze the diff and create ONE multiple-choice question.
{difficulty_instruction}
IMPORTANT: Focus on the MOST IMPORTANT change, not the largest one. A 2-line behavior change can matter more than 50 lines of boilerplate. Read the entire diff, use your understanding of the code and the product, and pick the single change that has the most meaningful impact on what users experience. Ignore formatting, imports, renaming, and refactors that don't change behavior.

Classify what happened:
- Was a feature ADDED? (new capability that didn't exist before)
- Was a feature CHANGED? (existing behavior now works differently)
- Was something REMOVED? (capability or safeguard that's now gone)
- Was it a FIX? (broken thing that now works)

Then ask a question that tests whether the developer understands the REAL-WORLD IMPACT of this specific change on their product. Vibe coders build products - they need to understand what their product does, not how to read code.

QUESTION FORMULA - pick one:
  * WHAT CHANGED: "Before this change, [X happened]. What happens now instead?"
  * WHAT'S NEW: "A user tries [action] for the first time. What do they experience?"
  * WHAT'S GONE: "You removed [feature/check/step]. What can users do now that they couldn't before - or what protection is no longer there?"
  * SIDE EFFECTS: "This change also affects [related area]. What's different there now?"
  * EDGE CASE: "A user does [unusual but realistic action]. How does your app handle it after this change?"
  * LIMITS: "What's the maximum/minimum [value/count/size] this feature now supports? What happens at the boundary?"
  * DATA: "After this change, what new data is being stored/sent/exposed? Who can see it?"

NEVER ASK:
  * About code syntax, language features, or programming concepts
  * About which library or framework is used
  * Anything a developer would need to read code to answer - the question should be answerable by someone who understands the PRODUCT but not the code
  * Generic questions unrelated to this specific diff

WRONG ANSWERS: Each should be a plausible misunderstanding of what the product change does. Things a developer might assume if they didn't pay attention to what Claude actually built.

Format: exactly 4 options (labels "A", "B", "C", "D"), one correct. Ask via AskUserQuestion with header: "VibeCheck", multiSelect: false.

STEP 4: After the user answers:
1. Explain the correct answer in plain language - what the product does now and why
2. If wrong: explain what they misunderstood about the change and what their answer would have meant for users
3. PROMPTING TIP: Compare the user's original prompt (provided below) with what was actually built (the diff). If the prompt was vague and the implementation has gaps or surprises the user might not expect, suggest a more specific prompt that would have covered those gaps. If the prompt was already detailed and the implementation matches well, say so - "Your prompt covered this well." Don't fabricate issues.

Then end your message with: [vibecheck:done]
{tracking_section}
CHANGE CONTEXT:
Changed files: {files_line}
{prompt_section}
Diff:
{diff_snippet}"#,
        files_line = files_line,
        prompt_section = prompt_section,
        diff_snippet = diff_snippet,
        state_path = state_path,
        config_path = config_path,
        difficulty_instruction = difficulty_instruction,
        tracking_section = tracking_section
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;

    #[test]
    fn truncate_diff_short() {
        let diff = "hello world";
        assert_eq!(truncate_diff(diff, 100), "hello world");
    }

    #[test]
    fn truncate_diff_long() {
        let diff = "a".repeat(50);
        let result = truncate_diff(&diff, 10);
        assert!(result.starts_with("aaaaaaaaaa"));
        assert!(result.ends_with("[diff truncated]"));
    }

    #[test]
    fn truncate_diff_exact_boundary() {
        let diff = "a".repeat(10);
        assert_eq!(truncate_diff(&diff, 10), "a".repeat(10));
    }

    #[test]
    fn hash_string_deterministic() {
        let h1 = hash_string("test");
        let h2 = hash_string("test");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_string_different_inputs() {
        let h1 = hash_string("hello");
        let h2 = hash_string("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn load_state_missing_file() {
        let state = load_state(Path::new("/tmp/nonexistent_vibecheck_state.json"));
        assert_eq!(state.last_quiz_at, 0);
        assert_eq!(state.last_diff_hash, "");
        assert_eq!(state.snoozed_until, 0);
    }

    #[test]
    fn save_and_load_state() {
        let path = std::env::temp_dir().join("vibecheck_test_state.json");
        let state = State {
            last_quiz_at: 12345,
            last_diff_hash: "abc".to_string(),
            snoozed_until: 99999,
            total_quizzes: 10,
            total_correct: 7,
            streak: 3,
        };
        save_state(&path, &state);
        let loaded = load_state(&path);
        assert_eq!(loaded.last_quiz_at, 12345);
        assert_eq!(loaded.last_diff_hash, "abc");
        assert_eq!(loaded.snoozed_until, 99999);
        assert_eq!(loaded.total_quizzes, 10);
        assert_eq!(loaded.total_correct, 7);
        assert_eq!(loaded.streak, 3);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_config_missing_file() {
        let cfg = read_config_file(Path::new("/tmp/nonexistent_vibecheck_config.json"));
        assert!(cfg.is_none());
    }

    #[test]
    fn load_config_valid_file() {
        let path = std::env::temp_dir().join("vibecheck_test_config.json");
        fs::write(
            &path,
            r#"{"enabled": false, "minSecondsBetweenQuizzes": 60}"#,
        )
        .unwrap();
        let cfg = read_config_file(&path).unwrap();
        assert_eq!(cfg.enabled, Some(false));
        assert_eq!(cfg.min_seconds_between_quizzes, Some(60));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn extract_user_prompt_missing_file() {
        let result = extract_user_prompt(Some("/tmp/nonexistent_transcript.jsonl"));
        assert_eq!(result, "");
    }

    #[test]
    fn extract_user_prompt_none() {
        let result = extract_user_prompt(None);
        assert_eq!(result, "");
    }

    #[test]
    fn extract_user_prompt_from_jsonl() {
        let path = std::env::temp_dir().join("vibecheck_test_transcript.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"role":"user","content":"Add rate limiting"}}"#).unwrap();
        writeln!(
            f,
            r#"{{"role":"assistant","content":"Sure, I'll add rate limiting."}}"#
        )
        .unwrap();
        drop(f);

        let result = extract_user_prompt(Some(path.to_str().unwrap()));
        assert_eq!(result, "Add rate limiting");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn extract_user_prompt_content_array() {
        let path = std::env::temp_dir().join("vibecheck_test_transcript2.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"{{"role":"user","content":[{{"type":"text","text":"Build a login page"}}]}}"#
        )
        .unwrap();
        drop(f);

        let result = extract_user_prompt(Some(path.to_str().unwrap()));
        assert_eq!(result, "Build a login page");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn extract_user_prompt_truncates_long_input() {
        let path = std::env::temp_dir().join("vibecheck_test_transcript3.jsonl");
        let long_text = "x".repeat(1000);
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"role":"user","content":"{}"}}"#, long_text).unwrap();
        drop(f);

        let result = extract_user_prompt(Some(path.to_str().unwrap()));
        assert_eq!(result.len(), 500);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn build_reason_includes_prompt() {
        let state = State::default();
        let reason = build_reason(
            "file.rs",
            "diff content",
            "/tmp/state",
            "/tmp/config",
            "Add auth",
            "normal",
            false,
            &state,
        );
        assert!(reason.contains("Add auth"));
        assert!(reason.contains("User's original prompt"));
    }

    #[test]
    fn build_reason_no_prompt() {
        let state = State::default();
        let reason = build_reason(
            "file.rs",
            "diff content",
            "/tmp/state",
            "/tmp/config",
            "",
            "normal",
            false,
            &state,
        );
        assert!(!reason.contains("User's original prompt"));
    }

    #[test]
    fn build_reason_beginner_difficulty() {
        let state = State::default();
        let reason = build_reason(
            "file.rs", "diff", "/tmp/s", "/tmp/c", "", "beginner", false, &state,
        );
        assert!(reason.contains("BEGINNER"));
    }

    #[test]
    fn build_reason_advanced_difficulty() {
        let state = State::default();
        let reason = build_reason(
            "file.rs", "diff", "/tmp/s", "/tmp/c", "", "advanced", false, &state,
        );
        assert!(reason.contains("ADVANCED"));
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
            "file.rs", "diff", "/tmp/s", "/tmp/c", "", "normal", true, &state,
        );
        assert!(reason.contains("PROGRESS TRACKING"));
        assert!(reason.contains("3/5"));
    }

    #[test]
    fn default_config_values() {
        let cfg = Config::default();
        assert_eq!(cfg.enabled, Some(true));
        assert_eq!(cfg.min_seconds_between_quizzes, Some(900));
        assert_eq!(cfg.max_diff_chars, Some(2000));
        assert_eq!(cfg.difficulty.as_deref(), Some("normal"));
        assert_eq!(cfg.track_progress, Some(false));
    }

    #[test]
    fn payload_deserializes_with_transcript_path() {
        let json = r#"{"hook_event_name":"Stop","transcript_path":"/tmp/test.jsonl"}"#;
        let payload: HookPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.hook_event_name.as_deref(), Some("Stop"));
        assert_eq!(payload.transcript_path.as_deref(), Some("/tmp/test.jsonl"));
    }

    // --- New tests for standalone features ---

    #[test]
    fn extract_function_name_rust() {
        assert_eq!(
            extract_function_name("  fn process_payment(amount: f64) {"),
            Some("process_payment".to_string())
        );
    }

    #[test]
    fn extract_function_name_python() {
        assert_eq!(
            extract_function_name("  def handle_request(self, req):"),
            Some("handle_request".to_string())
        );
    }

    #[test]
    fn extract_function_name_javascript() {
        assert_eq!(
            extract_function_name("function validateInput(data) {"),
            Some("validateInput".to_string())
        );
    }

    #[test]
    fn extract_function_name_go() {
        assert_eq!(
            extract_function_name("func ServeHTTP(w http.ResponseWriter, r *http.Request) {"),
            Some("ServeHTTP".to_string())
        );
    }

    #[test]
    fn extract_function_name_arrow() {
        assert_eq!(
            extract_function_name("const fetchUsers = (page) => {"),
            Some("fetchUsers".to_string())
        );
    }

    #[test]
    fn extract_function_name_not_a_function() {
        assert_eq!(extract_function_name("  let x = 42;"), None);
    }

    #[test]
    fn analyze_diff_counts_lines() {
        let diff = "+added line 1\n+added line 2\n-removed line\n unchanged\n";
        let summary = analyze_diff(diff);
        assert_eq!(summary.lines_added, 2);
        assert_eq!(summary.lines_removed, 1);
    }

    #[test]
    fn analyze_diff_detects_functions() {
        let diff = "+fn new_feature(x: i32) {\n+  x + 1\n+}\n-fn old_feature() {\n";
        let summary = analyze_diff(diff);
        assert_eq!(summary.functions_added, vec!["new_feature"]);
        assert_eq!(summary.functions_removed, vec!["old_feature"]);
    }

    #[test]
    fn analyze_diff_detects_patterns() {
        let diff = "+  if auth_token.is_empty() {\n+    return Err(AuthError::Unauthorized);\n+  }\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_security_changes);
        assert!(summary.has_error_handling_changes);
    }

    #[test]
    fn analyze_diff_detects_api_changes() {
        let diff = "+router.get('/api/users', handler)\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_api_changes);
    }

    #[test]
    fn analyze_diff_ignores_diff_headers() {
        let diff = "+++ b/src/main.rs\n--- a/src/main.rs\n+real line\n-real removed\n";
        let summary = analyze_diff(diff);
        assert_eq!(summary.lines_added, 1);
        assert_eq!(summary.lines_removed, 1);
    }

    #[test]
    fn dedup_file_list_merges() {
        let unstaged = "src/a.rs\nsrc/b.rs\n";
        let staged = "src/b.rs\nsrc/c.rs\n";
        let files = dedup_file_list(unstaged, staged);
        assert_eq!(files, vec!["src/a.rs", "src/b.rs", "src/c.rs"]);
    }

    #[test]
    fn dedup_file_list_caps_at_10() {
        let many = (0..20).map(|i| format!("file{}.rs", i)).collect::<Vec<_>>();
        let input = many.join("\n");
        let files = dedup_file_list(&input, "");
        assert_eq!(files.len(), 10);
    }

    #[test]
    fn init_and_remove_git_hook() {
        let dir = std::env::temp_dir().join("vibecheck_hook_test");
        let git_dir = dir.join(".git").join("hooks");
        let _ = fs::create_dir_all(&git_dir);
        let hook_path = git_dir.join("post-commit");

        // Write a hook file as if init was called
        let marker = "# vibecheck";
        let content = format!("#!/bin/sh\n\n{}\nvibecheck quiz --commit 2>/dev/null || true\n", marker);
        fs::write(&hook_path, &content).unwrap();

        // Verify it exists
        let read = fs::read_to_string(&hook_path).unwrap();
        assert!(read.contains(marker));

        // Simulate remove
        let lines: Vec<&str> = read.lines().collect();
        let mut new_lines = Vec::new();
        let mut skip_next = false;
        for line in &lines {
            if line.trim() == marker {
                skip_next = true;
                continue;
            }
            if skip_next {
                skip_next = false;
                continue;
            }
            new_lines.push(*line);
        }
        let new_content = new_lines.join("\n");
        if new_content.trim().is_empty() || new_content.trim() == "#!/bin/sh" {
            fs::remove_file(&hook_path).unwrap();
        }

        assert!(!hook_path.exists());

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }
}
