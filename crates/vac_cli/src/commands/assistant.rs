//! P1 — proactive assistant.
//!
//! Scans the signal pipeline for distress patterns (build failures,
//! test regressions) and enqueues a `Suggested` job for each match.
//! The actual rewind-store read is gated behind the `signal-rewind`
//! feature; the pattern-detection logic sits in a pure helper so
//! default-feature builds can still test the decision table.

use std::path::PathBuf;

/// Detector tag — which kind of signal stream produced the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKind {
    BuildFailure,
    TestRegression,
}

impl PatternKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::BuildFailure => "build-failure",
            Self::TestRegression => "test-regression",
        }
    }
}

/// Stream-name classifier: decides which detector runs on a given
/// signal stream. Returns `None` for unrecognised streams.
pub fn classify_stream(stream: &str) -> Option<PatternKind> {
    // Accept both rewind-store syntax (`build:cargo`, `test:x`) and
    // filesystem log stems (`build-cargo.log` → `build-cargo`) so the
    // no-feature fallback works out of the box.
    if stream.starts_with("build:") || stream.starts_with("build-") {
        Some(PatternKind::BuildFailure)
    } else if stream.starts_with("test:") || stream.starts_with("test-") {
        Some(PatternKind::TestRegression)
    } else {
        None
    }
}

/// Pure pattern detector. Returns a task description when the lines
/// contain the detector's signature marker; `None` otherwise. The
/// signature matches are deliberately narrow (substring) so noisy
/// logs don't fire the detector; a false-positive would push a
/// bogus suggestion to the operator.
pub fn detect_pattern(
    kind: PatternKind,
    stream: &str,
    lines: &[&str],
) -> Option<String> {
    match kind {
        PatternKind::BuildFailure => {
            // rustc emits `error[E0308]` etc; cargo relays the line.
            if lines.iter().any(|l| l.contains("error[E")) {
                return Some(format!("Fix build failure in {stream}"));
            }
            None
        }
        PatternKind::TestRegression => {
            // cargo-nextest + libtest both print `FAILED` on failing
            // tests. Checking for the literal avoids flaky single-
            // character matches.
            if lines.iter().any(|l| l.contains("FAILED")) {
                return Some(format!("Fix test regression in {stream}"));
            }
            None
        }
    }
}

/// Filesystem fallback: scan `<root>/.vac/signal-logs/*.log` where each
/// file's stem is treated as a stream name (e.g. `build-cargo.log` →
/// stream `build-cargo`). Returns `(stream, recent_lines)` tuples with
/// up to `max_lines` tail lines. Available without the `signal-rewind`
/// feature so the primary assistant sweep still works on plain-text
/// build/test output.
pub fn scan_log_dir(dir: &std::path::Path, max_lines: usize) -> std::io::Result<Vec<(String, Vec<String>)>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let lines: Vec<String> = text.lines().rev().take(max_lines).map(|s| s.to_string()).collect();
        out.push((stem.to_string(), lines.into_iter().rev().collect()));
    }
    Ok(out)
}

