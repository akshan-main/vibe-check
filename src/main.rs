use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Deserialize, Default)]
struct HookPayload {
    hook_event_name: Option<String>,
    stop_hook_active: Option<bool>,
    last_assistant_message: Option<String>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct Config {
    enabled: Option<bool>,
    #[serde(rename = "minSecondsBetweenQuizzes")]
    min_seconds_between_quizzes: Option<u64>,
    #[serde(rename = "maxDiffChars")]
    max_diff_chars: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: Some(true),
            min_seconds_between_quizzes: Some(900),
            max_diff_chars: Some(2000),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    last_quiz_at: u64,
    last_diff_hash: String,
    #[serde(default)]
    snoozed_until: u64,
}

#[derive(Serialize)]
struct BlockDecision {
    decision: String,
    reason: String,
}

fn main() {
    if let Err(_) = run() {
        // Any error → exit 0 (allow stop, never crash)
        std::process::exit(0);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
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

    // Get diff content — skip if empty (e.g. only untracked files, no modifications)
    let diff_all = get_full_diff(&project_dir);
    if diff_all.trim().is_empty() {
        return Ok(());
    }

    // Throttle: check if diff changed
    let diff_hash = hash_string(&diff_all);
    if diff_hash == state.last_diff_hash {
        return Ok(());
    }

    // Collect context for the quiz
    let file_list = get_changed_files(&project_dir);
    let max_chars = config.max_diff_chars.unwrap_or(2000);
    let truncated_diff = truncate_diff(&diff_all, max_chars);
    let files_line = if file_list.is_empty() {
        "(unknown files)".to_string()
    } else {
        file_list.join(", ")
    };

    // Update state before outputting (so if Claude crashes, we don't re-quiz)
    let new_state = State {
        last_quiz_at: now,
        last_diff_hash: diff_hash,
        snoozed_until: 0,
    };
    save_state(&state_path, &new_state);

    // Build the instruction
    let state_path_str = state_path.to_string_lossy().to_string();
    let config_path = project_dir.join(".claude").join("vibecheck.json");
    let config_path_str = config_path.to_string_lossy().to_string();
    let reason = build_reason(&files_line, &truncated_diff, &state_path_str, &config_path_str);

    // Output block decision
    let decision = BlockDecision {
        decision: "block".to_string(),
        reason,
    };
    println!("{}", serde_json::to_string(&decision)?);

    Ok(())
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

fn get_full_diff(dir: &Path) -> String {
    let unstaged = git_cmd_with_timeout(dir, &["diff", "--unified=3"], 3).unwrap_or_default();
    let staged = git_cmd_with_timeout(dir, &["diff", "--staged", "--unified=3"], 3).unwrap_or_default();
    format!("{}{}", unstaged, staged)
}

fn get_changed_files(dir: &Path) -> Vec<String> {
    let unstaged = git_cmd_with_timeout(dir, &["diff", "--name-only"], 3).unwrap_or_default();
    let staged = git_cmd_with_timeout(dir, &["diff", "--staged", "--name-only"], 3).unwrap_or_default();

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

fn build_reason(files_line: &str, diff_snippet: &str, state_path: &str, config_path: &str) -> String {
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
  python3 -c "import json,time,pathlib; p=pathlib.Path('{state_path}'); d=json.loads(p.read_text()) if p.exists() else {{}}; d['snoozed_until']=int(time.time())+1800; p.write_text(json.dumps(d,indent=2))"
- If "Disable": Run this Bash command to persist the disable, then say "VibeCheck disabled. Re-enable by setting enabled:true in .claude/vibecheck.json" and end with [vibecheck:done]:
  python3 -c "import json,pathlib; p=pathlib.Path('{config_path}'); d=json.loads(p.read_text()) if p.exists() else {{}}; d['enabled']=False; p.write_text(json.dumps(d,indent=2))"
- If "Yes (10s)": continue to STEP 3.

STEP 3: Create ONE multiple-choice question based on the diff below.

QUESTION RULES (critical — this determines if the tool is useful or annoying):
- Ask about BEHAVIOR and CONSEQUENCES, never syntax or trivia
- The user should walk away understanding what their code change DOES in the real world
- Frame questions from the perspective of a user/system interacting with the changed code

GOOD question patterns (use these):
  * "After this change, what happens when [specific user action or edge case]?"
  * "What problem does this change fix, and what was happening before?"
  * "If [realistic scenario], what would this code do differently now?"
  * "What could break if [this related component/input] behaves unexpectedly?"
  * "A user reports [symptom]. Based on this change, what's the most likely cause?"

BAD question patterns (never use these):
  * "What does [language keyword/syntax] mean?" — this is a textbook, not a quiz
  * "What is the return type of [function]?" — irrelevant to understanding
  * "Which design pattern is used here?" — academic, not practical
  * "What library is being imported?" — trivially visible in the diff

WRONG ANSWERS must be plausible. Each wrong option should be something a developer who DIDN'T read the diff carefully might believe. Never use obviously absurd options.

Have exactly 4 options (use labels "A", "B", "C", "D"). One clearly correct.

Ask it using AskUserQuestion with header: "VibeCheck" and multiSelect: false.

STEP 4: After the user answers, respond with:
1. The correct answer
2. A clear explanation of WHY — connect it to the actual code change (reference specific lines/functions from the diff)
3. If they got it wrong: why their choice was wrong and what part of the diff contradicts it
4. One practical takeaway: a rule of thumb they can apply when reviewing similar code in the future

Then end your message with: [vibecheck:done]

CHANGE CONTEXT:
Changed files: {files_line}

Diff:
{diff_snippet}"#,
        files_line = files_line,
        diff_snippet = diff_snippet,
        state_path = state_path,
        config_path = config_path
    )
}
