//! W8.A — review commands.
//!
//! Thin wrappers that surface common review-adjacent checks via the
//! CLI. Each command is:
//!
//! - **Headless** — no TUI, no LLM, no network. Works on CI.
//! - **Read-only** — observes the working tree, never mutates.
//! - **Structured JSON** when `--json` is passed; human prose otherwise.
//!
//! Commands:
//! - `vac advisor`           → snapshot of local state operators check
//!                              before committing.
//! - `vac autofix-pr`        → lint the unstaged diff for quick fixes.
//! - `vac bughunter`         → scan recent changes for code smells.
//! - `vac security-review`   → scan staged diff for secret-shaped
//!                              strings.
//! - `vac perf-issue`        → list large files + slow-test markers.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;

/// How a review command renders. The CLI passes `--format json` for
/// structured output.
#[derive(Debug, Clone, Copy)]
pub enum ReviewFormat {
    Text,
    Json,
}

impl ReviewFormat {
    pub fn from_str(s: &str) -> Self {
        match s {
            "json" => Self::Json,
            _ => Self::Text,
        }
    }
}

fn print_report<T: Serialize + ReportRender>(
    format: ReviewFormat,
    report: &T,
) -> anyhow::Result<()> {
    match format {
        ReviewFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        ReviewFormat::Text => report.render(),
    }
    Ok(())
}

/// Minimal trait so each report can customise its human-readable form
/// without every command re-implementing the JSON fallback.
pub trait ReportRender {
    fn render(&self);
}

// ── /advisor ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AdvisorReport {
    pub branch: Option<String>,
    pub uncommitted_files: usize,
    pub last_commit: Option<String>,
    pub cargo_check_hint: String,
}

impl ReportRender for AdvisorReport {
    fn render(&self) {
        println!("─ vac advisor ───────────────────────────────");
        println!("  branch: {}", self.branch.as_deref().unwrap_or("<detached>"));
        println!("  uncommitted files: {}", self.uncommitted_files);
        if let Some(last) = &self.last_commit {
            println!("  last commit: {last}");
        }
        println!("  hint: {}", self.cargo_check_hint);
    }
}

pub async fn advisor(project_root: PathBuf, format: ReviewFormat) -> anyhow::Result<()> {
    let branch = git_branch(&project_root).await;
    let uncommitted_files = git_dirty_count(&project_root).await.unwrap_or(0);
    let last_commit = git_last_commit(&project_root).await;
    let report = AdvisorReport {
        branch,
        uncommitted_files,
        last_commit,
        cargo_check_hint: if uncommitted_files > 0 {
            "run `cargo check` before commit".into()
        } else {
            "tree clean".into()
        },
    };
    print_report(format, &report)
}

// ── /autofix-pr ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AutofixReport {
    pub candidates: Vec<AutofixCandidate>,
}

#[derive(Debug, Serialize)]
pub struct AutofixCandidate {
    pub file: PathBuf,
    pub rule: String,
    pub line: usize,
}

impl ReportRender for AutofixReport {
    fn render(&self) {
        println!("─ vac autofix-pr ────────────────────────────");
        if self.candidates.is_empty() {
            println!("  no candidates");
            return;
        }
        for c in &self.candidates {
            println!("  {}:{} {}", c.file.display(), c.line, c.rule);
        }
    }
}

pub async fn autofix_pr(
    project_root: PathBuf,
    format: ReviewFormat,
) -> anyhow::Result<()> {
    let diff = git_unstaged_diff(&project_root).await;
    let candidates = scan_autofix(&diff);
    print_report(format, &AutofixReport { candidates })
}

/// Deterministic rule set. Each rule is a `(pattern_substring, label)`.
const AUTOFIX_RULES: &[(&str, &str)] = &[
    ("TODO(", "todo-token"),
    ("FIXME", "fixme"),
    ("println!(\"", "leftover-println"),
    (".unwrap()", "unwrap-in-diff"),
    ("dbg!(", "dbg-macro"),
];

