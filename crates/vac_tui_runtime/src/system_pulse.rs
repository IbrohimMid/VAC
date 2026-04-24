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
    // U7 will add: Lsp, Vil, Speculation, Subagent, Environment,
    // Memory, Policy, RateLimit, Shell.
}

impl SystemFacetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approvals => "approvals",
            Self::RuntimeBackoff => "runtime",
            Self::Mcp => "mcp",
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
        assert_eq!(facets.len(), 3);
        for f in &facets {
            match f.kind {
                SystemFacetKind::Approvals | SystemFacetKind::RuntimeBackoff => {
                    assert_eq!(f.severity, FacetSeverity::Ok);
                }
                // Default state has no MCP servers configured.
                SystemFacetKind::Mcp => {
                    assert_eq!(f.severity, FacetSeverity::Info);
                }
            }
        }
    }

    #[test]
    fn compact_line_joins_facets_with_space() {
        let state = AppState::default();
        let pulse = SystemPulse::from_state(&state);
        let line = pulse.compact_line();
        // "approvals✓ runtime✓ mcp·" — one space between each
        assert!(line.contains("approvals✓"));
        assert!(line.contains("runtime✓"));
        assert!(line.contains("mcp·"));
        // Exactly two internal spaces between three facets.
        assert_eq!(line.matches(' ').count(), 2);
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
    fn every_facet_has_a_nav_target() {
        let state = AppState::default();
        let pulse = SystemPulse::from_state(&state);
        for f in pulse.facets() {
            assert!(
                f.nav_target.is_some(),
                "facet {:?} must have a nav_target in v0",
                f.kind
            );
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
