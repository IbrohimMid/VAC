//! Overlay Manager
//!
//! Centralizes all modal overlay state: open/close lifecycle, event-capture
//! priority (topmost overlay wins), render order, and focus restore when an
//! overlay is dismissed.

use crate::app::{AppState, WorkspaceFocus};

/// Identifies a specific overlay/popup in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayId {
    CommandPalette,
    InitChecklist,
    Shortcuts,
    IsolationSwitcher,
    ProfileSwitcher,
    RulebookSwitcher,
    ModelSwitcher,
    FileSearch,
    Changeset,
    FileChanges,
    PlanReview,
    AskUser,
    ShellPopup,
    MessageAction,
    HelperDropdown,
    AtDropdown,
    /// Approval rejection reason prompt (modal input under an approval).
    RejectReason,
    /// Review workbench detail pane (modal when an item is open).
    ReviewPane,
    /// Background task tray (bottom-right, shows running/queued/completed jobs).
    TaskTray,
    /// Theme preset picker overlay (Ctrl+Shift+T).
    ThemePicker,
    /// Session resume overlay (Ctrl+R).
    SessionResume,
    /// File picker v2 (multi-select, dir nav, preview).
    FilePicker,
    /// QW.2 — `/context` operator overlay. Reuses the
    /// Shortcuts-style centred modal; no new domain state. Read
    /// from `AppState.operator_config.billing`.
    ContextInspector,
    /// G1 — MCP elicitation/request modal. Centred popup showing
    /// the URL (or prompt) + [Enter] open / [Esc] cancel footer.
    /// The prompt + oneshot sender live on
    /// `AppState.layout.elicitation`.
    Elicitation,
    ConfirmDangerMode,
}

/// Render order (lower index = rendered first = underneath).
const RENDER_ORDER: &[OverlayId] = &[
    OverlayId::ReviewPane,
    OverlayId::Changeset,
    OverlayId::FileSearch,
    OverlayId::FileChanges,
    OverlayId::PlanReview,
    OverlayId::ShellPopup,
    OverlayId::ModelSwitcher,
    OverlayId::MessageAction,
    OverlayId::IsolationSwitcher,
    OverlayId::ConfirmDangerMode,
    OverlayId::ProfileSwitcher,
    OverlayId::RulebookSwitcher,
    OverlayId::AskUser,
    OverlayId::Elicitation,
    OverlayId::ContextInspector,
    OverlayId::InitChecklist,
    OverlayId::Shortcuts,
    OverlayId::CommandPalette,
    OverlayId::HelperDropdown,
    OverlayId::AtDropdown,
    OverlayId::RejectReason,
    OverlayId::TaskTray,
    OverlayId::ThemePicker,
    OverlayId::SessionResume,
    OverlayId::FilePicker,
];

/// Manages the active overlay stack.
///
/// The stack is ordered from oldest (bottom) to newest (top).  The topmost
/// overlay captures keyboard events first.  When the stack is empty, events
/// fall through to the workspace.
#[derive(Debug, Clone, Default)]
pub struct OverlayManager {
    /// Active overlay stack (bottom → top).
    stack: Vec<OverlayId>,
    /// The focus that was active before any overlay was pushed; restored when
    /// the stack drains to empty.
    saved_focus: Option<WorkspaceFocus>,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Maximum simultaneous overlays. Deeper modal nesting hurts
    /// cognitive load (Claude-Code-friction rule). New pushes beyond this
    /// depth are refused with a `tracing::warn!` log so upstream code can
    /// notice the bug without crashing the session.
    pub const MAX_STACK_DEPTH: usize = 2;

    /// Push an overlay onto the active stack.  Idempotent: pushing an already-
    /// active overlay is a no-op. Refuses to push if stack is already at
    /// `MAX_STACK_DEPTH`.
    pub fn push(&mut self, id: OverlayId, current_focus: WorkspaceFocus) {
        if !self.stack.contains(&id) {
            if self.stack.len() >= Self::MAX_STACK_DEPTH {
                tracing::warn!(
                    topmost = ?self.stack.last(),
                    rejected = ?id,
                    "overlay push refused: stack at MAX_STACK_DEPTH"
                );
                return;
            }
            if self.stack.is_empty() {
                self.saved_focus = Some(current_focus);
            }
            self.stack.push(id);
        }
    }

    /// Remove an overlay from the stack (regardless of position).
    /// Returns the focus to restore if the stack is now empty.
    pub fn pop(&mut self, id: OverlayId) -> Option<WorkspaceFocus> {
        self.stack.retain(|&x| x != id);
        if self.stack.is_empty() {
            self.saved_focus.take()
        } else {
            None
        }
    }

    /// Close all overlays and return the saved focus (if any).
    pub fn pop_all(&mut self) -> Option<WorkspaceFocus> {
        self.stack.clear();
        self.saved_focus.take()
    }

    /// The topmost (event-capturing) overlay, if any.
    pub fn topmost(&self) -> Option<OverlayId> {
        self.stack.last().copied()
    }

