use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RiskLevel {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
        }
    }
}

pub(crate) struct DiffSummary {
    pub(crate) lines_added: usize,
    pub(crate) lines_removed: usize,
    pub(crate) functions_added: Vec<String>,
    pub(crate) functions_removed: Vec<String>,
    pub(crate) has_error_handling_changes: bool,
    pub(crate) has_security_changes: bool,
    pub(crate) has_api_changes: bool,
    pub(crate) has_dependency_changes: bool,
    pub(crate) has_migration_changes: bool,
    pub(crate) has_infra_changes: bool,
    pub(crate) has_concurrency_changes: bool,
    pub(crate) has_parsing_changes: bool,
    pub(crate) has_cache_changes: bool,
    pub(crate) binary_files: Vec<String>,
}

pub(crate) fn analyze_diff(diff: &str) -> DiffSummary {
    let mut summary = DiffSummary {
        lines_added: 0,
        lines_removed: 0,
        functions_added: Vec::new(),
        functions_removed: Vec::new(),
        has_error_handling_changes: false,
        has_security_changes: false,
        has_api_changes: false,
        has_dependency_changes: false,
        has_migration_changes: false,
        has_infra_changes: false,
        has_concurrency_changes: false,
        has_parsing_changes: false,
        has_cache_changes: false,
        binary_files: Vec::new(),
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

    // Error handling - these are specific enough
    if lower.contains("error")
        || lower.contains("catch")
        || lower.contains("except")
        || lower.contains("panic")
        || lower.contains("unwrap")
    {
        summary.has_error_handling_changes = true;
    }

    // Security - broad "auth" is ok since false positives (like "author") are
    // less costly than missing real security changes (like "authMiddleware")
    if (lower.contains("auth") && !lower.contains("author"))
        || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("csrf")
        || lower.contains("cors")
        || lower.contains("permission")
        || lower.contains("access_token")
        || lower.contains("jwt")
        || lower.contains("payment")
        || lower.contains("billing")
        || lower.contains("checkout")
        || lower.contains("stripe")
        || lower.contains("invoice")
        || lower.contains("subscription")
    {
        summary.has_security_changes = true;
    }

    // API/routing - require more specific patterns
    if lower.contains("router.")
        || lower.contains("route(")
        || lower.contains("routes.")
        || lower.contains("endpoint")
        || lower.contains("\"/api")
        || lower.contains("'/api")
        || lower.contains("handler(")
        || lower.contains("_handler")
        || lower.contains("middleware")
    {
        summary.has_api_changes = true;
    }

    // Concurrency - mutex/spawn/lock are specific enough, async is too broad
    if lower.contains("mutex")
        || lower.contains("rwlock")
        || lower.contains("spawn(")
        || lower.contains("thread::")
        || lower.contains("threading")
        || lower.contains("lock()")
        || lower.contains("channel(")
        || lower.contains("semaphore")
        || lower.contains("tokio::")
        || lower.contains("async fn")
    {
        summary.has_concurrency_changes = true;
    }

    // Parsing/serialization - require more specific patterns
    if lower.contains("serialize")
        || lower.contains("deserialize")
        || lower.contains("from_str(")
        || lower.contains("from_json")
        || lower.contains("to_json")
        || lower.contains("protobuf")
        || lower.contains("marshal")
        || lower.contains("serde")
    {
        summary.has_parsing_changes = true;
    }

    // Cache - "invalidat" matches "validate", use "invalidate" instead
    if lower.contains("cache")
        || lower.contains("invalidate")
        || lower.contains("invalidation")
        || lower.contains(" ttl")
        || lower.contains("_ttl")
        || lower.contains("expire")
        || lower.contains("evict")
    {
        summary.has_cache_changes = true;
    }

    // Database migrations
    if lower.contains("migration")
        || lower.contains("alter table")
        || lower.contains("create table")
        || lower.contains("drop table")
        || lower.contains("add column")
    {
        summary.has_migration_changes = true;
    }
}

pub(crate) fn check_file_risks(files: &[String], summary: &mut DiffSummary) {
    for f in files {
        let lower = f.to_lowercase();

        // Dependency lockfiles
        if lower.ends_with("cargo.lock")
            || lower.ends_with("package-lock.json")
            || lower.ends_with("yarn.lock")
            || lower.ends_with("pnpm-lock.yaml")
            || lower.ends_with("poetry.lock")
            || lower.ends_with("go.sum")
            || lower.ends_with("gemfile.lock")
        {
            summary.has_dependency_changes = true;
        }

        // Migration directories
        if lower.contains("migration") || lower.contains("migrate") {
            summary.has_migration_changes = true;
        }

        // Infra / CI files
        if lower.contains(".github/")
            || lower.contains("dockerfile")
            || lower.contains("docker-compose")
            || lower.contains(".gitlab-ci")
            || lower.contains("jenkinsfile")
            || lower.contains("terraform")
            || lower.ends_with(".tf")
            || (lower.ends_with(".yml") && lower.contains("ci"))
            || (lower.ends_with(".yaml") && lower.contains("ci"))
        {
            summary.has_infra_changes = true;
        }

        if is_binary_file(f) {
            summary.binary_files.push(f.clone());
        }
    }
}

pub(crate) fn is_binary_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "svg"
            | "ico"
            | "webp"
            | "bmp"
            | "woff"
            | "woff2"
            | "ttf"
            | "eot"
            | "otf"
            | "mp3"
            | "mp4"
            | "wav"
            | "ogg"
            | "avi"
            | "mov"
            | "webm"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "7z"
            | "rar"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "a"
            | "lib"
            | "sqlite"
            | "db"
            | "sqlite3"
            | "pyc"
            | "class"
            | "o"
            | "obj"
    )
}

