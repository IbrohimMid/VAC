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
    OverlayId::ProfileSwitcher,
    OverlayId::RulebookSwitcher,
    OverlayId::AskUser,
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

    /// Push an overlay onto the active stack.  Idempotent: pushing an already-
    /// active overlay is a no-op.
    pub fn push(&mut self, id: OverlayId, current_focus: WorkspaceFocus) {
        if !self.stack.contains(&id) {
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
    let focus = state.focus;
    state.overlay_manager.push(id, focus);
    sync_domain_state(state, id, true);
}

/// Close an overlay. Restores focus if the stack drains to empty.
pub fn close_overlay(state: &mut AppState, id: OverlayId) {
    if let Some(restored) = state.overlay_manager.pop(id) {
        state.focus = restored;
    }
    sync_domain_state(state, id, false);
}

/// Close all overlays.
pub fn close_all_overlays(state: &mut AppState) {
    if let Some(restored) = state.overlay_manager.pop_all() {
        state.focus = restored;
    }
    for id in RENDER_ORDER {
        sync_domain_state(state, *id, false);
    }
}

/// Sync non-OverlayManager domain state that some overlays back with a bool or struct field.
fn sync_domain_state(state: &mut AppState, id: OverlayId, value: bool) {
    match id {
        OverlayId::PlanReview => state.plan.review_open = value,
        OverlayId::ShellPopup => state.shell.session_store.popup_visible = value,
        OverlayId::AtDropdown => {
            state.at_trigger_active = value;
            if !value {
                state.at_query.clear();
                state.at_results.clear();
                state.at_selected_idx = 0;
            }
        }
        OverlayId::RejectReason => {
            if value {
                if state.reject_reason_input.is_none() {
                    state.reject_reason_input = Some(String::new());
                }
            } else {
                state.reject_reason_input = None;
            }
        }
        OverlayId::ReviewPane => state.review.open = value,
        // These overlays carry no additional domain state beyond the stack itself.
        OverlayId::CommandPalette
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
        | OverlayId::FilePicker => {}
    }
}

/// Render-order iterator of currently active overlays (for use in view.rs).
pub fn active_render_ids(state: &AppState) -> impl Iterator<Item = OverlayId> + '_ {
    state.overlay_manager.render_order()
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
        assert!(!state.plan.review_open);
        open_overlay(&mut state, OverlayId::PlanReview);
        assert!(state.overlay_manager.is_active(OverlayId::PlanReview));
        assert!(state.plan.review_open);
        close_overlay(&mut state, OverlayId::PlanReview);
        assert!(!state.overlay_manager.is_active(OverlayId::PlanReview));
        assert!(!state.plan.review_open);
    }

    #[test]
    fn sync_shell_popup_visible_on_open_overlay() {
        let mut state = make_state();
        assert!(!state.shell.session_store.popup_visible);
        open_overlay(&mut state, OverlayId::ShellPopup);
        assert!(state.overlay_manager.is_active(OverlayId::ShellPopup));
        assert!(state.shell.session_store.popup_visible);
        close_overlay(&mut state, OverlayId::ShellPopup);
        assert!(!state.overlay_manager.is_active(OverlayId::ShellPopup));
        assert!(!state.shell.session_store.popup_visible);
    }

    #[test]
    fn sync_at_trigger_active_on_open_overlay() {
        let mut state = make_state();
        assert!(!state.at_trigger_active);
        open_overlay(&mut state, OverlayId::AtDropdown);
        assert!(state.overlay_manager.is_active(OverlayId::AtDropdown));
        assert!(state.at_trigger_active);
        close_overlay(&mut state, OverlayId::AtDropdown);
        assert!(!state.overlay_manager.is_active(OverlayId::AtDropdown));
        assert!(!state.at_trigger_active);
    }

    #[test]
    fn sync_reject_reason_input_on_open_overlay() {
        let mut state = make_state();
        assert!(state.reject_reason_input.is_none());
        open_overlay(&mut state, OverlayId::RejectReason);
        assert!(state.overlay_manager.is_active(OverlayId::RejectReason));
        assert!(state.reject_reason_input.is_some());
        close_overlay(&mut state, OverlayId::RejectReason);
        assert!(!state.overlay_manager.is_active(OverlayId::RejectReason));
        assert!(state.reject_reason_input.is_none());
    }

    #[test]
    fn sync_review_open_on_open_overlay() {
        let mut state = make_state();
        assert!(!state.review.open);
        open_overlay(&mut state, OverlayId::ReviewPane);
        assert!(state.overlay_manager.is_active(OverlayId::ReviewPane));
        assert!(state.review.open);
        close_overlay(&mut state, OverlayId::ReviewPane);
        assert!(!state.overlay_manager.is_active(OverlayId::ReviewPane));
        assert!(!state.review.open);
    }
}
