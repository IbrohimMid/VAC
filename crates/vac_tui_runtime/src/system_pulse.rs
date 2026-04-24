//! U1 — `SystemPulse`: borrow-only projection of current state.
//!
//! Read-only view over `AppState` that derives a consistent
//! per-subsystem snapshot. Every view that wants to say something
//! about "what is the system doing right now?" reads from this
//! one projection — statusline, operator panel, activity panel,
//! future overlays. No storage, no cloning.
//!
//! U1 ships three facets (approvals / runtime / MCP) — the
//! producers that already have real data on `main`. U7 adds the
//! remaining nine.

use std::borrow::Cow;

use crate::app::AppState;

/// Severity ordering matches the NotifyRouter lanes from L4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FacetSeverity {
    Ok,
    Info,
    Warn,
    Critical,
}

impl FacetSeverity {
    /// Single glyph the statusline + operator panel + activity
    /// panel all use. Kept here so the grammar is truly shared.
    pub fn glyph(self) -> char {
        match self {
            Self::Ok => '✓',
            Self::Info => '·',
            Self::Warn => '●',
            Self::Critical => '✗',
        }
    }

    /// Short color name. Concrete `ratatui` style mapping lives in
    /// the renderer; we keep the pulse UI-framework-free so unit
    /// tests don't need to touch ratatui.
    pub fn color_name(self) -> &'static str {
        match self {
            Self::Ok => "green",
            Self::Info => "dim",
            Self::Warn => "yellow",
            Self::Critical => "red",
        }
    }
}

/// Where an operator lands on Enter / click for this facet.
/// Always points at an existing surface — a workbench tab or a
/// registered overlay — never at a drawer that doesn't exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    WorkbenchTab(crate::app::types::WorkbenchTab),
    Overlay(crate::overlay::OverlayId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SystemFacetKind {
    Approvals,
    RuntimeBackoff,
    Mcp,
    // U7 — three more facets whose data already lives on AppState.
    Shell,
    Speculation,
    Environment,
    // Deferred until producers wire: Lsp, Vil, Subagent, Memory,
    // Policy, RateLimit.
}

impl SystemFacetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approvals => "approvals",
            Self::RuntimeBackoff => "runtime",
            Self::Mcp => "mcp",
            Self::Shell => "shell",
            Self::Speculation => "spec",
            Self::Environment => "env",
        }
    }
}

/// What a subsystem contributes to the pulse.
#[derive(Debug, Clone)]
pub struct SystemFacet {
    pub kind: SystemFacetKind,
    pub severity: FacetSeverity,
    /// Tiny token for the statusline. Target: ≤ 12 chars so a row
    /// of 9 facets fits in 80 cols with margin.
    pub compact_token: Cow<'static, str>,
    /// Expanded rows for the operator panel or an overlay. Each
    /// string is one line; the renderer wraps. Keep ASCII + one
    /// glyph — no colours here, renderer decides.
    pub detail_rows: Vec<String>,
    /// Where Enter takes the operator. `None` means the facet is
    /// observational only.
    pub nav_target: Option<NavTarget>,
}

/// Borrow-only pulse. Cannot be stored (no `Clone`, no `'static`).
/// Construct fresh per render.
pub struct SystemPulse<'a> {
    state: &'a AppState,
}

impl NavTarget {
    /// Apply this nav target to the mutable AppState. Returns true
    /// when navigation actually moved (either a tab focus change or
    /// an overlay opened). Caller can emit a notify event on true.
    pub fn apply(&self, state: &mut AppState) -> bool {
        match self {
            NavTarget::WorkbenchTab(tab) => {
                if state.layout.workbench_tab == *tab {
                    return false;
                }
                state.layout.workbench_tab = *tab;
                true
            }
            NavTarget::Overlay(id) => {
                crate::overlay::open_overlay(state, *id);
                true
            }
        }
    }
}

