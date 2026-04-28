//! C.3 — MonitorTool primitive: stream stdout lines from a
//! subprocess, filter through a regex allowlist, route each match
//! to NotifyRouter.
//!
//! This module owns the *plumbing* — spawning the child, reading
//! its stdout line by line, applying the filter regex, and
//! yielding events. The Phase C.5 hook registry landing bridges
//! each event into the TUI activity lane via the existing
//! tracing subsystem (target `vac_tui_runtime::monitor`).

use std::process::Stdio;

use futures::Stream;
use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::error::{EngineError, EngineResult};

/// One matched line emitted by the monitor.
#[derive(Debug, Clone)]
pub struct MonitorLine {
    /// Short operator label (e.g. `"error"`, `"warn"`, `"info"`).
    /// Derived from the regex match: if the regex has a named
    /// capture `severity`, that value wins; otherwise `"info"`.
    pub severity: String,
    /// The matched line, stripped of trailing CR / LF.
    pub line: String,
    /// Monotonic sequence number assigned by the monitor runtime.
    pub seq: u64,
}

/// Operator-supplied configuration.
#[derive(Debug, Clone)]
pub struct MonitorSpec {
    /// Shell-tokenised argv (first element = program).
    pub argv: Vec<String>,
    /// Regex every stdout line is matched against. Lines that
    /// don't match are dropped (this is the "signal vs noise"
    /// guard the plan requires).
    pub match_regex: String,
    /// Cap on the number of lines emitted before the monitor
    /// auto-terminates — prevents a runaway child from flooding
    /// NotifyRouter. Zero means unlimited (operator opt-in).
    pub max_lines: u64,
}

impl MonitorSpec {
    pub fn new(argv: Vec<String>, match_regex: impl Into<String>) -> Self {
        Self {
            argv,
            match_regex: match_regex.into(),
            max_lines: 1_000,
        }
    }
}

/// Output of [`spawn_monitor`]: a stream of matched lines + the
/// child handle so callers can `kill()` on cancel.
pub struct MonitorHandle {
    pub lines: Box<dyn Stream<Item = MonitorLine> + Send + Unpin + 'static>,
    pub child: Child,
}

/// Spawn the monitor.
pub async fn spawn_monitor(spec: MonitorSpec) -> EngineResult<MonitorHandle> {
    if spec.argv.is_empty() {
        return Err(EngineError::Other("monitor argv is empty".into()));
    }
    let regex = Regex::new(&spec.match_regex).map_err(|e| {
        EngineError::Other(format!("invalid monitor regex '{}': {e}", spec.match_regex,))
    })?;
    let severity_group_idx = regex.capture_names().position(|n| n == Some("severity"));

    let mut cmd = Command::new(&spec.argv[0]);
    cmd.args(&spec.argv[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(EngineError::from)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EngineError::Other("monitor child has no stdout".into()))?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<MonitorLine>();
    let max = spec.max_lines;
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut seq: u64 = 0;
        loop {
            let line = match lines.next_line().await {
                Ok(Some(line)) => line,
                _ => break,
            };
            let line_ref = line.trim_end_matches(['\r', '\n']);
            let caps = match regex.captures(line_ref) {
                Some(c) => c,
                None => continue,
            };
            let severity = match severity_group_idx {
                Some(idx) => caps
                    .get(idx)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| "info".into()),
                None => "info".to_string(),
            };
            seq += 1;
            if tx
                .send(MonitorLine {
                    severity,
                    line: line_ref.to_string(),
                    seq,
                })
                .is_err()
            {
                break;
            }
            if max > 0 && seq >= max {
                tracing::warn!(
                    target: "vac_tui_runtime::monitor",
                    max,
                    "monitor line cap reached — terminating",
                );
                break;
            }
        }
    });

    Ok(MonitorHandle {
        lines: Box::new(UnboundedReceiverStream::new(rx)),
        child,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn monitor_streams_matching_lines() {
        // Use sh -c echo to avoid dependency on external tools.
        let spec = MonitorSpec::new(
            vec![
                "sh".into(),
                "-c".into(),
                "printf 'foo\\nERROR oops\\nbar\\nWARN slow\\n'".into(),
            ],
            r"^(?P<severity>ERROR|WARN)",
        );
        let mut handle = spawn_monitor(spec).await.unwrap();
        let mut collected: Vec<MonitorLine> = Vec::new();
        while let Some(line) = handle.lines.next().await {
            collected.push(line);
        }
        let _ = handle.child.wait().await;
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].severity, "ERROR");
        assert_eq!(collected[1].severity, "WARN");
    }

    #[tokio::test]
    async fn monitor_respects_max_lines() {
        let spec = MonitorSpec {
            argv: vec![
                "sh".into(),
                "-c".into(),
                "printf 'hit\\nhit\\nhit\\nhit\\nhit\\n'".into(),
            ],
            match_regex: "hit".into(),
            max_lines: 2,
        };
        let mut handle = spawn_monitor(spec).await.unwrap();
        let mut n = 0;
        while let Some(_) = handle.lines.next().await {
            n += 1;
        }
        let _ = handle.child.wait().await;
        assert_eq!(n, 2);
    }

    #[tokio::test]
    async fn monitor_rejects_bad_regex() {
        let spec = MonitorSpec::new(vec!["sh".into(), "-c".into(), "echo x".into()], "(");
        match spawn_monitor(spec).await {
            Err(e) => assert!(format!("{e}").contains("invalid monitor regex")),
            Ok(_) => panic!("expected error on bad regex"),
        }
    }
}
