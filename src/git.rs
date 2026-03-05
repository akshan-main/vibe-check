use serde::Deserialize;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Deserialize, Default)]
pub(crate) struct HookPayload {
    pub(crate) hook_event_name: Option<String>,
    pub(crate) stop_hook_active: Option<bool>,
    pub(crate) last_assistant_message: Option<String>,
    pub(crate) cwd: Option<String>,
    #[allow(dead_code)]
    pub(crate) transcript_path: Option<String>,
}

pub(crate) fn find_git_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = env::current_dir()?;
    let output = git_cmd(&dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim()))
}

pub(crate) fn resolve_project_dir(
    payload: &HookPayload,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
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

pub(crate) fn is_git_repo(dir: &Path) -> bool {
    git_cmd(dir, &["rev-parse", "--is-inside-work-tree"]).is_ok()
}

pub(crate) fn git_cmd(dir: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
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

pub(crate) fn git_cmd_with_timeout(
    dir: &Path,
    args: &[&str],
    timeout_secs: u64,
) -> Result<String, String> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn git: {}", e))?;

    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        if let Some(mut s) = stdout {
            let _ = s.read_to_string(&mut out);
        }
        out
    });

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().unwrap_or_default();
                if status.success() {
                    return Ok(out);
                }
                return Err(format!(
                    "git {} exited with {}",
                    args.first().unwrap_or(&""),
                    status
                ));
            }
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    let _ = reader.join();
                    return Err(format!(
                        "git {} timed out after {}s",
                        args.first().unwrap_or(&""),
                        timeout_secs
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("error waiting on git: {}", e)),
        }
    }
}

pub(crate) fn git_diff_between(
    dir: &Path,
    base: &str,
    head: &str,
) -> Result<(String, Vec<String>), Box<dyn std::error::Error>> {
    let range = format!("{}..{}", base, head);
    let d1 = dir.to_path_buf();
    let d2 = dir.to_path_buf();
    let range1 = range.clone();
    let range2 = range;

    let h1 = std::thread::spawn(move || {
        git_cmd_with_timeout(&d1, &["diff", &range1, "--unified=3"], 10)
    });
    let h2 = std::thread::spawn(move || {
        git_cmd_with_timeout(&d2, &["diff", &range2, "--name-only"], 10)
    });

    let diff_text = h1.join().map_err(|_| "thread panic")?;
    let files_raw = h2.join().map_err(|_| "thread panic")?;

    // If diff fails (e.g. invalid refs), return empty rather than error
    let diff = diff_text.unwrap_or_default();
    let files: Vec<String> = files_raw
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .take(20)
        .collect();

    Ok((diff, files))
}

pub(crate) fn init_git_hook() -> Result<(), Box<dyn std::error::Error>> {
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

pub(crate) fn remove_git_hook() -> Result<(), Box<dyn std::error::Error>> {
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

    if new_content.trim().is_empty() || new_content.trim() == "#!/bin/sh" {
        fs::remove_file(&hook_path)?;
        println!("Removed post-commit hook.");
    } else {
        fs::write(&hook_path, new_content)?;
        println!("Removed vibecheck from post-commit hook.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_deserializes() {
        let json = r#"{"hook_event_name":"Stop","stop_hook_active":false}"#;
        let payload: HookPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.hook_event_name.as_deref(), Some("Stop"));
        assert_eq!(payload.stop_hook_active, Some(false));
    }

    #[test]
    fn init_and_remove_git_hook() {
        let dir = std::env::temp_dir().join("vibecheck_hook_test");
        let git_dir = dir.join(".git").join("hooks");
        let _ = fs::create_dir_all(&git_dir);
        let hook_path = git_dir.join("post-commit");

        let marker = "# vibecheck";
        let content = format!(
            "#!/bin/sh\n\n{}\nvibecheck quiz --commit 2>/dev/null || true\n",
            marker
        );
        fs::write(&hook_path, &content).unwrap();

        let read = fs::read_to_string(&hook_path).unwrap();
        assert!(read.contains(marker));

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
        let _ = fs::remove_dir_all(&dir);
    }
}