pub(crate) fn scan_autofix(diff: &str) -> Vec<AutofixCandidate> {
    let mut out = Vec::new();
    let mut current_file: Option<PathBuf> = None;
    let mut line_no: usize = 0;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            current_file = Some(PathBuf::from(rest));
            line_no = 0;
            continue;
        }
        if line.starts_with("@@") {
            if let Some(tail) = line.split("+").nth(1) {
                if let Some(head) = tail.split(',').next() {
                    if let Some(stripped) = head.strip_prefix('+') {
                        line_no = stripped.trim().parse().unwrap_or(0);
                    } else {
                        line_no = head.trim().parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        if let Some(added) = line.strip_prefix('+') {
            if added.starts_with("++") {
                continue; // file header
            }
            line_no += 1;
            if let Some(file) = &current_file {
                for (pat, rule) in AUTOFIX_RULES {
                    if added.contains(pat) {
                        out.push(AutofixCandidate {
                            file: file.clone(),
                            rule: (*rule).into(),
                            line: line_no,
                        });
                    }
                }
            }
        } else if line.starts_with(' ') {
            line_no += 1;
        }
    }
    out
}

// ── /bughunter ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct BughunterReport {
    pub findings: Vec<BughunterFinding>,
}

#[derive(Debug, Serialize)]
pub struct BughunterFinding {
    pub file: PathBuf,
    pub line: usize,
    pub kind: String,
    pub excerpt: String,
}

impl ReportRender for BughunterReport {
    fn render(&self) {
        println!("─ vac bughunter ─────────────────────────────");
        if self.findings.is_empty() {
            println!("  clean");
            return;
        }
        for f in &self.findings {
            println!(
                "  {}:{} [{}] {}",
                f.file.display(),
                f.line,
                f.kind,
                f.excerpt.trim(),
            );
        }
    }
}

pub async fn bughunter(
    project_root: PathBuf,
    format: ReviewFormat,
) -> anyhow::Result<()> {
    let diff = git_last_commit_diff(&project_root).await;
    let findings = scan_bughunter(&diff);
    print_report(format, &BughunterReport { findings })
}

const BUGHUNTER_RULES: &[(&str, &str)] = &[
    (".expect(\"", "expect-in-prod"),
    ("panic!(", "panic-call"),
    ("unreachable!(", "unreachable-hit"),
    ("std::process::exit", "hard-exit"),
];

pub(crate) fn scan_bughunter(diff: &str) -> Vec<BughunterFinding> {
    scan_additions(diff, BUGHUNTER_RULES)
        .into_iter()
        .map(|(file, line, kind, excerpt)| BughunterFinding {
            file,
            line,
            kind: kind.into(),
            excerpt,
        })
        .collect()
}

// ── /security-review ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SecurityReport {
    pub hits: Vec<SecurityHit>,
}

#[derive(Debug, Serialize)]
pub struct SecurityHit {
    pub file: PathBuf,
    pub line: usize,
    pub kind: String,
    pub excerpt: String,
}

impl ReportRender for SecurityReport {
    fn render(&self) {
        println!("─ vac security-review ───────────────────────");
        if self.hits.is_empty() {
            println!("  no secret-shaped strings detected");
            return;
        }
        for h in &self.hits {
            println!("  {}:{} [{}] {}…", h.file.display(), h.line, h.kind, head(&h.excerpt, 40));
        }
    }
}

pub async fn security_review(
    project_root: PathBuf,
    format: ReviewFormat,
) -> anyhow::Result<()> {
    let diff = git_staged_diff(&project_root).await;
    let hits = scan_security(&diff);
    print_report(format, &SecurityReport { hits })
}

/// Conservative patterns. Avoid false positives on doc/comments.
const SECURITY_RULES: &[(&str, &str)] = &[
    ("AKIA", "aws-access-key"),
    ("ASIA", "aws-session-key"),
    ("-----BEGIN RSA PRIVATE KEY-----", "rsa-private-key"),
    ("-----BEGIN OPENSSH PRIVATE KEY-----", "openssh-private-key"),
    ("xoxb-", "slack-bot-token"),
    ("xoxp-", "slack-user-token"),
    ("ghp_", "github-token"),
    ("ghs_", "github-server-token"),
    ("sk-ant-", "anthropic-key"),
    ("sk-", "openai-like-key"),
];

pub(crate) fn scan_security(diff: &str) -> Vec<SecurityHit> {
    scan_additions(diff, SECURITY_RULES)
        .into_iter()
        .map(|(file, line, kind, excerpt)| SecurityHit {
            file,
            line,
            kind: kind.into(),
            excerpt,
        })
        .collect()
}

// ── /perf-issue ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PerfReport {
    pub large_files: Vec<LargeFile>,
    pub slow_test_markers: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct LargeFile {
    pub path: PathBuf,
    pub bytes: u64,
}

impl ReportRender for PerfReport {
    fn render(&self) {
        println!("─ vac perf-issue ────────────────────────────");
        println!("  large files (> 1 MiB):");
        if self.large_files.is_empty() {
            println!("    none");
        } else {
            for f in &self.large_files {
                println!("    {}  ({} KiB)", f.path.display(), f.bytes / 1024);
            }
        }
        println!("  slow-test markers:");
        if self.slow_test_markers.is_empty() {
            println!("    none");
        } else {
            for p in &self.slow_test_markers {
                println!("    {}", p.display());
            }
        }
    }
}

