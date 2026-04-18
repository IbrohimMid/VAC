//! Overlay Manager
//!
//! Centralizes all modal overlay state: open/close lifecycle, event-capture
//! priority (topmost overlay wins), render order, and focus restore when an
//! overlay is dismissed.

use crate::tui::app::{AppState, InputEvent, OutputEvent, WorkspaceFocus};
use ratatui::Frame;
use tokio::sync::mpsc::Sender;

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
}

/// Render order (lower index = rendered first = underneath).
const RENDER_ORDER: &[OverlayId] = &[
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

// ── Sync helpers ────────────────────────────────────────────────────────────
//
// These helpers keep `OverlayManager` in sync with the existing `show_X` bool
// flags on AppState.  They are the *only* place that should mutate both the
// manager stack and the legacy flags.  Once all callers migrate to going
// through the manager, the flags can be removed.

/// Open an overlay: updates both the manager stack and the legacy show_ flag.
pub fn open_overlay(state: &mut AppState, id: OverlayId) {
    let focus = state.focus;
    state.overlay_manager.push(id, focus);
    set_show_flag(state, id, true);
}

/// Close an overlay: removes it from the stack and clears the legacy flag.
/// Restores focus if the stack drains to empty.
pub fn close_overlay(state: &mut AppState, id: OverlayId) {
    if let Some(restored) = state.overlay_manager.pop(id) {
        state.focus = restored;
    }
    set_show_flag(state, id, false);
}

/// Close all overlays.
pub fn close_all_overlays(state: &mut AppState) {
    if let Some(restored) = state.overlay_manager.pop_all() {
        state.focus = restored;
    }
    for id in RENDER_ORDER {
        set_show_flag(state, *id, false);
    }
}

fn set_show_flag(state: &mut AppState, id: OverlayId, value: bool) {
    match id {
        OverlayId::CommandPalette => state.show_command_palette = value,
        OverlayId::Shortcuts => state.show_shortcuts = value,
        OverlayId::IsolationSwitcher => state.show_isolation_switcher = value,
        OverlayId::ProfileSwitcher => state.show_profile_switcher = value,
        OverlayId::RulebookSwitcher => state.show_rulebook_switcher = value,
        OverlayId::ModelSwitcher => state.show_model_switcher = value,
        OverlayId::FileSearch => state.show_file_search = value,
        OverlayId::Changeset => state.show_changeset = value,
        OverlayId::FileChanges => state.show_file_changes_popup = value,
        OverlayId::PlanReview => state.plan.review_open = value,
        OverlayId::AskUser => state.show_ask_user_popup = value,
        OverlayId::ShellPopup => state.shell.session_store.popup_visible = value,
        OverlayId::MessageAction => state.show_message_action_popup = value,
        OverlayId::HelperDropdown => state.show_helper_dropdown = value,
        OverlayId::AtDropdown => { /* at_trigger_active is not a plain bool toggle */ }
    }
}

/// Render-order iterator of currently active overlays (for use in view.rs).
pub fn active_render_ids(state: &AppState) -> impl Iterator<Item = OverlayId> + '_ {
    state.overlay_manager.render_order()
}

/// Trait for rendering and handling input for UI overlays (kept for
/// compatibility; concrete impls can migrate to this gradually).
pub trait Overlay {
    fn render(&self, f: &mut Frame, state: &AppState);

    fn handle_event(
        &mut self,
        state: &mut AppState,
        output_tx: &Sender<OutputEvent>,
        event: InputEvent,
    ) -> Result<bool, String>;

    fn is_active(&self, state: &AppState) -> bool;
}
