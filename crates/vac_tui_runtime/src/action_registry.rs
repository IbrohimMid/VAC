//! Action Registry
//!
//! Single source of truth for all TUI actions: keybindings, palette entries,
//! footer hints, slash aliases, and availability guards.

use crate::app::{WorkbenchTab, WorkspaceFocus};

// ── ActionId ────────────────────────────────────────────────────────────────

/// Closed enum of every user-invocable intent. No string IDs.
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
}

// ── KeyChord ────────────────────────────────────────────────────────────────

/// A human-readable key description for display in footer / shortcuts popup.
pub type KeyChord = &'static str;

// ── ActionSpec ──────────────────────────────────────────────────────────────

/// Full specification of a user-invocable action.
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

fn always_available(_: &crate::app::AppState) -> bool { true }

/// Static registry of all action specs. This is the single source of truth.
pub static ACTION_SPECS: &[ActionSpec] = &[
    ActionSpec {
        id: ActionId::Quit,
        title: "Quit",
        description: "Quit the application",
        scope: ActionContext::Global,
        keybindings: &["Ctrl+C×2"],
        slash_aliases: &["/quit"],
        palette_visible: true,
        footer_visible: false,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::OpenCommandPalette,
        title: "Command Palette",
        description: "Open the command palette",
        scope: ActionContext::Global,
        keybindings: &["Ctrl+P"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::OpenShortcuts,
        title: "Shortcuts",
        description: "Show keyboard shortcuts",
        scope: ActionContext::Global,
        keybindings: &["?"],
        slash_aliases: &["/help"],
        palette_visible: true,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::OpenFileSearch,
        title: "File Search",
        description: "Search and insert a file path",
        scope: ActionContext::Global,
        keybindings: &["Ctrl+F"],
        slash_aliases: &["/files"],
        palette_visible: true,
        footer_visible: false,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::CancelStream,
        title: "Cancel",
        description: "Cancel the current streaming response",
        scope: ActionContext::Global,
        keybindings: &["Ctrl+C"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: |s| s.is_streaming,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::SwitchModel,
        title: "Switch Model",
        description: "Switch the active language model",
        scope: ActionContext::Global,
        keybindings: &[],
        slash_aliases: &["/model"],
        palette_visible: true,
        footer_visible: false,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::SwitchProfile,
        title: "Switch Profile",
        description: "Switch configuration profile",
        scope: ActionContext::Global,
        keybindings: &[],
        slash_aliases: &["/profile"],
        palette_visible: true,
        footer_visible: false,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::SwitchIsolation,
        title: "Switch Isolation",
        description: "Change execution isolation mode",
        scope: ActionContext::Global,
        keybindings: &[],
        slash_aliases: &["/isolation"],
        palette_visible: true,
        footer_visible: false,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::SwitchRulebook,
        title: "Switch Rulebook",
        description: "Change active rulebooks",
        scope: ActionContext::Global,
        keybindings: &[],
        slash_aliases: &["/rulebook"],
        palette_visible: true,
        footer_visible: false,
        availability: always_available,
        activity_message: None,
    },
    // Workbench — Approvals
    ActionSpec {
        id: ActionId::ApproveCurrent,
        title: "Approve",
        description: "Approve the selected tool call",
        scope: ActionContext::WorkbenchApprovals,
        keybindings: &["a", "Enter"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: |s| !s.pending_approvals.is_empty(),
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::ApproveAll,
        title: "Approve All",
        description: "Approve all pending tool calls",
        scope: ActionContext::WorkbenchApprovals,
        keybindings: &["A"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: |s| !s.pending_approvals.is_empty(),
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::RejectCurrent,
        title: "Reject",
        description: "Reject the selected tool call",
        scope: ActionContext::WorkbenchApprovals,
        keybindings: &["x"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: |s| !s.pending_approvals.is_empty(),
        activity_message: None,
    },
    // Workbench — Review
    ActionSpec {
        id: ActionId::ToggleDiff,
        title: "Diff",
        description: "Toggle diff view for selected file",
        scope: ActionContext::WorkbenchReview,
        keybindings: &["Enter"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::RevertSelected,
        title: "Revert",
        description: "Revert selected file to snapshot",
        scope: ActionContext::WorkbenchReview,
        keybindings: &["Ctrl+X"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::RevertAll,
        title: "Revert All",
        description: "Revert all modified files",
        scope: ActionContext::WorkbenchReview,
        keybindings: &["Ctrl+Z"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: false,
        availability: always_available,
        activity_message: None,
    },
    // Workbench — Sessions
    ActionSpec {
        id: ActionId::ResumeCheckpoint,
        title: "Resume",
        description: "Resume session from checkpoint",
        scope: ActionContext::WorkbenchSessions,
        keybindings: &["Ctrl+R"],
        slash_aliases: &["/resume"],
        palette_visible: true,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::SwitchToSession,
        title: "Switch",
        description: "Switch to selected session",
        scope: ActionContext::WorkbenchSessions,
        keybindings: &["Enter"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    // Workbench — Runtime
    ActionSpec {
        id: ActionId::RefreshRuntime,
        title: "Refresh",
        description: "Refresh runtime job list",
        scope: ActionContext::WorkbenchRuntime,
        keybindings: &["r"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::CancelRuntimeJob,
        title: "Cancel Job",
        description: "Cancel selected runtime job",
        scope: ActionContext::WorkbenchRuntime,
        keybindings: &["c"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    // Workbench — Plan
    ActionSpec {
        id: ActionId::ApprovePlan,
        title: "Approve Plan",
        description: "Approve the current plan",
        scope: ActionContext::WorkbenchPlan,
        keybindings: &["a"],
        slash_aliases: &["/plan"],
        palette_visible: true,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::EditPlan,
        title: "Edit Plan",
        description: "Open plan in external editor",
        scope: ActionContext::WorkbenchPlan,
        keybindings: &["e"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    // Workbench — VIL
    ActionSpec {
        id: ActionId::RunRepair,
        title: "Repair",
        description: "Run VIL repair on codebase",
        scope: ActionContext::WorkbenchVil,
        keybindings: &["R"],
        slash_aliases: &["/repair"],
        palette_visible: true,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    ActionSpec {
        id: ActionId::RunAudit,
        title: "Audit",
        description: "Run VIL audit",
        scope: ActionContext::WorkbenchVil,
        keybindings: &["A"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: always_available,
        activity_message: None,
    },
    // Workbench — tab navigation (WorkbenchAny: shown for all WB tabs)
    ActionSpec {
        id: ActionId::CycleWorkbenchTab,
        title: "Next Tab",
        description: "Cycle to the next workbench tab",
        scope: ActionContext::WorkbenchAny,
        keybindings: &["Tab"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: |s| s.focus == crate::app::WorkspaceFocus::Workbench,
        activity_message: None,
    },
    // Overlay — generic close
    ActionSpec {
        id: ActionId::CloseOverlay,
        title: "Close",
        description: "Close the active overlay",
        scope: ActionContext::OverlayActive,
        keybindings: &["Esc"],
        slash_aliases: &[],
        palette_visible: false,
        footer_visible: true,
        availability: |s| s.overlay_manager.any_active(),
        activity_message: None,
    },
];

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
    );
    ACTION_SPECS.iter().filter(move |s| {
        s.scope == ctx
            || (ctx != ActionContext::OverlayActive && s.scope == ActionContext::Global)
            || (is_workbench && s.scope == ActionContext::WorkbenchAny)
    })
}

/// Look up a spec by slash alias.
pub fn spec_by_slash_alias(alias: &str) -> Option<&'static ActionSpec> {
    ACTION_SPECS.iter().find(|s| s.slash_aliases.contains(&alias))
}

/// All specs visible in the footer for a given context.
pub fn footer_specs(ctx: ActionContext) -> impl Iterator<Item = &'static ActionSpec> {
    specs_for_context(ctx).filter(|s| s.footer_visible)
}

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
    /// Matches any active workbench tab — use for shared tab-level hints.
    WorkbenchAny,
    OverlayActive,
}

impl ActionContext {
    pub fn from_app_state(state: &crate::app::AppState) -> Self {
        if state.overlay_manager.any_active() {
            return ActionContext::OverlayActive;
        }

        match state.focus {
            WorkspaceFocus::Input => ActionContext::InputFocus,
            WorkspaceFocus::Conversation => ActionContext::ConversationFocus,
            WorkspaceFocus::Activity => ActionContext::ActivityFocus,
            WorkspaceFocus::Workbench => match state.workbench_tab {
                WorkbenchTab::Approvals => ActionContext::WorkbenchApprovals,
                WorkbenchTab::Review => ActionContext::WorkbenchReview,
                WorkbenchTab::Sessions => ActionContext::WorkbenchSessions,
                WorkbenchTab::Agents => ActionContext::WorkbenchAgents,
                WorkbenchTab::Runtime => ActionContext::WorkbenchRuntime,
                WorkbenchTab::Plan => ActionContext::WorkbenchPlan,
                WorkbenchTab::Vil => ActionContext::WorkbenchVil,
            },
        }
    }
}