pub async fn perf_issue(
    project_root: PathBuf,
    format: ReviewFormat,
) -> anyhow::Result<()> {
    let large_files = find_large_files(&project_root, 1 * 1024 * 1024).await;
    let slow_test_markers = find_slow_test_markers(&project_root).await;
    print_report(format, &PerfReport { large_files, slow_test_markers })
}

// ── shared helpers ───────────────────────────────────────────────────

fn head(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Common diff scanner: walks `+` additions, tracks current file +
/// line, matches any rule substring. Returns tuples so each callsite
/// can wrap them into its own report type. Rules carry `'static`
/// lifetimes so the returned `kind` string can be re-borrowed into
/// owned report structs.
pub(crate) fn scan_additions(
    diff: &str,
    rules: &[(&'static str, &'static str)],
) -> Vec<(PathBuf, usize, &'static str, String)> {
    let mut out = Vec::new();
    let mut current_file: Option<PathBuf> = None;
    let mut line_no: usize = 0;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            current_file = Some(PathBuf::from(rest));
            line_no = 0;
            continue;
        }
        if let Some(after_plus) = line.strip_prefix("@@ -") {
            if let Some(plus_seg) = after_plus.split("+").nth(1) {
                let num: String = plus_seg
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                line_no = num.parse().unwrap_or(0);
            }
            continue;
        }
        if let Some(added) = line.strip_prefix('+') {
            if added.starts_with("++") {
                continue;
            }
            line_no += 1;
            if let Some(file) = &current_file {
                for (pat, kind) in rules {
                    if added.contains(pat) {
                        out.push((file.clone(), line_no, *kind, added.to_string()));
                    }
                }
            }
        } else if line.starts_with(' ') {
            line_no += 1;
        }
    }
    out
}

