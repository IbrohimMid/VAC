//! Action Registry
//!
//! Provides a single source of truth for all TUI actions, their keybindings,
//! descriptions, and contexts.

use crate::tui::app::{InputEvent, WorkspaceFocus, WorkbenchTab};

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
    OverlayActive,
}

impl ActionContext {
    pub fn from_app_state(state: &crate::tui::app::AppState) -> Self {
        // Basic resolution of context
        if state.show_command_palette
            || state.show_shortcuts
            || state.show_isolation_switcher
            || state.show_profile_switcher
            || state.show_rulebook_switcher
            || state.show_message_action_popup
            || state.show_model_switcher
            || state.show_file_search
            || state.show_changeset
            || state.shell.popup_visible
            || state.show_ask_user_popup
            || state.reject_reason_input.is_some()
            || state.plan.review_open
            || state.show_file_changes_popup
            || state.show_helper_dropdown
            || state.at_trigger_active
        {
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

#[derive(Debug, Clone)]
pub struct UiAction {
    pub id: &'static str,
    pub description: &'static str,
    pub keys: &'static [&'static str],
    pub context: ActionContext,
}

pub struct ActionRegistry {
    actions: Vec<UiAction>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            actions: Vec::new(),
        };
        registry.register_defaults();
        registry
    }