pub async fn execute(project_root: PathBuf, session_id: Option<String>) -> anyhow::Result<()> {
    use vac_runtime::{Job, JobKind, JobStatus, TaskQueue};
    let queue = TaskQueue::with_storage(project_root.join(".vac/queue.json"));
    let mut hits = 0usize;
    let session = session_id.unwrap_or_else(|| "default".to_string());

    // Build list of (stream, lines) from whichever source is available.
    // Prefer rewind DB when the feature is enabled; always also scan the
    // filesystem log dir so plain-text output streams are covered.
    let mut sources: Vec<(String, Vec<String>)> = Vec::new();

    #[cfg(feature = "signal-rewind")]
    {
        let db_path = project_root
            .join(".vac/signal")
            .join(format!("{}.db", session));
        if db_path.exists() {
            let store = vac_signal::rewind::RewindStore::open(&db_path)?;
            for stream in store.list_streams()? {
                let lines_owned = store.recent(&stream, 100)?;
                let lines: Vec<String> =
                    lines_owned.into_iter().map(|l| l.text).collect();
                sources.push((stream, lines));
            }
        }
    }
    #[cfg(not(feature = "signal-rewind"))]
    {
        let _ = &session;
    }

    // Filesystem fallback — always attempted.
    let log_dir = project_root.join(".vac/signal-logs");
    for (stream, lines) in scan_log_dir(&log_dir, 100)? {
        sources.push((stream, lines));
    }

    if sources.is_empty() {
        println!(
            "No signal sources found. Drop plain logs in {} or enable \
             `signal-rewind` for the SQLite archive.",
            log_dir.display(),
        );
        return Ok(());
    }

    for (stream, lines) in sources {
        let Some(kind) = classify_stream(&stream) else {
            continue;
        };
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        if let Some(desc) = detect_pattern(kind, &stream, &refs) {
            println!("Matched {} on stream {}", kind.label(), stream);
            let mut job = Job::new(JobKind::RunTask { description: desc });
            job.status = JobStatus::Suggested;
            queue.enqueue(job).await;
            hits += 1;
        }
    }
    println!("Assistant sweep complete; {hits} suggestion(s) enqueued");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_stream_routes_known_prefixes() {
        assert_eq!(
            classify_stream("build:cargo-test"),
            Some(PatternKind::BuildFailure)
        );
        assert_eq!(
            classify_stream("test:vac_session_engine"),
            Some(PatternKind::TestRegression)
        );
        assert_eq!(classify_stream("unknown:stream"), None);
        assert_eq!(classify_stream(""), None);
    }

    #[test]
    fn build_failure_fires_on_error_ecode() {
        let lines = vec!["Compiling vac_cli v0.1.0", "error[E0308]: mismatched types"];
        let out =
            detect_pattern(PatternKind::BuildFailure, "build:vac_cli", &lines).unwrap();
        assert!(out.contains("build:vac_cli"));
    }

    #[test]
    fn build_failure_quiet_on_clean_build() {
        let lines = vec!["Compiling x", "Finished dev profile"];
        assert!(detect_pattern(PatternKind::BuildFailure, "build:x", &lines).is_none());
    }

    #[test]
    fn test_regression_fires_on_failed_line() {
        let lines = vec![
            "running 3 tests",
            "test parse ... ok",
            "test submit ... FAILED",
        ];
        let out =
            detect_pattern(PatternKind::TestRegression, "test:vac_cli", &lines).unwrap();
        assert!(out.contains("test:vac_cli"));
    }

    #[test]
    fn test_regression_quiet_on_all_ok() {
        let lines = vec!["test parse ... ok", "test submit ... ok"];
        assert!(detect_pattern(PatternKind::TestRegression, "test:x", &lines).is_none());
    }

    #[test]
    fn pattern_kind_label_stable() {
        assert_eq!(PatternKind::BuildFailure.label(), "build-failure");
        assert_eq!(PatternKind::TestRegression.label(), "test-regression");
    }

    #[tokio::test]
    async fn execute_with_empty_project_is_noop_ok() {
        let tmp = tempfile::tempdir().unwrap();
        // No .vac/signal-logs dir, no DB — must succeed, not bail.
        execute(tmp.path().to_path_buf(), None).await.unwrap();
    }

    #[tokio::test]
    async fn execute_scans_filesystem_log_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = tmp.path().join(".vac/signal-logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(
            logs.join("build-cargo.log"),
            "Compiling x\nerror[E0308]: mismatched types\n",
        )
        .unwrap();
        execute(tmp.path().to_path_buf(), None).await.unwrap();
        // Queue file should exist with at least one suggested entry.
        let queue_path = tmp.path().join(".vac/queue.json");
        assert!(queue_path.exists(), "queue.json should be written");
        let raw = std::fs::read_to_string(&queue_path).unwrap();
        assert!(raw.contains("build-cargo"), "queue should reference stream: {raw}");
    }

    #[test]
    fn scan_log_dir_reads_tail_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = tmp.path().join(".vac/signal-logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("test-foo.log"), "a\nb\nc\nd\ne\n").unwrap();
        let out = scan_log_dir(&logs, 3).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "test-foo");
        assert_eq!(out[0].1, vec!["c", "d", "e"]);
    }

    #[test]
    fn scan_log_dir_missing_is_empty_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let out = scan_log_dir(&tmp.path().join("does-not-exist"), 10).unwrap();
        assert!(out.is_empty());
    }
}
