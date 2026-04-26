//! Dev-only test helpers for the shell cockpit layer.
//!
//! IMPORTANT: This crate must NEVER be a production dependency.
//! Only allowed in `[dev-dependencies]` of other crates.

use std::fmt::Debug;
use std::path::{Path, PathBuf};
use vac_shell_contracts::{SessionToolSummary, ShellActivityEntry, ShellActivityKind, VacPaths};
use vac_shell_host_activity::ActivityLog;

// =====================================================================
// Fake path helpers
// =====================================================================

/// Minimal VacPaths impl backed by a tempdir.
pub struct FakeVacRoot {
    pub dir: tempfile::TempDir,
}

impl FakeVacRoot {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }

    pub fn sessions_dir(&self) -> PathBuf {
        let d = self.dir.path().join("sessions");
        std::fs::create_dir_all(&d).ok();
        d
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Flat VacPaths impl: every path method returns `root` (or a child).
/// Use when the test only needs a single directory for all paths.
pub struct FakeVacPaths(pub PathBuf);

impl VacPaths for FakeVacPaths {
    fn project_root(&self) -> PathBuf {
        self.0.clone()
    }
    fn sessions_dir(&self) -> PathBuf {
        self.0.clone()
    }
    fn project_state_dir(&self) -> PathBuf {
        self.0.clone()
    }
    fn user_state_dir(&self) -> PathBuf {
        self.0.clone()
    }
    fn plan_file(&self) -> PathBuf {
        self.0.join("plan.md")
    }
    fn model_selection_file(&self) -> PathBuf {
        self.0.join("model_selection.json")
    }
    fn commands_dir(&self) -> PathBuf {
        self.0.join("commands")
    }
}

/// Write a transcript JSONL file to `path` for testing.
/// Each row is written one per line with a trailing newline.
pub fn write_transcript_rows<I, S>(path: &Path, rows: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut content = String::new();
    for row in rows {
        let row = row.as_ref();
        if row.is_empty() {
            continue;
        }
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(row);
    }
    if !content.is_empty() {
        content.push('\n');
    }
    std::fs::write(path, content).expect("write transcript");
}

/// Write a multi-line JSONL body string to `path`.
/// Non-empty lines are written as-is; blank lines are skipped.
pub fn write_jsonl_body(path: &Path, body: &str) {
    write_transcript_rows(path, body.lines().filter(|l| !l.is_empty()));
}

// =====================================================================
// Transcript row builders
// =====================================================================

/// Build a minimal `tool_call` transcript row JSON string.
pub fn tool_call_json_line(id: &str, name: &str, args: serde_json::Value) -> String {
    serde_json::json!({
        "id": format!("row-{id}"),
        "session_id": "00000000-0000-0000-0000-000000000000",
        "kind": "tool_call",
        "timestamp": "2026-04-26T00:00:00Z",
        "content": {
            "id": id,
            "name": name,
            "arguments": args,
            "reason": null,
            "estimated_tokens": 0,
        }
    })
    .to_string()
}

/// Build a minimal `tool_result` transcript row JSON string.
pub fn tool_result_json_line(
    id: &str,
    name: &str,
    kind: &str,
    summary: &str,
    payload: serde_json::Value,
    duration_ms: u64,
) -> String {
    serde_json::json!({
        "id": format!("row-r-{id}"),
        "session_id": "00000000-0000-0000-0000-000000000000",
        "kind": "tool_result",
        "timestamp": "2026-04-26T00:00:00Z",
        "content": {
            "id": id,
            "name": name,
            "envelope": {
                "kind": kind,
                "payload": payload,
                "summary": summary,
                "duration_ms": duration_ms,
            }
        }
    })
    .to_string()
}

/// Append a minimal `tool_call` + `tool_result` pair to `path`.
/// The call uses empty arguments, and the result uses an empty payload
/// with zero duration.
pub fn write_tool_call_result_pair(path: &Path, id: &str, name: &str, kind: &str, summary: &str) {
    write_transcript_rows(
        path,
        [
            tool_call_json_line(id, name, serde_json::json!({})),
            tool_result_json_line(id, name, kind, summary, serde_json::json!({}), 0),
        ],
    );
}

fn finished_json_line(via: &str) -> String {
    serde_json::json!({
        "id": "row-finished",
        "session_id": "00000000-0000-0000-0000-000000000000",
        "kind": "finished",
        "timestamp": "2026-04-26T00:00:00Z",
        "content": {
            "via": via,
            "usage": {
                "input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": 0,
            }
        }
    })
    .to_string()
}

/// Append a minimal `finished` transcript row to `path`.
pub fn write_finished_row(path: &Path, via: &str) {
    let row = finished_json_line(via);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open transcript");
    use std::io::Write as _;
    writeln!(file, "{row}").expect("append finished row");
}

/// Read a JSONL transcript file into raw `serde_json::Value` rows.
pub fn read_jsonl_rows(path: &Path) -> Vec<serde_json::Value> {
    let body = std::fs::read_to_string(path).expect("read transcript");
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("valid jsonl"))
        .collect()
}