    fn register_defaults(&mut self) {
        // Global Actions
        self.actions.push(UiAction {
            id: "quit",
            description: "Quit",
            keys: &["Ctrl+C"],
            context: ActionContext::Global,
        });
        self.actions.push(UiAction {
            id: "command_palette",
            description: "Command Palette",
            keys: &["Ctrl+P"],
            context: ActionContext::Global,
        });
        self.actions.push(UiAction {
            id: "next_pane",
            description: "Next Pane",
            keys: &["Tab"],
            context: ActionContext::Global,
        });
        self.actions.push(UiAction {
            id: "next_tab",
            description: "Next Tab",
            keys: &["Ctrl+Tab"],
            context: ActionContext::Global,
        });

        // Input Focus
        self.actions.push(UiAction {
            id: "submit",
            description: "Submit",
            keys: &["Enter"],
            context: ActionContext::InputFocus,
        });
        self.actions.push(UiAction {
            id: "history_up",
            description: "History Up",
            keys: &["Up"],
            context: ActionContext::InputFocus,
        });
        self.actions.push(UiAction {
            id: "history_down",
            description: "History Down",
            keys: &["Down"],
            context: ActionContext::InputFocus,
        });

        // Workbench Approvals
        self.actions.push(UiAction {
            id: "approve",
            description: "Approve",
            keys: &["a"],
            context: ActionContext::WorkbenchApprovals,
        });
        self.actions.push(UiAction {
            id: "reject",
            description: "Reject",
            keys: &["r"],
            context: ActionContext::WorkbenchApprovals,
        });

        // Workbench Review
        self.actions.push(UiAction {
            id: "diff",
            description: "Diff",
            keys: &["Enter"],
            context: ActionContext::WorkbenchReview,
        });
        self.actions.push(UiAction {
            id: "revert_selected",
            description: "Revert Selected",
            keys: &["Ctrl+X"],
            context: ActionContext::WorkbenchReview,
        });
        self.actions.push(UiAction {
            id: "revert_filtered",
            description: "Revert Filtered",
            keys: &["Ctrl+Y"],
            context: ActionContext::WorkbenchReview,
        });
        self.actions.push(UiAction {
            id: "revert_all",
            description: "Revert All",
            keys: &["Ctrl+Z"],
            context: ActionContext::WorkbenchReview,
        });
        self.actions.push(UiAction {
            id: "edit",
            description: "Edit",
            keys: &["Ctrl+N"],
            context: ActionContext::WorkbenchReview,
        });

        // Workbench Sessions
        self.actions.push(UiAction {
            id: "restore",
            description: "Restore",
            keys: &["Enter"],
            context: ActionContext::WorkbenchSessions,
        });
        self.actions.push(UiAction {
            id: "resume_checkpoint",
            description: "Resume Checkpoint",
            keys: &["r"],
            context: ActionContext::WorkbenchSessions,
        });

        // Workbench Runtime & Agents
        self.actions.push(UiAction {
            id: "refresh",
            description: "Refresh",
            keys: &["r"],
            context: ActionContext::WorkbenchRuntime,
        });
        self.actions.push(UiAction {
            id: "cancel",
            description: "Cancel",
            keys: &["c"],
            context: ActionContext::WorkbenchRuntime,
        });
        self.actions.push(UiAction {
            id: "retry",
            description: "Retry",
            keys: &["t"],
            context: ActionContext::WorkbenchRuntime,
        });
        self.actions.push(UiAction {
            id: "refresh_agents",
            description: "Refresh",
            keys: &["r"],
            context: ActionContext::WorkbenchAgents,
        });

        // Workbench Plan
        self.actions.push(UiAction {
            id: "edit_plan",
            description: "Edit",
            keys: &["e"],
            context: ActionContext::WorkbenchPlan,
        });
        self.actions.push(UiAction {
            id: "approve_plan",
            description: "Approve",
            keys: &["a"],
            context: ActionContext::WorkbenchPlan,
        });
        self.actions.push(UiAction {
            id: "request_changes",
            description: "Request Changes",
            keys: &["r"],
            context: ActionContext::WorkbenchPlan,
        });

        // Workbench Vil
        self.actions.push(UiAction {
            id: "repair",
            description: "Repair",
            keys: &["R"],
            context: ActionContext::WorkbenchVil,
        });
        self.actions.push(UiAction {
            id: "audit",
            description: "Audit",
            keys: &["A"],
            context: ActionContext::WorkbenchVil,
        });
        self.actions.push(UiAction {
            id: "ir_diff",
            description: "IR Diff",
            keys: &["D"],
            context: ActionContext::WorkbenchVil,
        });
        self.actions.push(UiAction {
            id: "open",
            description: "Open",
            keys: &["O"],
            context: ActionContext::WorkbenchVil,
        });

        // Navigation (Global, but contextual hints)
        self.actions.push(UiAction {
            id: "select",
            description: "Select",
            keys: &["↑/↓"],
            context: ActionContext::WorkbenchApprovals,
        });
        self.actions.push(UiAction {
            id: "select",
            description: "Select",
            keys: &["↑/↓"],
            context: ActionContext::WorkbenchReview,
        });
        self.actions.push(UiAction {
            id: "select",
            description: "Select",
            keys: &["↑/↓"],
            context: ActionContext::WorkbenchSessions,
        });
        self.actions.push(UiAction {
            id: "select",
            description: "Select",
            keys: &["↑/↓"],
            context: ActionContext::WorkbenchAgents,
        });
        self.actions.push(UiAction {
            id: "select",
            description: "Select",
            keys: &["↑/↓"],
            context: ActionContext::WorkbenchRuntime,
        });
        self.actions.push(UiAction {
            id: "select",
            description: "Select",
            keys: &["↑/↓"],
            context: ActionContext::WorkbenchVil,
        });
        self.actions.push(UiAction {
            id: "filter",
            description: "Filter",
            keys: &["←/→"],
            context: ActionContext::WorkbenchVil,
        });
    }

    pub fn get_actions_for_context(&self, context: ActionContext) -> Vec<UiAction> {
        let mut results = Vec::new();
        // Add global actions first
        for action in &self.actions {
            if action.context == ActionContext::Global {
                results.push(action.clone());
            }
        }
        // Then add context-specific actions
        if context != ActionContext::Global {
            for action in &self.actions {
                if action.context == context {
                    results.push(action.clone());
                }
            }
        }
        results
    }
}
