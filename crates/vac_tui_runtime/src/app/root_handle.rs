//! W6.1 — shared root handle for subagents.
//!
//! Multi-level subagent nests (parent → child → grandchild) need to
//! push into a small set of root-scoped fields without racing the
//! parent's UI frame: notification drops, tool-use counter bumps,
//! subagent breadcrumbs. `AppStateRootHandle` is the primitive:
//! it wraps an `Arc<tokio::sync::RwLock<RootObservables>>` so each
//! nest level holds a cheap clone and mutations always land at the
//! root.
//!
//! Design goals (from the cc-parity plan):
//!
//! 1. Cheap to clone — spawning a subagent must be O(1) w.r.t. the
//!    handle, not O(fields).
//! 2. No held cross-guard deadlocks — every mutation takes the root
//!    guard, releases before acking. The API is shape-limited to
//!    `push_*` and `set_*` so a caller cannot accidentally hold a
//!    write guard across an `.await`.
//! 3. Bounded growth — notifications + breadcrumbs are ring buffers;
//!    the tool counter is a monotonic u64 (saturating add).
//!
//! W6.2 wires this into the subagent runner so every nest level sees
//! the same root.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::RwLock;

/// One pushed notification. `source` identifies the nest level so
/// the renderer can show "parent" / "child" / "grandchild" tags
/// without duplicating payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootNotification {
    pub source: String,
    pub level: NotificationLevel,
    pub message: String,
    pub ts_unix: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotificationLevel {
    Info,
    Warn,
    Error,
}

/// One breadcrumb — a single tool use observed by a subagent. Used
/// so the renderer can display an aggregate "subagent X did Y, Z"
/// row without the parent owning the full transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBreadcrumb {
    pub agent: String,
    pub tool: String,
    pub summary: String,
    pub ts_unix: i64,
}

/// What the handle actually guards. Ring-buffered; max sizes are
/// the constants below so a misbehaving subagent can't balloon the
/// root.
#[derive(Debug, Default)]
pub struct RootObservables {
    pub notifications: VecDeque<RootNotification>,
    pub breadcrumbs: VecDeque<AgentBreadcrumb>,
    pub tool_counter: u64,
    pub errors_seen: u64,
}

pub const NOTIFICATION_RING_CAP: usize = 128;
pub const BREADCRUMB_RING_CAP: usize = 256;

/// Per-field length caps (chars, not bytes — keeps UTF-8 safe).
/// Prevents a misbehaving subagent from OOMing the parent via a
/// single giant notification / breadcrumb payload.
pub const NOTIFICATION_MESSAGE_CAP: usize = 4 * 1024;
pub const BREADCRUMB_FIELD_CAP: usize = 1 * 1024;
pub const AGENT_IDENT_CAP: usize = 128;

/// Truncate a string to `cap` chars, UTF-8-safe. Appends an ellipsis
/// marker when truncation occurred AND there's room for it; for very
/// small caps the marker is dropped so the returned string never
/// exceeds `cap` chars.
pub(crate) fn truncate_to_cap(s: &str, cap: usize) -> String {
    if s.chars().count() <= cap {
        return s.to_string();
    }
    const MARK: &str = "…[truncated]";
    let mark_len = MARK.chars().count();
    if cap <= mark_len {
        // No room for the marker — return just the first `cap` chars.
        return s.chars().take(cap).collect();
    }
    let keep = cap - mark_len;
    let head: String = s.chars().take(keep).collect();
    format!("{head}{MARK}")
}

/// Sanitise an identifier string — used for `agent`, `source`,
/// `tool` fields where control chars would pollute transcripts and
/// status renderings. Strips ASCII control chars (including `\n`,
/// `\r`, `\t`) and caps length.
pub(crate) fn sanitise_ident(s: &str, cap: usize) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    truncate_to_cap(&cleaned, cap)
}