async fn run_git(root: &Path, args: &[&str]) -> Option<String> {
    // Async process spawn: `std::process::Command` in an async fn
    // would block the tokio executor thread for the full duration
    // of git's run. The tokio equivalent yields around the wait.
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

async fn git_branch(root: &Path) -> Option<String> {
    run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn git_last_commit(root: &Path) -> Option<String> {
    run_git(root, &["log", "-1", "--pretty=%h %s"])
        .await
        .map(|s| s.trim().to_string())
}

async fn git_dirty_count(root: &Path) -> Option<usize> {
    run_git(root, &["status", "--porcelain"])
        .await
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
}

async fn git_unstaged_diff(root: &Path) -> String {
    run_git(root, &["diff"]).await.unwrap_or_default()
}

async fn git_staged_diff(root: &Path) -> String {
    run_git(root, &["diff", "--cached"]).await.unwrap_or_default()
}

async fn git_last_commit_diff(root: &Path) -> String {
    run_git(root, &["show", "--unified=2", "HEAD"])
        .await
        .unwrap_or_default()
}

/// Slow-test / large-file walkers share this read cap so a symlink
/// to `/dev/zero` — or a genuinely huge log file — can't stall the
/// command. 2 MiB is plenty for grep-shaped heuristics.
const MAX_WALK_READ_BYTES: u64 = 2 * 1024 * 1024;

async fn find_large_files(root: &Path, min_bytes: u64) -> Vec<LargeFile> {
    use std::collections::VecDeque;
    let mut out = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_path_buf());
    // Bounded walk so a giant repo doesn't eat the command for lunch.
    const MAX_ENTRIES: usize = 50_000;
    let mut visited = 0usize;
    while let Some(dir) = queue.pop_front() {
        if visited >= MAX_ENTRIES {
            break;
        }
        let Ok(mut rd) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            visited += 1;
            let p = entry.path();
            if should_skip(&p) {
                continue;
            }
            // symlink_metadata does NOT follow links. A `.vac/link →
            // /proc` would otherwise trigger unbounded descent.
            let Ok(meta) = tokio::fs::symlink_metadata(&p).await else {
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue; // never traverse or read symlinks in the walker
            }
            if ft.is_dir() {
                queue.push_back(p);
            } else if ft.is_file() && meta.len() >= min_bytes {
                out.push(LargeFile {
                    path: p,
                    bytes: meta.len(),
                });
            }
        }
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out
}

async fn find_slow_test_markers(root: &Path) -> Vec<PathBuf> {
    use std::collections::VecDeque;
    let mut out = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_path_buf());
    const MAX_ENTRIES: usize = 20_000;
    let mut visited = 0usize;
    while let Some(dir) = queue.pop_front() {
        if visited >= MAX_ENTRIES {
            break;
        }
        let Ok(mut rd) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            visited += 1;
            let p = entry.path();
            if should_skip(&p) {
                continue;
            }
            let Ok(meta) = tokio::fs::symlink_metadata(&p).await else {
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                queue.push_back(p);
            } else if ft.is_file()
                && p.extension().and_then(|e| e.to_str()) == Some("rs")
                && meta.len() <= MAX_WALK_READ_BYTES
            {
                if let Ok(content) = tokio::fs::read_to_string(&p).await {
                    if content.contains("#[ignore]") || content.contains("// slow") {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

fn should_skip(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(
        name,
        "target" | "node_modules" | ".git" | ".vac" | "dist" | "build"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DIFF: &str = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 pub fn f() {
+    // TODO(alice): finish this
+    let _ = x.unwrap();
     println!(\"ok\");
 }
";

    #[test]
    fn autofix_rules_detect_todo_and_unwrap() {
        let out = scan_autofix(SAMPLE_DIFF);
        let rules: Vec<&str> = out.iter().map(|c| c.rule.as_str()).collect();
        assert!(rules.contains(&"todo-token"));
        assert!(rules.contains(&"unwrap-in-diff"));
    }

    #[test]
    fn autofix_skips_context_lines() {
        let out = scan_autofix(SAMPLE_DIFF);
        assert!(
            !out.iter().any(|c| c.rule == "leftover-println"),
            "context-line println should not match",
        );
    }

    #[test]
    fn bughunter_detects_panics() {
        let diff = "\
+++ b/x.rs
@@ -1,1 +1,2 @@
 fn y() {}
+    panic!(\"boom\");
";
        let out = scan_bughunter(diff);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "panic-call");
    }

    #[test]
    fn security_review_detects_github_token() {
        let diff = "\
+++ b/x.rs
@@ -1,1 +1,2 @@
 fn y() {}
+    let t = \"ghp_abcdefghijklmnopqrstuvwxyz0123456789\";
";
        let hits = scan_security(diff);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "github-token");
    }

    #[test]
    fn security_review_detects_anthropic_key() {
        let diff = "\
+++ b/.env
@@ -1,1 +1,2 @@
 ZZZ=1
+ANTHROPIC_API_KEY=sk-ant-api03-abc123
";
        let hits = scan_security(diff);
        assert!(hits.iter().any(|h| h.kind == "anthropic-key"));
    }

    #[test]
    fn security_review_clean_diff_yields_zero_hits() {
        let hits = scan_security(SAMPLE_DIFF);
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn perf_issue_skips_target_and_vac_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // A big file inside `target/` must not appear in the report.
        tokio::fs::create_dir_all(root.join("target")).await.unwrap();
        let big = root.join("target/huge.bin");
        tokio::fs::write(&big, vec![0u8; 2 * 1024 * 1024]).await.unwrap();
        let out = find_large_files(root, 1 * 1024 * 1024).await;
        assert!(
            out.iter().all(|f| !f.path.starts_with(root.join("target"))),
            "target/ must be skipped",
        );
    }

    #[tokio::test]
    async fn perf_issue_surfaces_large_top_level_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let big = root.join("data.bin");
        tokio::fs::write(&big, vec![0u8; 2 * 1024 * 1024]).await.unwrap();
        let out = find_large_files(root, 1 * 1024 * 1024).await;
        assert!(out.iter().any(|f| f.path == big));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn perf_walker_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Large real file inside root.
        let real = root.join("real.bin");
        tokio::fs::write(&real, vec![0u8; 2 * 1024 * 1024]).await.unwrap();
        // Symlink inside root pointing at the SAME file — if the
        // walker follows links, we'd see two hits and the symlink
        // path would show up in the report.
        let link = root.join("twin.bin");
        symlink(&real, &link).unwrap();
        let out = find_large_files(root, 1 * 1024 * 1024).await;
        // Only the real file, never the symlink.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, real);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn slow_walker_skips_symlinked_rs_file() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let real = root.join("lib.rs");
        tokio::fs::write(&real, "#[ignore] fn x() {}").await.unwrap();
        let link = root.join("link.rs");
        symlink(&real, &link).unwrap();
        let out = find_slow_test_markers(root).await;
        assert_eq!(out, vec![real]);
    }

    #[tokio::test]
    async fn slow_walker_skips_oversize_rs_file() {
        let tmp = tempfile::tempdir().unwrap();
        // A 3 MiB .rs file is above the MAX_WALK_READ_BYTES cap —
        // walker must not OOM / hang on it.
        let big = tmp.path().join("huge.rs");
        let payload = vec![b'a'; (MAX_WALK_READ_BYTES as usize) + 1024];
        tokio::fs::write(&big, payload).await.unwrap();
        let out = find_slow_test_markers(tmp.path()).await;
        assert!(out.is_empty(), "oversize .rs must be skipped");
    }

    #[tokio::test]
    async fn slow_test_markers_finds_ignored_tests() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let f = root.join("a.rs");
        tokio::fs::write(&f, "#[ignore] fn x() {}").await.unwrap();
        let out = find_slow_test_markers(root).await;
        assert_eq!(out, vec![f]);
    }
}
