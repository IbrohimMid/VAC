//! A1 — `TuiTracingLayer`: single source that converts
//! `tracing::warn!` / `error!` on allow-listed subsystem targets
//! into `NotifyRouter` events.
//!
//! Before this bridge, subsystem denials in `vac_tools::trust_gate`,
//! `vac_runtime::isolation`, `vac_mcp_core::channel`, etc. emitted
//! `tracing::warn!` that operators could only see via
//! `RUST_LOG=info`. Now they route through the unified grammar —
//! activity entry for Warn, activity + banner for Error/Critical —
//! the same pipeline the TUI already uses for every other
//! user-visible alert.
//!
//! Installed once from `vac_cli::commands::interactive` startup.
//! Feature-flagged via `VAC_TRACING_BRIDGE=0` to disable (useful
//! in CI where we want only the structured test assertions).

use std::sync::{Arc, Mutex};

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use crate::app::AppState;
use crate::services::notify_router::{route, NotifyEvent};

/// Subsystem target → label mapping. Each entry turns a
/// `tracing::warn!(target = "vac_tools::trust_gate", …)` into a
/// `NotifyEvent` with the matching subsystem label so operators
/// see the same vocabulary everywhere.
///
/// Keep this list aligned with `SystemFacetKind::label` where a
/// matching facet exists. New subsystems that emit user-worthy
/// warn-level events add themselves here.
pub const BRIDGE_ALLOWLIST: &[(&str, &str)] = &[
    ("vac_tools::trust_gate", "trust"),
    ("vac_runtime::isolation", "env"),
    ("vac_mcp_core::channel", "mcp"),
    ("vac_tools::result_spill", "spill"),
    ("vac_core::policy_limits", "policy"),
    ("vil_llm::rate_limit", "rate"),
    ("vac_tui_runtime::auto_dream", "memory"),
    ("vac_tui_runtime::away_summary", "resume"),
    ("vac_tui_runtime::speculation", "spec"),
    ("vac_tui_runtime::passive_feedback", "lsp"),
    ("vac_session_engine::auto_compact", "compact"),
    ("vac_session_engine::gate", "gate"),
    ("vac_skill::md_registry", "skills"),
];

/// Shared handle the TUI event loop holds to the `AppState` so the
/// subscriber callback can push notifications. Interior mutability
/// via `Arc<Mutex<…>>` — the bridge is only active inside the
/// interactive session which owns the lone runtime.
pub type SharedState = Arc<Mutex<AppState>>;

/// Environment variable that disables the bridge at startup. Any
/// value other than `1`/`true` keeps it disabled; default (unset)
/// is **enabled**.
pub const DISABLE_ENV: &str = "VAC_TRACING_BRIDGE";

/// Return true when the bridge should be installed.
pub fn is_enabled() -> bool {
    match std::env::var(DISABLE_ENV) {
        Ok(v) => !matches!(v.as_str(), "0" | "false" | "off"),
        Err(_) => true,
    }
}

/// Look up the subsystem label for a given event target. Target
/// names are module-path prefixes — we match on `starts_with` so
/// nested children (`vac_tools::trust_gate::some_fn`) route
/// correctly.
pub fn subsystem_for_target(target: &str) -> Option<&'static str> {
    for (prefix, label) in BRIDGE_ALLOWLIST {
        if target == *prefix || target.starts_with(&format!("{prefix}::")) {
            return Some(*label);
        }
    }
    None
}

/// Tracing → NotifyRouter bridge.
pub struct TuiTracingLayer {
    state: SharedState,
}

