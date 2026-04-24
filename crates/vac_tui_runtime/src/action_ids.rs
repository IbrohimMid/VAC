//! Action types, context, and query helpers.
//!
//! Extracted from `action_registry.rs` to keep that file under 600 lines.

use crate::app::{WorkbenchTab, WorkspaceFocus};

// ── ActionId ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionId {
    // Global
    Quit,
    OpenCommandPalette,
    OpenShortcuts,
    OpenFileSearch,
    SwitchModel,
    SwitchProfile,
    SwitchIsolation,
    SwitchRulebook,
    CyclePane,
    CycleWorkbenchTab,
    ToggleSidePanel,
    ToggleAutoApprove,
    // Input focus
    Submit,
    CancelStream,
    // Workbench — Approvals
    ApproveCurrent,
    ApproveAll,
    RejectCurrent,
    RejectAll,
    // Workbench — Review
    ToggleDiff,
    RevertSelected,
    RevertFiltered,
    RevertAll,
    OpenEditor,
    // Workbench — Sessions
    ResumeCheckpoint,
    CleanSession,
    SwitchToSession,
    // Workbench — Runtime
    RefreshRuntime,
    CancelRuntimeJob,
    RetryRuntimeJob,
    // Workbench — Agents
    RefreshAgents,
    // Workbench — Plan
    OpenPlan,
    ApprovePlan,
    RequestPlanChanges,
    EditPlan,
    OpenPlanReview,
    // Workbench — VIL
    RunRepair,
    RunAudit,
    RunIrDiff,
    OpenVilEditor,
    RunBatchCampaign,
    // Overlay — generic
    CloseOverlay,
    // New session / review
    NewSession,
    ReviewOpen,
    // Slash-command actions (BuiltIn handlers)
    Clear,
    Sessions,
    Runtime,
    Agents,
    Vwfd,
    Shell,
    ShellFocus,
    ShellBackground,
    ShellKill,
    Context,
    Export,
    Import,
    Changes,
    FileChanges,
    PlanReview,
    PlanEdit,
    // Wave 2 overlays
    OpenTaskTray,
    OpenThemePicker,
    OpenSessionResume,
    OpenFilePicker,
    // Phase B1 — bundled skills exposed as palette entries so the
    // operator discovers them via `Ctrl+P` instead of reading
    // `vac skills list`. Dispatched through `SkillTool`.
    SkillBatch,
    SkillLoop,
    SkillRemember,
    SkillSimplify,
    SkillStuck,
    SkillVerify,
    // Phase F1 — CLI command bridge. Each entry spawns
    // `vac <name>` in a shell popup.
    SpawnCliAdvisor,
    SpawnCliAutofixPr,
    SpawnCliBughunter,
    SpawnCliSecurityReview,
    SpawnCliPerfIssue,
    SpawnCliTeleport,
    SpawnCliThinkback,
    SpawnCliUltraplan,
    SpawnCliRewind,
    SpawnCliDecisions,
    /// G2 — launch `vac auth login` interactively via the shell popup.
    SpawnCliAuthLogin,
    /// QW.2 — open the /context overlay.
    OpenContextInspector,
    /// D.6 — scrub to previous Accepted row in the transcript.
    Thinkback,
    /// D.6 — replay tool calls from a saved point forward.
    ThinkbackPlay,
}

// ── KeyChord ────────────────────────────────────────────────────────────────

pub type KeyChord = &'static str;

// ── ActionSpec ──────────────────────────────────────────────────────────────

pub struct ActionSpec {
    pub id: ActionId,
    pub title: &'static str,
    pub description: &'static str,
    pub scope: ActionContext,
    pub keybindings: &'static [KeyChord],
    pub slash_aliases: &'static [&'static str],
    pub palette_visible: bool,
    pub footer_visible: bool,
    pub availability: fn(&crate::app::AppState) -> bool,
    pub activity_message: Option<fn(&crate::app::AppState) -> String>,
}

pub(crate) fn always_available(_: &crate::app::AppState) -> bool {
    true
}

// ── ActionContext ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionContext {
    Global,
    InputFocus,
    ConversationFocus,
    ActivityFocus,
    WorkbenchApprovals,
    WorkbenchReview,
    WorkbenchSessions,
    WorkbenchAgents,
    WorkbenchRuntime,
    WorkbenchPlan,
    WorkbenchVil,
    WorkbenchVwfd,
    /// Matches any active workbench tab — use for shared tab-level hints.
    WorkbenchAny,
    OverlayActive,
}

impl ActionContext {
    pub fn from_app_state(state: &crate::app::AppState) -> Self {
        if state.layout.overlay_manager.any_active() {
            return ActionContext::OverlayActive;
        }

        match state.layout.focus {
            WorkspaceFocus::Input => ActionContext::InputFocus,
            WorkspaceFocus::Conversation => ActionContext::ConversationFocus,
            WorkspaceFocus::Activity => ActionContext::ActivityFocus,
            WorkspaceFocus::Workbench => match state.layout.workbench_tab {
                WorkbenchTab::Approvals => ActionContext::WorkbenchApprovals,
                WorkbenchTab::Review => ActionContext::WorkbenchReview,
                WorkbenchTab::Sessions => ActionContext::WorkbenchSessions,
                WorkbenchTab::Agents => ActionContext::WorkbenchAgents,
                WorkbenchTab::Runtime => ActionContext::WorkbenchRuntime,
                WorkbenchTab::Plan => ActionContext::WorkbenchPlan,
                WorkbenchTab::Vil => ActionContext::WorkbenchVil,
                WorkbenchTab::Vwfd => ActionContext::WorkbenchVwfd,
                // Signal tab reuses the generic Workbench action context
                // until dedicated bindings are required.
                WorkbenchTab::Signal => ActionContext::WorkbenchRuntime,
                WorkbenchTab::Memory => ActionContext::WorkbenchAny,
            },
        }
    }
}

// ── Query helpers ────────────────────────────────────────────────────────────

use crate::action_registry::ACTION_SPECS;

/// Look up specs for a given scope (includes Global specs for non-overlay contexts).
/// Specs scoped to `WorkbenchAny` appear for all workbench contexts.
pub fn specs_for_context(ctx: ActionContext) -> impl Iterator<Item = &'static ActionSpec> {
    let is_workbench = matches!(
        ctx,
        ActionContext::WorkbenchApprovals
            | ActionContext::WorkbenchReview
            | ActionContext::WorkbenchSessions
            | ActionContext::WorkbenchAgents
            | ActionContext::WorkbenchRuntime
            | ActionContext::WorkbenchPlan
            | ActionContext::WorkbenchVil
            | ActionContext::WorkbenchVwfd
    );
    ACTION_SPECS.iter().filter(move |s| {
        s.scope == ctx
            || (ctx != ActionContext::OverlayActive && s.scope == ActionContext::Global)
            || (is_workbench && s.scope == ActionContext::WorkbenchAny)
    })
}

/// Look up a spec by slash alias.
pub fn spec_by_slash_alias(alias: &str) -> Option<&'static ActionSpec> {
    ACTION_SPECS
        .iter()
        .find(|s| s.slash_aliases.contains(&alias))
}

/// All specs visible in the footer for a given context.
pub fn footer_specs(ctx: ActionContext) -> impl Iterator<Item = &'static ActionSpec> {
    specs_for_context(ctx).filter(|s| s.footer_visible)
}
