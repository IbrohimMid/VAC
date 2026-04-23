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
    if stream.starts_with("build:") {
        Some(PatternKind::BuildFailure)
    } else if stream.starts_with("test:") {
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

pub async fn execute(project_root: PathBuf, session_id: Option<String>) -> anyhow::Result<()> {
    #[cfg(feature = "signal-rewind")]
    {
        use vac_runtime::{Job, JobKind, JobStatus, TaskQueue};
        let session = session_id.unwrap_or_else(|| "default".to_string());
        let db_path = project_root
            .join(".vac/signal")
            .join(format!("{}.db", session));
        if !db_path.exists() {
            println!("No signal db found for session {session}");
            return Ok(());
        }

        let store = vac_signal::rewind::RewindStore::open(&db_path)?;
        let streams = store.list_streams()?;
        let queue = TaskQueue::with_storage(project_root.join(".vac/queue.json"));

        let mut hits = 0usize;
        for stream in streams {
            let Some(kind) = classify_stream(&stream) else {
                continue;
            };
            let lines_owned = store.recent(&stream, 100)?;
            let refs: Vec<&str> = lines_owned.iter().map(|l| l.text.as_str()).collect();
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
    #[cfg(not(feature = "signal-rewind"))]
    {
        let _ = (project_root, session_id);
        anyhow::bail!("vac assistant requires the `signal-rewind` feature");
    }
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

    #[cfg(not(feature = "signal-rewind"))]
    #[tokio::test]
    async fn execute_without_feature_errors_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let err = execute(tmp.path().to_path_buf(), None).await.unwrap_err();
        assert!(format!("{err}").contains("signal-rewind"));
    }
}