impl TuiTracingLayer {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for TuiTracingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let level = *meta.level();
        if level != Level::WARN && level != Level::ERROR && level != Level::INFO {
            return;
        }
        let Some(subsystem) = subsystem_for_target(meta.target()) else {
            return;
        };
        // Extract a summary from the event fields. We grab the
        // `message` implicit field + any `reason` field. Visitor
        // collects both without allocating until needed.
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let summary = visitor.message.unwrap_or_else(|| {
            visitor.reason.clone().unwrap_or_else(|| meta.name().to_string())
        });
        let mut state = match self.state.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let event = match level {
            Level::ERROR => {
                let e = NotifyEvent::critical(subsystem, summary);
                match visitor.reason {
                    Some(r) => e.with_detail(r),
                    None => e,
                }
            }
            Level::WARN => {
                let e = NotifyEvent::warn(subsystem, summary);
                match visitor.reason {
                    Some(r) => e.with_detail(r),
                    None => e,
                }
            }
            _ => {
                // INFO — activity-only (no toast / banner pressure).
                // Used by subsystems that want a breadcrumb
                // (e.g. speculation warmed N files, auto_dream wrote).
                let e = NotifyEvent::info(subsystem, summary);
                match visitor.reason {
                    Some(r) => e.with_detail(r),
                    None => e,
                }
            }
        };
        route(&mut state, event);
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
    reason: Option<String>,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "message" => self.message = Some(value.to_string()),
            "reason" => self.reason = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn std::fmt::Debug,
    ) {
        if field.name() == "message" && self.message.is_none() {
            self.message = Some(format!("{value:?}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    fn fresh_state() -> SharedState {
        Arc::new(Mutex::new(AppState::default()))
    }

    #[test]
    fn subsystem_for_target_maps_known_prefixes() {
        assert_eq!(
            subsystem_for_target("vac_tools::trust_gate"),
            Some("trust"),
        );
        assert_eq!(
            subsystem_for_target("vac_tools::trust_gate::inner"),
            Some("trust"),
        );
        assert_eq!(
            subsystem_for_target("vac_runtime::isolation"),
            Some("env"),
        );
        assert_eq!(subsystem_for_target("unrelated::module"), None);
    }

    #[test]
    fn is_enabled_defaults_on_when_unset() {
        // SAFETY: test-local env mutation.
        unsafe { std::env::remove_var(DISABLE_ENV) };
        assert!(is_enabled());
    }

    #[test]
    fn is_enabled_respects_disable_values() {
        for v in ["0", "false", "off"] {
            unsafe { std::env::set_var(DISABLE_ENV, v) };
            assert!(!is_enabled(), "VAC_TRACING_BRIDGE={v} should disable");
        }
        unsafe { std::env::remove_var(DISABLE_ENV) };
    }

    #[test]
    fn warn_on_allowlisted_target_routes_to_notify_router() {
        let state = fresh_state();
        let layer = TuiTracingLayer::new(state.clone());
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(
                target: "vac_tools::trust_gate",
                "isolation mode denied remote tool",
            );
        });
        let guard = state.lock().unwrap();
        assert_eq!(guard.execution.activity.len(), 1);
        assert!(
            guard.execution.activity[0]
                .message
                .starts_with("[trust]")
        );
        // Warn-level → toast present, banner absent.
        assert_eq!(guard.layout.toasts.len(), 1);
        assert!(guard.layout.banner.message.is_none());
    }

    #[test]
    fn error_on_allowlisted_target_routes_to_banner() {
        let state = fresh_state();
        let layer = TuiTracingLayer::new(state.clone());
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(
                target: "vac_runtime::isolation",
                "spawn denied",
            );
        });
        let guard = state.lock().unwrap();
        assert_eq!(guard.execution.activity.len(), 1);
        // Critical → banner set, toast absent.
        assert!(guard.layout.banner.message.is_some());
        assert!(guard.layout.toasts.is_empty());
    }

    #[test]
    fn info_on_allowlisted_target_routes_as_breadcrumb() {
        // D2 extended the bridge to forward INFO as activity-only
        // breadcrumbs (no toast / banner pressure). Used by
        // speculation warmed-reads, auto_dream writes, etc.
        let state = fresh_state();
        let layer = TuiTracingLayer::new(state.clone());
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                target: "vac_tui_runtime::speculation",
                "warmed 2 file(s): a.rs, b.rs",
            );
        });
        let guard = state.lock().unwrap();
        assert_eq!(guard.execution.activity.len(), 1);
        assert!(guard.execution.activity[0].message.starts_with("[spec]"));
        assert!(guard.layout.toasts.is_empty());
        assert!(guard.layout.banner.message.is_none());
    }

    #[test]
    fn warn_on_non_allowlisted_target_is_ignored() {
        let state = fresh_state();
        let layer = TuiTracingLayer::new(state.clone());
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(
                target: "some::other::lib",
                "unrelated warning",
            );
        });
        let guard = state.lock().unwrap();
        assert!(guard.execution.activity.is_empty());
    }
}
