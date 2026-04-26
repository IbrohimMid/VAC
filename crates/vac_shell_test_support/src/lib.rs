//! Dev-only test helpers for the shell cockpit layer.
//!
//! IMPORTANT: This crate must NEVER be a production dependency.
//! Only allowed in `[dev-dependencies]` of other crates.

use std::path::{Path, PathBuf};
use vac_shell_contracts::{SessionToolSummary, ShellActivityEntry, ShellActivityKind};
use vac_shell_host_activity::ActivityLog;

// =====================================================================
// Fake path helpers
// =====================================================================

/// Minimal VacPaths impl backed by a tempdir.
/// Use `temp_vac_root()` to construct.
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

/// Write a minimal transcript JSONL file to `path` for testing.
/// Each `rows` entry is a raw JSON string (one per line).
pub fn write_transcript_rows(path: &Path, rows: &[&str]) {
    let content = rows.join("\n") + "\n";
    std::fs::write(path, content).expect("write transcript");
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

/// Build a session_tool_summary_provider that always returns None.
pub fn no_summary_provider(
) -> Arc<dyn Fn(&Path) -> Option<SessionToolSummary> + Send + Sync> {
    Arc::new(|_: &Path| None)
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
