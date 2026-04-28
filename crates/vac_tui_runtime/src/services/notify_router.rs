//! U4 — `NotifyRouter`: one grammar for every user-visible output.
//!
//! Before U4 the codebase had ~86 direct call sites across 18 files
//! that pushed into `execution.activity`, produced `Toast`s, or
//! showed `Banner`s with their own ad-hoc rendering conventions.
//! Each subsystem picked its own lane and severity wording — so
//! two similar-importance events surfaced as different UX types.
//!
//! `NotifyRouter` is the single entry point. It takes a
//! `NotifyEvent { severity, subsystem, summary, detail }` and
//! routes through the three EXISTING lanes the TUI already speaks:
//!
//! - `Info`            → `push_activity` (Info kind)
//! - `Warn`            → `push_activity` (Warning kind) + toast
//! - `NonBlockingCritical` → `push_activity` (Error) + banner
//! - `OperatorDecision`    → existing approval / ask-user / reject-
//!                            reason modal path (reserved; router
//!                            does not surface it directly)
//!
//! No new modal class. No new event plane. No new storage.
//! Severity maps mechanically to a lane; subsystem string is the
//! shared grammar with `SystemPulse::label`.

use crate::app::{ActivityKind, AppState};

/// Upper bounds on NotifyEvent string fields. Prevents a buggy
/// producer from pushing a multi-MB message into the activity ring
/// or toast / banner surfaces.
pub const NOTIFY_SUBSYSTEM_CAP: usize = 64;
pub const NOTIFY_SUMMARY_CAP: usize = 512;
pub const NOTIFY_DETAIL_CAP: usize = 4 * 1024;

fn clamp_chars(s: &str, cap: usize) -> String {
    if s.chars().count() <= cap {
        return s.to_string();
    }
    const MARK: &str = "…";
    let keep = cap.saturating_sub(MARK.chars().count()).max(1);
    let head: String = s.chars().take(keep).collect();
    format!("{head}{MARK}")
}

/// Severity lanes. Maps to the three existing surfaces.
/// `OperatorDecision` is reserved — routers must not auto-fire it;
/// approval/ask-user flows own that path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotifySeverity {
    Info,
    Warn,
    NonBlockingCritical,
}

impl NotifySeverity {
    /// Map our severity lane to an existing `ActivityKind`. There is
    /// no `Info` / `Warning` variant on ActivityKind — it uses
    /// domain-shaped kinds (`Status` for normal info, `Error` for
    /// failure). Warn folds into `Status` because that's where most
    /// low-severity system events live today; toast does the
    /// attention-grabbing for warnings.
    fn to_activity_kind(self) -> ActivityKind {
        match self {
            Self::Info => ActivityKind::Status,
            Self::Warn => ActivityKind::Status,
            Self::NonBlockingCritical => ActivityKind::Error,
        }
    }
}

/// One structured notification. Subsystem is a short ident
/// (`approvals`, `runtime`, `mcp`, `policy`, …) that matches
/// `SystemPulse::SystemFacetKind::label` so operators see the
/// same vocabulary everywhere.
#[derive(Debug, Clone)]
pub struct NotifyEvent {
    pub severity: NotifySeverity,
    pub subsystem: String,
    pub summary: String,
    pub detail: Option<String>,
}

impl NotifyEvent {
    pub fn info(subsystem: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            severity: NotifySeverity::Info,
            subsystem: subsystem.into(),
            summary: summary.into(),
            detail: None,
        }
    }
    pub fn warn(subsystem: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            severity: NotifySeverity::Warn,
            subsystem: subsystem.into(),
            summary: summary.into(),
            detail: None,
        }
    }
    pub fn critical(subsystem: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            severity: NotifySeverity::NonBlockingCritical,
            subsystem: subsystem.into(),
            summary: summary.into(),
            detail: None,
        }
    }
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Route a `NotifyEvent` through the three existing lanes. Adds
/// one activity row unconditionally; toasts / banners fire
/// according to severity. The activity row includes the subsystem
/// prefix so operators can scan origin at a glance — matches the
/// SystemPulse panel's grammar.
pub fn route(state: &mut AppState, event: NotifyEvent) {
    // Clamp every field before rendering — a producer pushing a
    // 10 MB message would otherwise OOM the activity ring and
    // toast pipeline.
    let subsystem = clamp_chars(&event.subsystem, NOTIFY_SUBSYSTEM_CAP);
    let summary = clamp_chars(&event.summary, NOTIFY_SUMMARY_CAP);
    let prefixed = format!("[{}] {}", subsystem, summary);
    let kind = event.severity.to_activity_kind();
    state.push_activity(kind, prefixed.clone());

    match event.severity {
        NotifySeverity::Info => {
            // Activity-only. No toast pressure on steady-state ops.
        }
        NotifySeverity::Warn => {
            push_toast(state, &prefixed);
        }
        NotifySeverity::NonBlockingCritical => {
            push_banner(state, &prefixed);
        }
    }
}

