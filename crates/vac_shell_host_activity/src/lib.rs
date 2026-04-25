//! Slice 14 — host-side activity log.
//!
//! Bounded ring buffer of [`ShellActivityEntry`]. Hosts push events
//! through `record(...)` or via the projection helpers
//! (`record_tool_call`, `record_approval_requested`,
//! `record_model_changed`, …) so the on-disk shape is consistent
//! across call sites. The widget reads `snapshot()`.

use std::sync::{Arc, Mutex};

use vac_shell_contracts::{Severity, ShellActivityEntry, ShellActivityKind};

const DEFAULT_CAP: usize = 500;

#[derive(Debug, Clone)]
pub struct ActivityLog {
    cap: usize,
    inner: Arc<Mutex<Vec<ShellActivityEntry>>>,
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self::new(DEFAULT_CAP)
    }
}

impl ActivityLog {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn record(&self, entry: ShellActivityEntry) {
        let mut g = self.inner.lock().expect("activity log lock");
        g.push(entry);
        if g.len() > self.cap {
            let drop = g.len() - self.cap;
            g.drain(0..drop);
        }
    }

    pub fn snapshot(&self) -> Vec<ShellActivityEntry> {
        self.inner.lock().expect("activity log lock").clone()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("activity log lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // ----- projection helpers -----

    pub fn record_user_input(&self, id: impl Into<String>, ts_unix: u64, text: impl Into<String>) {
        self.record(ShellActivityEntry {
            id: id.into(),
            ts_unix,
            kind: ShellActivityKind::UserInput,
            title: text.into(),
            detail: None,
            severity: Severity::Info,
        });
    }

    pub fn record_tool_call(
        &self,
        id: impl Into<String>,
        ts_unix: u64,
        tool: impl Into<String>,
        args_summary: Option<String>,
    ) {
        self.record(ShellActivityEntry {
            id: id.into(),
            ts_unix,
            kind: ShellActivityKind::ToolCall,
            title: tool.into(),
            detail: args_summary,
            severity: Severity::Info,
        });
    }

    pub fn record_approval_requested(
        &self,
        id: impl Into<String>,
        ts_unix: u64,
        tool: impl Into<String>,
    ) {
        self.record(ShellActivityEntry {
            id: id.into(),
            ts_unix,
            kind: ShellActivityKind::ApprovalRequested,
            title: tool.into(),
            detail: None,
            severity: Severity::Warn,
        });
    }

    pub fn record_approval_resolved(
        &self,
        id: impl Into<String>,
        ts_unix: u64,
        tool: impl Into<String>,
        approved: bool,
    ) {
        self.record(ShellActivityEntry {
            id: id.into(),
            ts_unix,
            kind: ShellActivityKind::ApprovalResolved,
            title: tool.into(),
            detail: Some(if approved { "approved".into() } else { "rejected".into() }),
            severity: if approved { Severity::Ok } else { Severity::Warn },
        });
    }

    pub fn record_model_changed(
        &self,
        id: impl Into<String>,
        ts_unix: u64,
        provider: impl Into<String>,
        model_id: impl Into<String>,
    ) {
        self.record(ShellActivityEntry {
            id: id.into(),
            ts_unix,
            kind: ShellActivityKind::ModelChanged,
            title: format!("{} / {}", provider.into(), model_id.into()),
            detail: None,
            severity: Severity::Ok,
        });
    }

    pub fn record_error(
        &self,
        id: impl Into<String>,
        ts_unix: u64,
        title: impl Into<String>,
        detail: Option<String>,
    ) {
        self.record(ShellActivityEntry {
            id: id.into(),
            ts_unix,
            kind: ShellActivityKind::Error,
            title: title.into(),
            detail,
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_helpers_record_with_correct_kind_and_severity() {
        let log = ActivityLog::default();
        log.record_user_input("a", 1, "hi");
        log.record_tool_call("b", 2, "shell", Some("ls".into()));
        log.record_approval_requested("c", 3, "shell");
        log.record_approval_resolved("d", 4, "shell", true);
        log.record_model_changed("e", 5, "openai", "gpt-4o");
        log.record_error("f", 6, "boom", None);
        let snap = log.snapshot();
        assert_eq!(snap.len(), 6);
        assert_eq!(snap[0].kind, ShellActivityKind::UserInput);
        assert_eq!(snap[0].severity, Severity::Info);
        assert_eq!(snap[1].kind, ShellActivityKind::ToolCall);
        assert_eq!(snap[2].severity, Severity::Warn);
        assert_eq!(snap[3].severity, Severity::Ok);
        assert_eq!(snap[4].kind, ShellActivityKind::ModelChanged);
        assert_eq!(snap[5].severity, Severity::Error);
    }

    #[test]
    fn ring_buffer_drops_oldest_past_cap() {
        let log = ActivityLog::new(3);
        for i in 0..10 {
            log.record_user_input(format!("id-{i}"), i, format!("line {i}"));
        }
        let snap = log.snapshot();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].title, "line 7");
        assert_eq!(snap[2].title, "line 9");
    }
}