impl<'a> SystemPulse<'a> {
    pub fn from_state(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Deterministic list of facets. Ordering is by
    /// `SystemFacetKind` discriminant so the statusline never
    /// reorders between frames.
    pub fn facets(&self) -> Vec<SystemFacet> {
        vec![
            self.approvals_facet(),
            self.runtime_facet(),
            self.mcp_facet(),
            self.shell_facet(),
            self.speculation_facet(),
            self.environment_facet(),
        ]
    }

    /// One-line compact rendering. Used by the statusline (U2).
    /// Format: `approvals✓ runtime✓ mcp:2/3`.
    pub fn compact_line(&self) -> String {
        self.facets()
            .iter()
            .map(|f| f.compact_token.as_ref().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    // ── per-facet derivations ──────────────────────────────────────

    fn approvals_facet(&self) -> SystemFacet {
        let a = &self.state.execution.approvals;
        let pending = a.pending_approvals.len();
        let approved = a.approved_tools.len();
        let rejected = a.rejected_tools.len();
        let (severity, token) = if pending == 0 {
            (FacetSeverity::Ok, "approvals✓".into())
        } else if pending >= 3 {
            (
                FacetSeverity::Warn,
                format!("approvals●{pending}").into(),
            )
        } else {
            (FacetSeverity::Info, format!("approvals:{pending}").into())
        };
        let detail_rows = vec![
            format!("pending:  {pending}"),
            format!("approved: {approved}"),
            format!("rejected: {rejected}"),
        ];
        SystemFacet {
            kind: SystemFacetKind::Approvals,
            severity,
            compact_token: token,
            detail_rows,
            nav_target: Some(NavTarget::WorkbenchTab(
                crate::app::types::WorkbenchTab::Approvals,
            )),
        }
    }

    fn runtime_facet(&self) -> SystemFacet {
        let r = &self.state.execution.runtime;
        let total = r.jobs.len();
        let running = r
            .jobs
            .iter()
            .filter(|j| {
                matches!(
                    j.status,
                    vac_runtime::JobStatus::Running
                        | vac_runtime::JobStatus::Queued
                )
            })
            .count();
        let failed = r
            .jobs
            .iter()
            .filter(|j| matches!(j.status, vac_runtime::JobStatus::Failed(_)))
            .count();
        let (severity, token) = if failed > 0 {
            (
                FacetSeverity::Critical,
                format!("runtime✗{failed}").into(),
            )
        } else if running > 0 {
            (
                FacetSeverity::Info,
                format!("runtime:{running}").into(),
            )
        } else {
            (FacetSeverity::Ok, "runtime✓".into())
        };
        SystemFacet {
            kind: SystemFacetKind::RuntimeBackoff,
            severity,
            compact_token: token,
            detail_rows: vec![
                format!("running: {running}"),
                format!("failed:  {failed}"),
                format!("total:   {total}"),
            ],
            nav_target: Some(NavTarget::WorkbenchTab(
                crate::app::types::WorkbenchTab::Runtime,
            )),
        }
    }

    fn mcp_facet(&self) -> SystemFacet {
        let m = &self.state.execution.mcp_maps;
        let total = m.server_states.len();
        let connected = m
            .server_states
            .values()
            .filter(|c| {
                matches!(
                    c.state,
                    vac_mcp_core::state::McpConnectionState::Connected
                )
            })
            .count();
        let failed = m
            .server_states
            .values()
            .filter(|c| {
                matches!(
                    c.state,
                    vac_mcp_core::state::McpConnectionState::Failed
                )
            })
            .count();
        let (severity, token) = if total == 0 {
            (FacetSeverity::Info, "mcp·".into())
        } else if failed > 0 {
            (
                FacetSeverity::Critical,
                format!("mcp✗{failed}/{total}").into(),
            )
        } else if connected == total {
            (
                FacetSeverity::Ok,
                format!("mcp✓{total}").into(),
            )
        } else {
            (
                FacetSeverity::Warn,
                format!("mcp●{connected}/{total}").into(),
            )
        };
        SystemFacet {
            kind: SystemFacetKind::Mcp,
            severity,
            compact_token: token,
            detail_rows: vec![
                format!("connected: {connected}"),
                format!("failed:    {failed}"),
                format!("total:     {total}"),
            ],
            nav_target: Some(NavTarget::WorkbenchTab(
                crate::app::types::WorkbenchTab::Signal,
            )),
        }
    }

    fn shell_facet(&self) -> SystemFacet {
        let s = &self.state.execution.shell.session_store;
        let total = s.sessions.len();
        let active_label = s.active().map(|ss| ss.label.clone());
        let (severity, token) = if total == 0 {
            (FacetSeverity::Ok, "shell✓".into())
        } else if active_label.is_some() {
            (FacetSeverity::Info, format!("shell:{total}").into())
        } else {
            // Sessions exist but none active → backgrounded.
            (FacetSeverity::Info, format!("shell·{total}").into())
        };
        SystemFacet {
            kind: SystemFacetKind::Shell,
            severity,
            compact_token: token,
            detail_rows: vec![
                format!("sessions: {total}"),
                format!(
                    "active:   {}",
                    active_label.unwrap_or_else(|| "<none>".into())
                ),
            ],
            // No dedicated workbench tab; surface via shell popup.
            nav_target: Some(NavTarget::Overlay(
                crate::overlay::OverlayId::ShellPopup,
            )),
        }
    }

    fn speculation_facet(&self) -> SystemFacet {
        // SpeculationCache carries { predicted_submit, precomputed_context,
        // prediction_hits }. We project "is there a warm prediction?".
        let c = &self.state.speculation;
        let (severity, token) = if c.predicted_submit.is_some() {
            (FacetSeverity::Info, "spec●".into())
        } else {
            (FacetSeverity::Ok, "spec·".into())
        };
        SystemFacet {
            kind: SystemFacetKind::Speculation,
            severity,
            compact_token: token,
            detail_rows: vec![
                format!(
                    "predicted: {}",
                    if c.predicted_submit.is_some() { "yes" } else { "no" }
                ),
                format!("hits:      {}", c.prediction_hits),
                format!("context:   {} entries", c.precomputed_context.len()),
            ],
            // No dedicated surface; pulse is observational for now.
            nav_target: None,
        }
    }

    fn environment_facet(&self) -> SystemFacet {
        // StartupSnapshot.environment is the human label vac_doctor
        // sets ("host" / "isolated-batch" / etc.). Used as-is for
        // the compact token.
        let env = &self.state.core.startup;
        let mode = env.environment.as_str();
        let (severity, token) = match mode {
            "restricted-offline" => (
                FacetSeverity::Warn,
                "env:restricted".into(),
            ),
            m if m.starts_with("isolated") => {
                (FacetSeverity::Info, format!("env:{m}").into())
            }
            "" => (FacetSeverity::Info, "env:?".into()),
            m => (FacetSeverity::Ok, format!("env:{m}").into()),
        };
        SystemFacet {
            kind: SystemFacetKind::Environment,
            severity,
            compact_token: token,
            detail_rows: vec![
                format!(
                    "mode: {}",
                    if mode.is_empty() { "<unknown>" } else { mode }
                ),
                format!(
                    "profile: {}",
                    env.active_profile.as_deref().unwrap_or("<none>"),
                ),
            ],
            nav_target: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;

    #[test]
    fn empty_state_yields_ok_facets() {
        let state = AppState::default();
        let pulse = SystemPulse::from_state(&state);
        let facets = pulse.facets();
        assert_eq!(facets.len(), 6);
        // Sanity: every facet emits a non-empty compact token.
        for f in &facets {
            assert!(!f.compact_token.is_empty(), "facet {:?} has empty token", f.kind);
        }
    }

    #[test]
    fn compact_line_joins_facets_with_space() {
        let state = AppState::default();
        let pulse = SystemPulse::from_state(&state);
        let line = pulse.compact_line();
        assert!(line.contains("approvals✓"));
        assert!(line.contains("runtime✓"));
        assert!(line.contains("mcp·"));
        assert!(line.contains("shell✓"));
        assert!(line.contains("spec·"));
        assert!(line.contains("env:"));
        // Six facets → exactly five internal spaces.
        assert_eq!(line.matches(' ').count(), 5);
    }

    #[test]
    fn facets_are_deterministic_between_calls() {
        let state = AppState::default();
        let pulse = SystemPulse::from_state(&state);
        let a = pulse.compact_line();
        let b = pulse.compact_line();
        assert_eq!(a, b);
    }

    #[test]
    fn approvals_facet_warns_on_three_pending() {
        let mut state = AppState::default();
        for i in 0..3 {
            state
                .execution
                .approvals
                .pending_approvals
                .push(mock_tool_call(&format!("t{i}")));
        }
        let pulse = SystemPulse::from_state(&state);
        let facets = pulse.facets();
        let approvals = facets
            .iter()
            .find(|f| f.kind == SystemFacetKind::Approvals)
            .unwrap();
        assert_eq!(approvals.severity, FacetSeverity::Warn);
        assert!(approvals.compact_token.contains("approvals●"));
    }

    #[test]
    fn approvals_facet_info_on_one_or_two_pending() {
        let mut state = AppState::default();
        state
            .execution
            .approvals
            .pending_approvals
            .push(mock_tool_call("t1"));
        let pulse = SystemPulse::from_state(&state);
        let approvals = pulse
            .facets()
            .into_iter()
            .find(|f| f.kind == SystemFacetKind::Approvals)
            .unwrap();
        assert_eq!(approvals.severity, FacetSeverity::Info);
    }

    #[test]
    fn facet_glyph_per_severity_is_stable() {
        assert_eq!(FacetSeverity::Ok.glyph(), '✓');
        assert_eq!(FacetSeverity::Info.glyph(), '·');
        assert_eq!(FacetSeverity::Warn.glyph(), '●');
        assert_eq!(FacetSeverity::Critical.glyph(), '✗');
    }

    #[test]
    fn nav_target_apply_switches_workbench_tab() {
        let mut state = AppState::default();
        state.layout.workbench_tab = crate::app::types::WorkbenchTab::Sessions;
        let tgt = NavTarget::WorkbenchTab(
            crate::app::types::WorkbenchTab::Approvals,
        );
        let moved = tgt.apply(&mut state);
        assert!(moved);
        assert_eq!(
            state.layout.workbench_tab,
            crate::app::types::WorkbenchTab::Approvals
        );
        // Idempotent: second apply is a no-op.
        let moved_again = tgt.apply(&mut state);
        assert!(!moved_again);
    }

    #[test]
    fn nav_target_apply_opens_overlay() {
        let mut state = AppState::default();
        let tgt = NavTarget::Overlay(crate::overlay::OverlayId::CommandPalette);
        let moved = tgt.apply(&mut state);
        assert!(moved);
        assert!(
            state
                .layout
                .overlay_manager
                .is_active(crate::overlay::OverlayId::CommandPalette)
        );
    }

    #[test]
    fn facets_that_point_at_a_tab_or_overlay_resolve() {
        // Not every facet has a NavTarget — speculation and
        // environment are observational (None). The ones that DO
        // have one must be apply()-able without panicking.
        let state = AppState::default();
        let pulse = SystemPulse::from_state(&state);
        for f in pulse.facets() {
            if let Some(tgt) = f.nav_target.clone() {
                let mut fresh = AppState::default();
                let _ = tgt.apply(&mut fresh);
            }
        }
    }

    fn mock_tool_call(id: &str) -> crate::types::ToolCall {
        crate::types::ToolCall {
            id: id.into(),
            r#type: "function".into(),
            function: crate::types::FunctionCall {
                name: "test".into(),
                arguments: "{}".into(),
            },
            metadata: None,
        }
    }
}