/// Root handle. Clone is cheap — the `Arc` just bumps a refcount.
#[derive(Debug, Clone)]
pub struct AppStateRootHandle {
    inner: Arc<RwLock<RootObservables>>,
}

impl Default for AppStateRootHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl AppStateRootHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RootObservables::default())),
        }
    }

    /// Push a notification. Ring-buffered at `NOTIFICATION_RING_CAP`
    /// so a runaway subagent can't OOM the root. Fields are
    /// sanitised + capped: `source` has control chars stripped and
    /// is clipped to `AGENT_IDENT_CAP`; `message` is clipped to
    /// `NOTIFICATION_MESSAGE_CAP` chars.
    ///
    /// `errors_seen` is a **lifetime** counter — it counts every
    /// Error-level push, including ones later displaced from the
    /// ring. This is intentional: operators want "how many errors
    /// has this session ever seen", not "how many are currently on
    /// screen".
    pub async fn push_notification(&self, n: RootNotification) {
        let n = RootNotification {
            source: sanitise_ident(&n.source, AGENT_IDENT_CAP),
            level: n.level,
            message: truncate_to_cap(&n.message, NOTIFICATION_MESSAGE_CAP),
            ts_unix: n.ts_unix,
        };
        let mut guard = self.inner.write().await;
        if matches!(n.level, NotificationLevel::Error) {
            guard.errors_seen = guard.errors_seen.saturating_add(1);
        }
        if guard.notifications.len() >= NOTIFICATION_RING_CAP {
            guard.notifications.pop_front();
        }
        guard.notifications.push_back(n);
    }

    /// Push a breadcrumb. Ring-buffered at `BREADCRUMB_RING_CAP`.
    /// Field-level caps match `push_notification`: `agent` and
    /// `tool` are sanitised identifiers; `summary` is length-capped.
    pub async fn push_breadcrumb(&self, b: AgentBreadcrumb) {
        let b = AgentBreadcrumb {
            agent: sanitise_ident(&b.agent, AGENT_IDENT_CAP),
            tool: sanitise_ident(&b.tool, AGENT_IDENT_CAP),
            summary: truncate_to_cap(&b.summary, BREADCRUMB_FIELD_CAP),
            ts_unix: b.ts_unix,
        };
        let mut guard = self.inner.write().await;
        guard.tool_counter = guard.tool_counter.saturating_add(1);
        if guard.breadcrumbs.len() >= BREADCRUMB_RING_CAP {
            guard.breadcrumbs.pop_front();
        }
        guard.breadcrumbs.push_back(b);
    }

    /// Bump the tool counter without attaching a breadcrumb. Used
    /// by callers that want the aggregate count (statusline) but
    /// already have breadcrumb-shaped data elsewhere.
    pub async fn bump_tool_counter(&self) -> u64 {
        let mut guard = self.inner.write().await;
        guard.tool_counter = guard.tool_counter.saturating_add(1);
        guard.tool_counter
    }

    /// Snapshot of notifications. Callers use this to render; the
    /// snapshot is owned so the read-guard drops immediately.
    pub async fn notifications(&self) -> Vec<RootNotification> {
        self.inner.read().await.notifications.iter().cloned().collect()
    }

    pub async fn breadcrumbs(&self) -> Vec<AgentBreadcrumb> {
        self.inner.read().await.breadcrumbs.iter().cloned().collect()
    }

    pub async fn tool_counter(&self) -> u64 {
        self.inner.read().await.tool_counter
    }

    pub async fn errors_seen(&self) -> u64 {
        self.inner.read().await.errors_seen
    }

    /// Drain all notifications; the renderer marks them read.
    pub async fn drain_notifications(&self) -> Vec<RootNotification> {
        let mut guard = self.inner.write().await;
        guard.notifications.drain(..).collect()
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn notif(src: &str, msg: &str) -> RootNotification {
        RootNotification {
            source: src.into(),
            level: NotificationLevel::Info,
            message: msg.into(),
            ts_unix: 0,
        }
    }

    fn err_notif(src: &str, msg: &str) -> RootNotification {
        RootNotification {
            source: src.into(),
            level: NotificationLevel::Error,
            message: msg.into(),
            ts_unix: 0,
        }
    }

    fn bc(agent: &str, tool: &str) -> AgentBreadcrumb {
        AgentBreadcrumb {
            agent: agent.into(),
            tool: tool.into(),
            summary: String::new(),
            ts_unix: 0,
        }
    }

    #[tokio::test]
    async fn new_handle_is_empty() {
        let h = AppStateRootHandle::new();
        assert!(h.notifications().await.is_empty());
        assert!(h.breadcrumbs().await.is_empty());
        assert_eq!(h.tool_counter().await, 0);
        assert_eq!(h.errors_seen().await, 0);
    }

    #[tokio::test]
    async fn clone_is_shared_not_copied() {
        let h1 = AppStateRootHandle::new();
        let h2 = h1.clone();
        h1.push_notification(notif("parent", "one")).await;
        let seen_via_clone = h2.notifications().await;
        assert_eq!(seen_via_clone.len(), 1);
        assert_eq!(seen_via_clone[0].source, "parent");
    }

    #[tokio::test]
    async fn two_level_nest_push_3_notifications() {
        // W6.1 acceptance from the plan: parent → child → grandchild
        // each pushing one notification; all three land at root.
        let parent = AppStateRootHandle::new();
        let child = parent.clone();
        let grandchild = child.clone();
        parent.push_notification(notif("parent", "a")).await;
        child.push_notification(notif("child", "b")).await;
        grandchild.push_notification(notif("grandchild", "c")).await;
        let all = parent.notifications().await;
        assert_eq!(all.len(), 3);
        let sources: Vec<&str> = all.iter().map(|n| n.source.as_str()).collect();
        assert_eq!(sources, vec!["parent", "child", "grandchild"]);
    }

    #[tokio::test]
    async fn errors_seen_counts_only_error_level() {
        let h = AppStateRootHandle::new();
        h.push_notification(notif("x", "info")).await;
        h.push_notification(err_notif("x", "boom")).await;
        h.push_notification(err_notif("y", "crash")).await;
        assert_eq!(h.errors_seen().await, 2);
    }

    #[tokio::test]
    async fn notifications_ring_drops_oldest() {
        let h = AppStateRootHandle::new();
        for i in 0..NOTIFICATION_RING_CAP + 10 {
            h.push_notification(notif("x", &format!("n{i}"))).await;
        }
        let all = h.notifications().await;
        assert_eq!(all.len(), NOTIFICATION_RING_CAP);
        // Oldest 10 were dropped → first visible is "n10".
        assert_eq!(all.first().unwrap().message, "n10");
        assert_eq!(
            all.last().unwrap().message,
            format!("n{}", NOTIFICATION_RING_CAP + 9),
        );
    }

    #[tokio::test]
    async fn breadcrumbs_ring_drops_oldest() {
        let h = AppStateRootHandle::new();
        for i in 0..BREADCRUMB_RING_CAP + 5 {
            h.push_breadcrumb(bc("a", &format!("t{i}"))).await;
        }
        let all = h.breadcrumbs().await;
        assert_eq!(all.len(), BREADCRUMB_RING_CAP);
    }

    #[tokio::test]
    async fn breadcrumb_bumps_tool_counter() {
        let h = AppStateRootHandle::new();
        h.push_breadcrumb(bc("x", "tool1")).await;
        h.push_breadcrumb(bc("x", "tool2")).await;
        assert_eq!(h.tool_counter().await, 2);
    }

    #[tokio::test]
    async fn bump_tool_counter_returns_new_value() {
        let h = AppStateRootHandle::new();
        assert_eq!(h.bump_tool_counter().await, 1);
        assert_eq!(h.bump_tool_counter().await, 2);
        assert_eq!(h.bump_tool_counter().await, 3);
    }

    #[tokio::test]
    async fn drain_notifications_empties_root() {
        let h = AppStateRootHandle::new();
        h.push_notification(notif("x", "a")).await;
        h.push_notification(notif("x", "b")).await;
        let drained = h.drain_notifications().await;
        assert_eq!(drained.len(), 2);
        assert!(h.notifications().await.is_empty());
    }

    #[test]
    fn truncate_helper_preserves_short_strings() {
        assert_eq!(truncate_to_cap("hello", 10), "hello");
    }

    #[test]
    fn truncate_helper_marks_overlong_strings() {
        let long: String = "x".repeat(200);
        let out = truncate_to_cap(&long, 32);
        assert!(out.chars().count() <= 32);
        assert!(out.contains("truncated"));
    }

    #[test]
    fn truncate_helper_is_utf8_safe() {
        let multibyte: String = "日".repeat(100);
        let out = truncate_to_cap(&multibyte, 10);
        assert!(out.chars().count() <= 10);
    }

    #[test]
    fn sanitise_ident_strips_control_chars() {
        let evil = "parent\nparent\0fake";
        let out = sanitise_ident(evil, 64);
        assert!(!out.contains('\n'));
        assert!(!out.contains('\0'));
        assert_eq!(out, "parentparentfake");
    }

    #[test]
    fn sanitise_ident_caps_length() {
        let long: String = "a".repeat(200);
        let out = sanitise_ident(&long, 16);
        assert!(out.chars().count() <= 16);
    }

    #[tokio::test]
    async fn oversized_message_is_truncated_in_ring() {
        let h = AppStateRootHandle::new();
        let huge: String = "x".repeat(NOTIFICATION_MESSAGE_CAP * 4);
        h.push_notification(RootNotification {
            source: "x".into(),
            level: NotificationLevel::Info,
            message: huge,
            ts_unix: 0,
        })
        .await;
        let seen = h.notifications().await;
        assert_eq!(seen.len(), 1);
        let n_chars = seen[0].message.chars().count();
        assert!(
            n_chars <= NOTIFICATION_MESSAGE_CAP,
            "message should be capped, got {n_chars}",
        );
    }

    #[tokio::test]
    async fn control_char_source_is_sanitised() {
        let h = AppStateRootHandle::new();
        h.push_notification(RootNotification {
            source: "child\nspoof".into(),
            level: NotificationLevel::Info,
            message: "msg".into(),
            ts_unix: 0,
        })
        .await;
        let seen = h.notifications().await;
        assert_eq!(seen[0].source, "childspoof");
    }

    #[tokio::test]
    async fn oversized_breadcrumb_summary_is_truncated() {
        let h = AppStateRootHandle::new();
        let huge: String = "y".repeat(BREADCRUMB_FIELD_CAP * 4);
        h.push_breadcrumb(AgentBreadcrumb {
            agent: "a".into(),
            tool: "t".into(),
            summary: huge,
            ts_unix: 0,
        })
        .await;
        let seen = h.breadcrumbs().await;
        assert!(seen[0].summary.chars().count() <= BREADCRUMB_FIELD_CAP);
    }

    #[tokio::test]
    async fn concurrent_pushes_are_serialised_no_deadlock() {
        let h = AppStateRootHandle::new();
        let mut joins = Vec::new();
        for i in 0..16 {
            let clone = h.clone();
            joins.push(tokio::spawn(async move {
                clone.push_notification(notif("p", &format!("n{i}"))).await;
                clone.push_breadcrumb(bc("p", &format!("t{i}"))).await;
            }));
        }
        for j in joins {
            j.await.unwrap();
        }
        assert_eq!(h.notifications().await.len(), 16);
        assert_eq!(h.breadcrumbs().await.len(), 16);
        assert_eq!(h.tool_counter().await, 16);
    }
}
