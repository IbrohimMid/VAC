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
    // Phase A2 — budget facet reads BillingState.total_session.
    Budget,
    // Phase B2 — memory facet reads Consolidator phase state +
    // BM25 staleness.
    Memory,
    // Phase C — policy / rate-limit facets once producers wire.
    Policy,
    RateLimit,
    // Phase E1 — subagent facet reads AppStateRootHandle.
    Subagent,
    // Phase G — LSP passive-feedback tick counter.
    Lsp,
    // Phase A.5 — background tasks (bash, agent, dream, remote,
    // monitor, workflow, teammate). Aggregates over
    // `AppState.execution.task_tray.entries`.
    Tasks,
    // Deferred until producers wire: Vil.
}

impl SystemFacetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approvals => "approvals",
            Self::RuntimeBackoff => "runtime",
            Self::Mcp => "mcp",
            Self::Shell => "shell",
            Self::Speculation => "spec",
            Self::Budget => "budget",
            Self::Memory => "memory",
            Self::Policy => "policy",
            Self::RateLimit => "rate",
            Self::Subagent => "sub",
            Self::Environment => "env",
            Self::Lsp => "lsp",
            Self::Tasks => "tasks",
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
            self.budget_facet(),
            self.memory_facet(),
            self.policy_facet(),
            self.lsp_facet(),
            self.tasks_facet(),
            self.subagent_facet(),
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

    /// Phase A2 — budget facet. Reads the already-tracked session
    /// token total. When `VAC_BUDGET_TOKENS` is set we treat it as
    /// a cap and derive severity from the remaining ratio; otherwise
    /// the facet is observational (Info) and shows raw usage.
    fn budget_facet(&self) -> SystemFacet {
        let used = self
            .state
            .operator_config
            .billing
            .total_session
            .total_tokens;
        let cap = std::env::var("VAC_BUDGET_TOKENS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|n| *n > 0);
        let (severity, token) = match cap {
            Some(cap_n) => {
                let remaining = cap_n.saturating_sub(used);
                let ratio = (remaining as f64) / (cap_n as f64);
                let sev = if ratio < 0.05 {
                    FacetSeverity::Critical
                } else if ratio < 0.20 {
                    FacetSeverity::Warn
                } else if ratio < 0.50 {
                    FacetSeverity::Info
                } else {
                    FacetSeverity::Ok
                };
                (
                    sev,
                    format!("budget:{}/{}", k_fmt(used), k_fmt(cap_n)).into(),
                )
            }
            None => {
                // No cap configured — Info regardless, show raw used.
                let sev = if used > 100_000 {
                    FacetSeverity::Warn
                } else {
                    FacetSeverity::Info
                };
                (sev, format!("budget:{}", k_fmt(used)).into())
            }
        };
        let detail_rows = match cap {
            Some(cap_n) => vec![
                format!("used:       {used}"),
                format!("cap:        {cap_n}"),
                format!("remaining:  {}", cap_n.saturating_sub(used)),
            ],
            None => vec![
                format!("used:  {used}"),
                "cap:   unlimited (set VAC_BUDGET_TOKENS to enforce)".into(),
            ],
        };
        SystemFacet {
            kind: SystemFacetKind::Budget,
            severity,
            compact_token: token,
            detail_rows,
            nav_target: Some(NavTarget::WorkbenchTab(
                crate::app::types::WorkbenchTab::Sessions,
            )),
        }
    }

    /// Phase B2 — memory facet. **No disk I/O on the render path.**
    /// The statusline renders every frame; a `read_dir` per frame
    /// would hammer the filesystem and add visible latency on
    /// rotating disks. Instead we expose *where* the memdir lives
    /// in detail_rows; a follow-up landing wires an idle-tick
    /// producer that caches the entry count on AppState so the
    /// facet can flip to `memory✓N` without syscalls.
    fn memory_facet(&self) -> SystemFacet {
        let archived = self
            .state
            .core
            .project_root
            .join(".vac")
            .join("memory")
            .join("archived");
        SystemFacet {
            kind: SystemFacetKind::Memory,
            severity: FacetSeverity::Info,
            compact_token: "memory·".into(),
            detail_rows: vec![
                format!("archive dir: {}", archived.display()),
                "entry count: cached elsewhere (no fs on render path)".into(),
            ],
            nav_target: Some(NavTarget::WorkbenchTab(
                crate::app::types::WorkbenchTab::Memory,
            )),
        }
    }

    /// Phase C1 — policy facet. Reads
    /// `AppState.execution.policy`, the latest PolicyTracker
    /// snapshot refreshed by an idle tick. Severity is driven by
    /// the higher of the two utilisation ratios (submits / tokens).
    /// `None` snapshot → observational Info with "unconfigured".
    fn policy_facet(&self) -> SystemFacet {
        let snap = &self.state.execution.policy;
        let (severity, token, detail_rows) = match snap {
            None => (
                FacetSeverity::Info,
                Cow::Borrowed("policy·"),
                vec!["tracker: not attached".into()],
            ),
            Some(s) => {
                let submits_ratio = s.policy.max_submits_per_hour.and_then(|cap| {
                    if cap == 0 {
                        None
                    } else {
                        Some((s.submits_last_hour as f64) / (cap as f64))
                    }
                });
                let tokens_ratio = s.policy.max_tokens_per_session.and_then(|cap| {
                    if cap == 0 {
                        None
                    } else {
                        Some((s.tokens_consumed as f64) / (cap as f64))
                    }
                });
                let worst = [submits_ratio, tokens_ratio]
                    .into_iter()
                    .flatten()
                    .fold(0.0f64, f64::max);
                let sev = if submits_ratio.is_none() && tokens_ratio.is_none() {
                    FacetSeverity::Ok
                } else if worst >= 0.85 {
                    FacetSeverity::Critical
                } else if worst >= 0.50 {
                    FacetSeverity::Warn
                } else {
                    FacetSeverity::Ok
                };
                let compact = match (s.policy.max_submits_per_hour, s.policy.max_tokens_per_session) {
                    (Some(cap), _) => {
                        format!("policy:{}/{}", s.submits_last_hour, cap)
                    }
                    (None, Some(cap)) => {
                        format!("policy:{}/{}", k_fmt(s.tokens_consumed), k_fmt(cap))
                    }
                    (None, None) => "policy✓".to_string(),
                };
                let rows = vec![
                    format!(
                        "submits/hr: {} / {}",
                        s.submits_last_hour,
                        s.policy
                            .max_submits_per_hour
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "unlimited".into()),
                    ),
                    format!(
                        "tokens:     {} / {}",
                        s.tokens_consumed,
                        s.policy
                            .max_tokens_per_session
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "unlimited".into()),
                    ),
                    format!("denied tools: {}", s.policy.denied_tools.len()),
                ];
                (sev, Cow::Owned(compact), rows)
            }
        };
        SystemFacet {
            kind: SystemFacetKind::Policy,
            severity,
            compact_token: token,
            detail_rows,
            nav_target: None,
        }
    }

    /// Phase G — LSP passive-feedback facet. Reads the last-tick
    /// counter maintained by `spawn_passive_feedback_loop`. Info
    /// with the recent diagnostics count; Warn when the driver has
    /// not ticked in > 30 s (indicates the loop died).
    fn lsp_facet(&self) -> SystemFacet {
        let snap = &self.state.execution.lsp;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (severity, token, detail_rows) = match snap.last_tick_unix {
            0 => (
                FacetSeverity::Info,
                Cow::Borrowed("lsp·"),
                vec!["driver: not ticked yet".into()],
            ),
            last => {
                let age = now.saturating_sub(last);
                let sev = if age > 30 {
                    FacetSeverity::Warn
                } else {
                    FacetSeverity::Ok
                };
                let tok = format!("lsp✓{}", snap.recent_toasts);
                let rows = vec![
                    format!("last tick:   {age}s ago"),
                    format!("last toasts: {}", snap.recent_toasts),
                    format!("total ticks: {}", snap.total_ticks),
                ];
                (sev, Cow::Owned(tok), rows)
            }
        };
        SystemFacet {
            kind: SystemFacetKind::Lsp,
            severity,
            compact_token: token,
            detail_rows,
            nav_target: None,
        }
    }

    /// A.5 — tasks facet. Aggregates over `task_tray.entries`.
    /// Severity: Ok when 0 in-flight; Info at 1–3; Warn at ≥4;
    /// Critical if any entry is Failed. Compact token `tasks✓N`
    /// or `tasks●N` or `tasks✗N`.
    fn tasks_facet(&self) -> SystemFacet {
        use crate::app::types::TaskStatus;
        let entries = &self.state.execution.task_tray.entries;
        let running = entries
            .iter()
            .filter(|e| matches!(e.status, TaskStatus::Running | TaskStatus::Queued))
            .count();
        let failed = entries
            .iter()
            .filter(|e| matches!(e.status, TaskStatus::Failed))
            .count();
        let (severity, token) = if failed > 0 {
            (
                FacetSeverity::Critical,
                format!("tasks✗{failed}").into(),
            )
        } else if running == 0 {
            (FacetSeverity::Ok, Cow::Borrowed("tasks✓"))
        } else if running >= 4 {
            (FacetSeverity::Warn, format!("tasks●{running}").into())
        } else {
            (FacetSeverity::Info, format!("tasks:{running}").into())
        };
        SystemFacet {
            kind: SystemFacetKind::Tasks,
            severity,
            compact_token: token,
            detail_rows: vec![
                format!("running/queued: {running}"),
                format!("failed:         {failed}"),
                format!("total:          {}", entries.len()),
            ],
            nav_target: Some(NavTarget::Overlay(crate::overlay::OverlayId::TaskTray)),
        }
    }

    /// Phase E1 — subagent facet. Reads the in-memory root handle
    /// counters if the TUI has bound one. Today AppState does not
    /// carry an AppStateRootHandle; we defer real wiring to the
    /// TUI runner. For now we project zeros — the facet exists so
    /// the statusline slot is present and the grammar doesn't drift
    /// when the producer lands.
    fn subagent_facet(&self) -> SystemFacet {
        // Placeholder projection until AppStateRootHandle is threaded
        // into AppState. Keeping a live facet slot with Ok severity
        // reserves the statusline column and the detail rows so
        // the operator-panel layout doesn't reflow when the real
        // producer wires in.
        SystemFacet {
            kind: SystemFacetKind::Subagent,
            severity: FacetSeverity::Ok,
            compact_token: "sub·".into(),
            detail_rows: vec![
                "producers: none wired".into(),
                "RootHandle binding: pending".into(),
            ],
            nav_target: Some(NavTarget::WorkbenchTab(
                crate::app::types::WorkbenchTab::Agents,
            )),
        }
    }
}

fn k_fmt(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", (n as f64) / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", (n as f64) / 1_000.0)
    } else {
        n.to_string()
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
        // Phase A2 + B2 + E1 added three more facets (budget,
        // memory, subagent). Count is 9 now; the grammar stays
        // consistent.
        assert_eq!(facets.len(), 12);
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
        assert!(line.contains("budget"));
        assert!(line.contains("memory"));
        assert!(line.contains("sub"));
        assert!(line.contains("policy"));
        assert!(line.contains("lsp"));
        assert!(line.contains("tasks"));
        // Twelve facets → exactly eleven internal spaces.
        assert_eq!(line.matches(' ').count(), 11);
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