pub(crate) fn compute_risk_level(summary: &DiffSummary) -> RiskLevel {
    // High risk: auth, payments, DB migrations, public API, dependencies, infra
    if summary.has_security_changes
        || summary.has_migration_changes
        || summary.has_api_changes
        || summary.has_dependency_changes
        || summary.has_infra_changes
    {
        return RiskLevel::High;
    }

    // Medium risk: error handling, caching, concurrency, parsing
    if summary.has_error_handling_changes
        || summary.has_cache_changes
        || summary.has_concurrency_changes
        || summary.has_parsing_changes
    {
        return RiskLevel::Medium;
    }

    RiskLevel::Low
}

pub(crate) fn extract_function_name(line: &str) -> Option<String> {
    let trimmed = line.trim();

    let patterns: &[(&str, &str)] = &[
        ("fn ", "("),
        ("def ", "("),
        ("function ", "("),
        ("func ", "("),
        ("sub ", "("),
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
            if let Some(name) = before.split_whitespace().last() {
                if name.chars().all(|c| c.is_alphanumeric() || c == '_') && name.len() > 1 {
                    return Some(name.to_string());
                }
            }
        }
    }

    None
}

pub(crate) fn dedup_file_list(unstaged: &str, staged: &str) -> Vec<String> {
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

pub(crate) fn hash_string(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn truncate_diff(diff: &str, max_chars: usize) -> String {
    if diff.len() <= max_chars {
        diff.to_string()
    } else {
        let truncated: String = diff.chars().take(max_chars).collect();
        format!("{}\n\n[diff truncated]", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let diff =
            "+  if auth_token.is_empty() {\n+    return Err(AuthError::Unauthorized);\n+  }\n";
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

    // Golden tests for diff analysis

    #[test]
    fn analyze_diff_api_route_addition() {
        let diff = "+app.get('/api/v2/users', listUsers)\n+app.post('/api/v2/users', createUser)\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_api_changes);
        assert!(!summary.has_security_changes);
        assert_eq!(summary.lines_added, 2);
    }

    #[test]
    fn analyze_diff_auth_middleware_removal() {
        let diff = "-    if !verify_auth_token(req.headers()) {\n-        return Err(AuthError::Unauthorized);\n-    }\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_security_changes);
        assert!(summary.has_error_handling_changes);
        assert_eq!(summary.lines_removed, 3);
    }

    #[test]
    fn analyze_diff_error_handling_added() {
        let diff = "+    match result {\n+        Ok(val) => val,\n+        Err(e) => {\n+            eprintln!(\"error: {}\", e);\n+            return Err(e);\n+        }\n+    }\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_error_handling_changes);
        assert!(!summary.has_security_changes);
    }

    #[test]
    fn analyze_diff_password_field_exposed() {
        let diff = "+    struct UserResponse {\n+        name: String,\n+        password: String,\n+    }\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_security_changes);
    }

    #[test]
    fn analyze_diff_unwrap_added() {
        let diff = "+    let val = data.unwrap();\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_error_handling_changes);
    }

    #[test]
    fn analyze_diff_mixed_patterns() {
        let diff = "+fn handle_login(req: Request) -> Result<Token, AuthError> {\n+    let token = generate_auth_token(&req.password);\n+    if token.is_none() {\n+        return Err(AuthError::InvalidCredentials);\n+    }\n+    Ok(token.unwrap())\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_security_changes);
        assert!(summary.has_error_handling_changes);
        assert!(summary
            .functions_added
            .contains(&"handle_login".to_string()));
    }

    #[test]
    fn analyze_diff_cors_wildcard() {
        let diff = "+    app.use(cors({ origin: '*' }));\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_security_changes);
    }

    #[test]
    fn analyze_diff_middleware_addition() {
        let diff = "+app.use(rateLimitMiddleware)\n+app.use(authMiddleware)\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_api_changes);
        assert!(summary.has_security_changes);
    }

    #[test]
    fn analyze_diff_concurrency_patterns() {
        let diff = "+    let handle = thread::spawn(move || {\n+        let guard = mutex.lock().unwrap();\n+    });\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_concurrency_changes);
    }

    #[test]
    fn analyze_diff_parsing_patterns() {
        let diff = "+    let data: Config = serde_json::from_str(&input).deserialize();\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_parsing_changes);
    }

    #[test]
    fn analyze_diff_cache_patterns() {
        let diff = "+    cache.invalidate(&key);\n+    let ttl = Duration::from_secs(300);\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_cache_changes);
    }

    #[test]
    fn analyze_diff_migration_patterns() {
        let diff = "+ALTER TABLE users ADD COLUMN email VARCHAR(255);\n";
        let summary = analyze_diff(diff);
        assert!(summary.has_migration_changes);
    }

    #[test]
    fn check_file_risks_lockfiles() {
        let files = vec!["Cargo.lock".to_string(), "src/main.rs".to_string()];
        let mut summary = analyze_diff("");
        check_file_risks(&files, &mut summary);
        assert!(summary.has_dependency_changes);
    }

    #[test]
    fn check_file_risks_infra() {
        let files = vec![".github/workflows/ci.yml".to_string()];
        let mut summary = analyze_diff("");
        check_file_risks(&files, &mut summary);
        assert!(summary.has_infra_changes);
    }

    #[test]
    fn check_file_risks_migrations_dir() {
        let files = vec!["db/migrations/001_create_users.sql".to_string()];
        let mut summary = analyze_diff("");
        check_file_risks(&files, &mut summary);
        assert!(summary.has_migration_changes);
    }

    #[test]
    fn compute_risk_level_high_security() {
        let mut summary = analyze_diff("+    verify_auth_token(req)\n");
        check_file_risks(&[], &mut summary);
        assert_eq!(compute_risk_level(&summary), RiskLevel::High);
    }

    #[test]
    fn compute_risk_level_high_deps() {
        let mut summary = analyze_diff("");
        check_file_risks(&["package-lock.json".to_string()], &mut summary);
        assert_eq!(compute_risk_level(&summary), RiskLevel::High);
    }

    #[test]
    fn compute_risk_level_medium_concurrency() {
        let mut summary = analyze_diff("+    let guard = mutex.lock();\n");
        check_file_risks(&[], &mut summary);
        assert_eq!(compute_risk_level(&summary), RiskLevel::Medium);
    }

    #[test]
    fn compute_risk_level_low_plain() {
        let mut summary = analyze_diff("+    let x = 42;\n");
        check_file_risks(&[], &mut summary);
        assert_eq!(compute_risk_level(&summary), RiskLevel::Low);
    }

    #[test]
    fn is_binary_file_images() {
        assert!(is_binary_file("assets/logo.png"));
        assert!(is_binary_file("icon.svg"));
        assert!(is_binary_file("photo.jpg"));
        assert!(!is_binary_file("src/main.rs"));
        assert!(!is_binary_file("README.md"));
        assert!(!is_binary_file("styles.css"));
    }

    #[test]
    fn is_binary_file_archives_and_compiled() {
        assert!(is_binary_file("release.zip"));
        assert!(is_binary_file("data.sqlite"));
        assert!(is_binary_file("lib.dylib"));
        assert!(is_binary_file("Module.pyc"));
    }

    #[test]
    fn check_file_risks_detects_binary() {
        let files = vec!["src/main.rs".to_string(), "assets/logo.png".to_string()];
        let mut summary = analyze_diff("");
        check_file_risks(&files, &mut summary);
        assert_eq!(summary.binary_files, vec!["assets/logo.png"]);
    }
}