    /// Whether any overlay is currently active.
    pub fn any_active(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Whether a specific overlay is active anywhere in the stack.
    pub fn is_active(&self, id: OverlayId) -> bool {
        self.stack.contains(&id)
    }

    /// Ordered list of overlay IDs to render (bottom → top).
    pub fn render_order(&self) -> impl Iterator<Item = OverlayId> + '_ {
        RENDER_ORDER
            .iter()
            .copied()
            .filter(|id| self.stack.contains(id))
    }
}

/// Open an overlay and update any associated non-bool domain state.
pub fn open_overlay(state: &mut AppState, id: OverlayId) {
    let focus = state.layout.focus;
    state.layout.overlay_manager.push(id, focus);
    sync_domain_state(state, id, true);
}

/// Close an overlay. Restores focus if the stack drains to empty.
pub fn close_overlay(state: &mut AppState, id: OverlayId) {
    if let Some(restored) = state.layout.overlay_manager.pop(id) {
        state.layout.focus = restored;
    }
    sync_domain_state(state, id, false);
}

/// Close all overlays.
pub fn close_all_overlays(state: &mut AppState) {
    if let Some(restored) = state.layout.overlay_manager.pop_all() {
        state.layout.focus = restored;
    }
    for id in RENDER_ORDER {
        sync_domain_state(state, *id, false);
    }
}

/// Sync non-OverlayManager domain state that some overlays back with a bool or struct field.
fn sync_domain_state(state: &mut AppState, id: OverlayId, value: bool) {
    match id {
        OverlayId::PlanReview => state.workspace.plan.review_open = value,
        OverlayId::ShellPopup => state.execution.shell.session_store.popup_visible = value,
        OverlayId::AtDropdown => {
            state.composer.at_mention.trigger_active = value;
            if !value {
                state.composer.at_mention.query.clear();
                state.composer.at_mention.results.clear();
                state.composer.at_mention.selected_idx = 0;
            }
        }
        OverlayId::RejectReason => {
            if value {
                if state.execution.approvals.reject_reason_input.is_none() {
                    state.execution.approvals.reject_reason_input = Some(String::new());
                }
            } else {
                state.execution.approvals.reject_reason_input = None;
            }
        }
        OverlayId::ReviewPane => state.workspace.review.open = value,
        OverlayId::Elicitation => {
            // When the overlay is dismissed without the operator
            // explicitly resolving it (e.g. Esc), drop the prompt so
            // the oneshot sender fires Cancelled via its Drop path
            // for any awaiting handler.
            if !value {
                state.layout.elicitation = None;
            }
        }
        // These overlays carry no additional domain state beyond the stack itself.
        OverlayId::CommandPalette
        | OverlayId::InitChecklist
        | OverlayId::Shortcuts
        | OverlayId::IsolationSwitcher
        | OverlayId::ProfileSwitcher
        | OverlayId::RulebookSwitcher
        | OverlayId::ModelSwitcher
        | OverlayId::FileSearch
        | OverlayId::Changeset
        | OverlayId::FileChanges
        | OverlayId::AskUser
        | OverlayId::MessageAction
        | OverlayId::HelperDropdown
        | OverlayId::TaskTray
        | OverlayId::ThemePicker
        | OverlayId::SessionResume
        | OverlayId::FilePicker
        | OverlayId::ContextInspector
        | OverlayId::ConfirmDangerMode => {}
    }
}