/// Write a plain-text session transcript at `path`.
pub fn temp_session_transcript(path: impl AsRef<Path>, body: &str) -> PathBuf {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create transcript parent");
    }
    std::fs::write(path, body).expect("write transcript");
    path.to_path_buf()
}

// =====================================================================
// Activity log assertions
// =====================================================================

/// Assert that no snapshot entry's debug representation contains `needle`.
pub fn assert_activity_log_not_contains(log: &ActivityLog, needle: &str) {
    let snap = log.snapshot();
    let serialized = format!("{snap:?}");
    assert!(
        !serialized.contains(needle),
        "expected '{needle}' NOT in activity log, but found it: {serialized}"
    );
}

/// Assert that at least one snapshot entry matches `kind`.
pub fn assert_activity_log_contains_kind(log: &ActivityLog, kind: ShellActivityKind) {
    let snap = log.snapshot();
    assert!(
        snap.iter().any(|e| e.kind == kind),
        "expected kind {kind:?} in activity log, got: {snap:?}"
    );
}

/// Assert that at least one snapshot entry of `kind` has `severity` matching expectation.
pub fn assert_activity_log_kind_severity(
    log: &ActivityLog,
    kind: ShellActivityKind,
    expected_severity: vac_shell_contracts::Severity,
) {
    let snap = log.snapshot();
    let entry = snap
        .iter()
        .find(|e| e.kind == kind)
        .unwrap_or_else(|| panic!("no entry with kind {kind:?} in log: {snap:?}"));
    assert_eq!(
        entry.severity, expected_severity,
        "kind {kind:?} has wrong severity: got {:?}, expected {:?}",
        entry.severity, expected_severity
    );
}

/// Assert that the `Debug` representation of `value` does not
/// contain `secret`.
pub fn assert_no_secret_in_debug<T: Debug>(value: &T, secret: &str) {
    let rendered = format!("{value:?}");
    assert!(
        !rendered.contains(secret),
        "secret '{secret}' found in debug output: {rendered}"
    );
}

// =====================================================================
// Session summary provider helpers
// =====================================================================

use std::collections::HashMap;
use std::sync::Arc;

/// Build a session_tool_summary_provider callback from a path→summary map.
/// Useful for injecting into ShellApp in tests.
pub fn session_summary_provider_from_map(
    map: HashMap<PathBuf, SessionToolSummary>,
) -> Arc<dyn Fn(&Path) -> Option<SessionToolSummary> + Send + Sync> {
    Arc::new(move |path: &Path| map.get(path).cloned())
}

/// Build a session_tool_summary_provider that always returns the
/// same value for every path.
pub fn fake_session_summary_provider(
    summary: Option<SessionToolSummary>,
) -> Arc<dyn Fn(&Path) -> Option<SessionToolSummary> + Send + Sync> {
    Arc::new(move |_: &Path| summary.clone())
}

/// Build a session_tool_summary_provider that always returns None.
pub fn no_summary_provider() -> Arc<dyn Fn(&Path) -> Option<SessionToolSummary> + Send + Sync> {
    fake_session_summary_provider(None)
}

// =====================================================================
// Simple test assertions
// =====================================================================

/// Assert that the serialized form of `entries` does not contain `secret`.
pub fn assert_entries_not_contain(entries: &[ShellActivityEntry], secret: &str) {
    let s = format!("{entries:?}");
    assert!(
        !s.contains(secret),
        "secret '{secret}' found in entries: {s}"
    );
}
