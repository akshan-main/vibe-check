use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    #[default]
    VibeCoder,
    Developer,
    Hardcore,
    Learning,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::VibeCoder => write!(f, "vibe_coder"),
            Mode::Developer => write!(f, "developer"),
            Mode::Hardcore => write!(f, "hardcore"),
            Mode::Learning => write!(f, "learning"),
        }
    }
}

impl Mode {
    #[cfg(test)]
    pub(crate) fn from_str_lossy(s: &str) -> Self {
        match s {
            "vibe_coder" => Mode::VibeCoder,
            "developer" => Mode::Developer,
            "hardcore" => Mode::Hardcore,
            "learning" => Mode::Learning,
            _ => Mode::VibeCoder,
        }
    }

    pub(crate) fn from_str_strict(s: &str) -> Option<Self> {
        match s {
            "vibe_coder" => Some(Mode::VibeCoder),
            "developer" => Some(Mode::Developer),
            "hardcore" => Some(Mode::Hardcore),
            "learning" => Some(Mode::Learning),
            _ => None,
        }
    }

    pub(crate) fn all_names() -> &'static [&'static str] {
        &["vibe_coder", "developer", "hardcore", "learning"]
    }
}

#[derive(Deserialize)]
pub(crate) struct Config {
    pub(crate) enabled: Option<bool>,
    #[serde(rename = "minSecondsBetweenQuizzes")]
    pub(crate) min_seconds_between_quizzes: Option<u64>,
    #[serde(rename = "maxDiffChars")]
    pub(crate) max_diff_chars: Option<usize>,
    pub(crate) difficulty: Option<String>,
    #[serde(rename = "trackProgress")]
    pub(crate) track_progress: Option<bool>,
    #[serde(default)]
    pub(crate) mode: Option<Mode>,
    #[serde(rename = "hookAction")]
    pub(crate) hook_action: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: Some(true),
            min_seconds_between_quizzes: Some(900),
            max_diff_chars: Some(2000),
            difficulty: None,
            track_progress: Some(false),
            mode: Some(Mode::VibeCoder),
            hook_action: None,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct State {
    pub(crate) last_quiz_at: u64,
    pub(crate) last_diff_hash: String,
    #[serde(default)]
    pub(crate) snoozed_until: u64,
    #[serde(default)]
    pub(crate) total_quizzes: u64,
    #[serde(default)]
    pub(crate) total_correct: u64,
    #[serde(default)]
    pub(crate) streak: u64,
}

pub(crate) fn load_config(project_dir: &Path) -> Config {
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

pub(crate) fn read_config_file(path: &Path) -> Option<Config> {
    let content = fs::read_to_string(path).ok()?;

    // Try strict parse first
    if let Ok(cfg) = serde_json::from_str::<Config>(&content) {
        return Some(cfg);
    }

    // Tolerant parse: read as generic JSON, extract valid fields
    let map: serde_json::Map<String, serde_json::Value> = match serde_json::from_str(&content) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "warning: could not parse {}: {}. Using defaults.",
                path.display(),
                e
            );
            return None;
        }
    };

    eprintln!(
        "warning: {} has invalid fields, using valid ones with defaults for the rest.",
        path.display()
    );

    let mut cfg = Config::default();
    if let Some(v) = map.get("enabled").and_then(|v| v.as_bool()) {
        cfg.enabled = Some(v);
    }
    if let Some(v) = map.get("minSecondsBetweenQuizzes").and_then(|v| v.as_u64()) {
        cfg.min_seconds_between_quizzes = Some(v);
    }
    if let Some(v) = map.get("maxDiffChars").and_then(|v| v.as_u64()) {
        cfg.max_diff_chars = Some(v as usize);
    }
    if let Some(v) = map.get("difficulty").and_then(|v| v.as_str()) {
        cfg.difficulty = Some(v.to_string());
    }
    if let Some(v) = map.get("trackProgress").and_then(|v| v.as_bool()) {
        cfg.track_progress = Some(v);
    }
    if let Some(v) = map.get("mode").and_then(|v| v.as_str()) {
        if let Some(m) = Mode::from_str_strict(v) {
            cfg.mode = Some(m);
        } else {
            eprintln!("warning: unknown mode '{}', using default.", v);
        }
    }
    if let Some(v) = map.get("hookAction").and_then(|v| v.as_str()) {
        cfg.hook_action = Some(v.to_string());
    }
    Some(cfg)
}

pub(crate) fn load_state(path: &Path) -> State {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_state(path: &Path, state: &State) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, json);
    }
}

pub(crate) fn set_mode(project_dir: &Path, mode: &Mode) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = project_dir.join(".claude").join("vibecheck.json");
    let mut map: serde_json::Map<String, serde_json::Value> = if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };
    map.insert("mode".to_string(), serde_json::to_value(mode)?);
    fs::create_dir_all(config_path.parent().unwrap())?;
    fs::write(&config_path, serde_json::to_string_pretty(&map)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(cfg.mode, None); // backward compat: missing field = None
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn default_config_values() {
        let cfg = Config::default();
        assert_eq!(cfg.enabled, Some(true));
        assert_eq!(cfg.min_seconds_between_quizzes, Some(900));
        assert_eq!(cfg.max_diff_chars, Some(2000));
        assert_eq!(cfg.difficulty, None);
        assert_eq!(cfg.track_progress, Some(false));
        assert_eq!(cfg.mode, Some(Mode::VibeCoder));
    }

    #[test]
    fn mode_from_str_lossy() {
        assert_eq!(Mode::from_str_lossy("developer"), Mode::Developer);
        assert_eq!(Mode::from_str_lossy("hardcore"), Mode::Hardcore);
        assert_eq!(Mode::from_str_lossy("garbage"), Mode::VibeCoder);
    }

    #[test]
    fn mode_display() {
        assert_eq!(format!("{}", Mode::VibeCoder), "vibe_coder");
        assert_eq!(format!("{}", Mode::Hardcore), "hardcore");
    }

    #[test]
    fn set_mode_preserves_other_fields() {
        let dir = std::env::temp_dir().join("vibecheck_set_mode_test");
        let _ = fs::remove_dir_all(&dir);
        let config_dir = dir.join(".claude");
        fs::create_dir_all(&config_dir).unwrap();

        // Write config with existing fields
        fs::write(
            config_dir.join("vibecheck.json"),
            r#"{"enabled": false, "difficulty": "advanced"}"#,
        )
        .unwrap();

        set_mode(&dir, &Mode::Hardcore).unwrap();

        let content = fs::read_to_string(config_dir.join("vibecheck.json")).unwrap();
        let map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&content).unwrap();
        assert_eq!(map["enabled"], false);
        assert_eq!(map["difficulty"], "advanced");
        assert_eq!(map["mode"], "hardcore");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_with_mode_field() {
        let path = std::env::temp_dir().join("vibecheck_mode_config.json");
        fs::write(&path, r#"{"mode": "developer"}"#).unwrap();
        let cfg = read_config_file(&path).unwrap();
        assert_eq!(cfg.mode, Some(Mode::Developer));
        let _ = fs::remove_file(&path);
    }
}