/// Render-order iterator of currently active overlays (for use in view.rs).
pub fn active_render_ids(state: &AppState) -> impl Iterator<Item = OverlayId> + '_ {
    state.layout.overlay_manager.render_order()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::app::{AppState, AppStateOptions};

    fn make_state() -> AppState {
        AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        })
    }

    #[test]
    fn sync_plan_review_open_on_open_overlay() {
        let mut state = make_state();
        assert!(!state.workspace.plan.review_open);
        open_overlay(&mut state, OverlayId::PlanReview);
        assert!(
            state
                .layout
                .overlay_manager
                .is_active(OverlayId::PlanReview)
        );
        assert!(state.workspace.plan.review_open);
        close_overlay(&mut state, OverlayId::PlanReview);
        assert!(
            !state
                .layout
                .overlay_manager
                .is_active(OverlayId::PlanReview)
        );
        assert!(!state.workspace.plan.review_open);
    }

    #[test]
    fn sync_shell_popup_visible_on_open_overlay() {
        let mut state = make_state();
        assert!(!state.execution.shell.session_store.popup_visible);
        open_overlay(&mut state, OverlayId::ShellPopup);
        assert!(
            state
                .layout
                .overlay_manager
                .is_active(OverlayId::ShellPopup)
        );
        assert!(state.execution.shell.session_store.popup_visible);
        close_overlay(&mut state, OverlayId::ShellPopup);
        assert!(
            !state
                .layout
                .overlay_manager
                .is_active(OverlayId::ShellPopup)
        );
        assert!(!state.execution.shell.session_store.popup_visible);
    }

    #[test]
    fn sync_at_trigger_active_on_open_overlay() {
        let mut state = make_state();
        assert!(!state.composer.at_mention.trigger_active);
        open_overlay(&mut state, OverlayId::AtDropdown);
        assert!(
            state
                .layout
                .overlay_manager
                .is_active(OverlayId::AtDropdown)
        );
        assert!(state.composer.at_mention.trigger_active);
        close_overlay(&mut state, OverlayId::AtDropdown);
        assert!(
            !state
                .layout
                .overlay_manager
                .is_active(OverlayId::AtDropdown)
        );
        assert!(!state.composer.at_mention.trigger_active);
    }

    #[test]
    fn sync_reject_reason_input_on_open_overlay() {
        let mut state = make_state();
        assert!(state.execution.approvals.reject_reason_input.is_none());
        open_overlay(&mut state, OverlayId::RejectReason);
        assert!(
            state
                .layout
                .overlay_manager
                .is_active(OverlayId::RejectReason)
        );
        assert!(state.execution.approvals.reject_reason_input.is_some());
        close_overlay(&mut state, OverlayId::RejectReason);
        assert!(
            !state
                .layout
                .overlay_manager
                .is_active(OverlayId::RejectReason)
        );
        assert!(state.execution.approvals.reject_reason_input.is_none());
    }

    #[test]
    fn sync_review_open_on_open_overlay() {
        let mut state = make_state();
        assert!(!state.workspace.review.open);
        open_overlay(&mut state, OverlayId::ReviewPane);
        assert!(
            state
                .layout
                .overlay_manager
                .is_active(OverlayId::ReviewPane)
        );
        assert!(state.workspace.review.open);
        close_overlay(&mut state, OverlayId::ReviewPane);
        assert!(
            !state
                .layout
                .overlay_manager
                .is_active(OverlayId::ReviewPane)
        );
        assert!(!state.workspace.review.open);
    }

    // ── Z-order / stack contract tests (D3) ─────────────────────────────

    #[test]
    fn contract_push_is_idempotent() {
        let mut m = OverlayManager::new();
        m.push(OverlayId::CommandPalette, WorkspaceFocus::Input);
        m.push(OverlayId::CommandPalette, WorkspaceFocus::Input);
        m.push(OverlayId::CommandPalette, WorkspaceFocus::Input);
        assert_eq!(m.render_order().count(), 1);
    }

    #[test]
    fn contract_topmost_is_last_pushed() {
        let mut m = OverlayManager::new();
        m.push(OverlayId::CommandPalette, WorkspaceFocus::Input);
        m.push(OverlayId::Shortcuts, WorkspaceFocus::Input);
        assert_eq!(m.topmost(), Some(OverlayId::Shortcuts));
    }

    #[test]
    fn contract_pop_from_middle_removes_only_that_overlay() {
        let mut m = OverlayManager::new();
        m.push(OverlayId::CommandPalette, WorkspaceFocus::Input);
        m.push(OverlayId::Shortcuts, WorkspaceFocus::Input);
        m.pop(OverlayId::CommandPalette);
        assert!(!m.is_active(OverlayId::CommandPalette));
        assert!(m.is_active(OverlayId::Shortcuts));
        assert_eq!(m.topmost(), Some(OverlayId::Shortcuts));
    }

    #[test]
    fn contract_saved_focus_only_restored_when_stack_empties() {
        let mut m = OverlayManager::new();
        m.push(OverlayId::CommandPalette, WorkspaceFocus::Input);
        m.push(OverlayId::Shortcuts, WorkspaceFocus::Workbench);
        // Popping one does not restore focus.
        assert!(m.pop(OverlayId::Shortcuts).is_none());
        // Popping the last one restores the original focus (the one saved
        // on first push).
        assert_eq!(
            m.pop(OverlayId::CommandPalette),
            Some(WorkspaceFocus::Input)
        );
    }

    #[test]
    fn contract_stack_depth_capped_at_max() {
        let mut m = OverlayManager::new();
        m.push(OverlayId::CommandPalette, WorkspaceFocus::Input);
        m.push(OverlayId::Shortcuts, WorkspaceFocus::Input);
        // Third push should be rejected (warning logged).
        m.push(OverlayId::AskUser, WorkspaceFocus::Input);
        assert_eq!(m.render_order().count(), OverlayManager::MAX_STACK_DEPTH);
        assert!(!m.is_active(OverlayId::AskUser));
    }

    #[test]
    fn contract_render_order_follows_canonical_not_push_order() {
        // Push in "wrong" order and verify render_order re-sorts via RENDER_ORDER.
        let mut m = OverlayManager::new();
        m.push(OverlayId::TaskTray, WorkspaceFocus::Input);
        m.push(OverlayId::ReviewPane, WorkspaceFocus::Input);
        let order: Vec<_> = m.render_order().collect();
        let review_idx = order
            .iter()
            .position(|&x| x == OverlayId::ReviewPane)
            .unwrap();
        let tray_idx = order
            .iter()
            .position(|&x| x == OverlayId::TaskTray)
            .unwrap();
        // Per RENDER_ORDER, ReviewPane renders below TaskTray.
        assert!(review_idx < tray_idx);
    }
}