/// Push a warning-styled toast onto the existing toast ring.
/// Uses the same constructors + 3-max cap the rest of the TUI
/// relies on via `event_loop`; we don't enforce that cap here
/// since `event_loop` already does after every tick.
fn push_toast(state: &mut AppState, msg: &str) {
    state.layout.toasts.push(crate::services::Toast::warning(
        msg,
        std::time::Duration::from_secs(4),
    ));
}

fn push_banner(state: &mut AppState, msg: &str) {
    // Banner single-slot: overwrite whatever's there. The Error
    // style gives the usual red top-strip rendering.
    state.layout.banner.message = Some(crate::services::banner::BannerMessage::new(
        msg,
        crate::services::banner::BannerStyle::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> AppState {
        AppState::default()
    }

    #[test]
    fn info_routes_to_activity_only() {
        let mut state = fresh();
        route(&mut state, NotifyEvent::info("approvals", "one pending"));
        assert_eq!(state.execution.activity.len(), 1);
        assert!(
            state.execution.activity[0]
                .message
                .starts_with("[approvals]")
        );
        assert_eq!(state.execution.activity[0].kind, ActivityKind::Status);
        assert!(state.layout.banner.message.is_none());
    }

    #[test]
    fn warn_routes_to_activity_plus_toast() {
        let mut state = fresh();
        route(&mut state, NotifyEvent::warn("runtime", "3 retries"));
        assert_eq!(state.execution.activity.len(), 1);
        // Warn folds into Status at the activity layer — see
        // NotifySeverity::to_activity_kind docs. Toast does the
        // attention-grabbing part.
        assert_eq!(state.execution.activity[0].kind, ActivityKind::Status);
        assert_eq!(state.layout.toasts.len(), 1);
        assert!(state.layout.toasts[0].message.contains("runtime"));
    }

    #[test]
    fn non_blocking_critical_routes_to_activity_plus_banner() {
        let mut state = fresh();
        route(
            &mut state,
            NotifyEvent::critical("mcp", "github server failed").with_detail("401"),
        );
        assert_eq!(state.execution.activity.len(), 1);
        assert_eq!(state.execution.activity[0].kind, ActivityKind::Error);
        assert!(state.layout.banner.message.is_some());
        assert!(
            state
                .layout
                .banner
                .message
                .as_ref()
                .unwrap()
                .text
                .starts_with("[mcp]")
        );
    }

    #[test]
    fn subsystem_prefix_is_consistent() {
        let mut state = fresh();
        route(&mut state, NotifyEvent::info("approvals", "m"));
        route(&mut state, NotifyEvent::warn("runtime", "m"));
        route(&mut state, NotifyEvent::critical("mcp", "m"));
        let prefixes: Vec<&str> = state
            .execution
            .activity
            .iter()
            .map(|a| a.message.as_str())
            .collect();
        assert!(prefixes[0].starts_with("[approvals]"));
        assert!(prefixes[1].starts_with("[runtime]"));
        assert!(prefixes[2].starts_with("[mcp]"));
    }

    #[test]
    fn oversized_fields_are_clamped() {
        let mut state = fresh();
        let huge_sub: String = "s".repeat(NOTIFY_SUBSYSTEM_CAP * 4);
        let huge_sum: String = "m".repeat(NOTIFY_SUMMARY_CAP * 4);
        route(&mut state, NotifyEvent::warn(huge_sub, huge_sum));
        let line = &state.execution.activity[0].message;
        // [s…]  + [m…] — both fields truncated; total line length
        // bounded by cap+cap+format overhead.
        assert!(
            line.chars().count() <= NOTIFY_SUBSYSTEM_CAP + NOTIFY_SUMMARY_CAP + 8,
            "activity row too long: {} chars",
            line.chars().count(),
        );
    }

    #[test]
    fn clamp_helper_marks_truncation() {
        let long: String = "x".repeat(100);
        let clamped = clamp_chars(&long, 20);
        assert!(clamped.chars().count() <= 20);
        assert!(clamped.contains('…'));
    }

    #[test]
    fn severity_order_is_increasing() {
        assert!(NotifySeverity::Info < NotifySeverity::Warn);
        assert!(NotifySeverity::Warn < NotifySeverity::NonBlockingCritical);
    }
}

// Derive PartialOrd/Ord on NotifySeverity for the test above +
// future filter helpers. Kept out of the enum definition so
// non_exhaustive doesn't interact with derive order semantics.
impl PartialOrd for NotifySeverity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for NotifySeverity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let rank = |s: &NotifySeverity| match s {
            NotifySeverity::Info => 0,
            NotifySeverity::Warn => 1,
            NotifySeverity::NonBlockingCritical => 2,
        };
        rank(self).cmp(&rank(other))
    }
}
