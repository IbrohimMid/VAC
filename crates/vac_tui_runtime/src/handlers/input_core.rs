//! Main input router.
//!
//! Routes `InputEvent` through four stages in order:
//!   1. Overlay router  — if any overlay is active, dispatch and return.
//!   2. Global events   — Tab, Esc, quit, shell Ctrl+Z, palette, etc.
//!   3. Workspace router — Input / Conversation / Activity focus.
//!   4. Workbench router — Workbench tab handlers.

use crate::app::{AppState, InputEvent, OutputEvent, WorkspaceFocus};
use crate::handlers::HandlerContext;
use crate::handlers::input_popup;
use crate::handlers::{
    approval, changeset as changeset_handler, file_search, model_switcher, profile_switcher,
    review as review_handler, rulebook_switcher, shell as shell_handler,
};
use crate::handlers::{workbench_input, workspace_input};
use tokio::sync::mpsc::Sender;

pub fn handle_input_event(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    // Stage 1: Overlay router — topmost overlay captures everything.
    if state.overlay_manager.any_active() {
        input_popup::dispatch_popup_event(state, output_tx, event);
        return;
    }

    // Stage 2: Global events (not focus-dependent).
    if handle_global(state, output_tx, &event) {
        return;
    }

    // Stage 3 & 4: Focus-based routing.
    if state.focus == WorkspaceFocus::Workbench {
        workbench_input::handle(state, output_tx, event);
    } else {
        workspace_input::handle(state, output_tx, event);
    }
}

/// Returns `true` if the event was consumed globally.
fn handle_global(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: &InputEvent,
) -> bool {
    match event {
        InputEvent::Tab => {
            state.focus = state.focus.next();
            true
        }
        InputEvent::WorkbenchNextTab => {
            if state.focus != WorkspaceFocus::Workbench {
                state.focus = WorkspaceFocus::Workbench;
            }
            state.workbench_tab = state.workbench_tab.next();
            if state.workbench_tab == crate::app::WorkbenchTab::Agents {
                let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
                let _ = output_tx.try_send(OutputEvent::LoadAgentState);
            } else if state.workbench_tab == crate::app::WorkbenchTab::Runtime {
                let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
            }
            true
        }
        InputEvent::AttemptQuit => {
            if state.streaming.is_streaming {
                // First Ctrl+C while streaming: cancel the stream, not the app.
                let _ = output_tx.try_send(OutputEvent::CancelStream);
                state.streaming.is_streaming = false;
                state.streaming.start = None;
                state.streaming.tokens = 0;
            } else {
                // Outside streaming: require two presses within 2 s to quit.
                let now = std::time::Instant::now();
                let double = state
                    .quit.first_press
                    .map(|t| now.duration_since(t) < std::time::Duration::from_secs(2))
                    .unwrap_or(false);
                if double {
                    state.quit.cancel_requested = true;
                    state.quit.press_count = 0;
                    state.quit.first_press = None;
                } else {
                    state.quit.press_count = 1;
                    state.quit.first_press = Some(now);
                    state.toasts.push(crate::services::Toast::info(
                        "Press Ctrl+C again within 2 s to quit",
                    ));
                }
            }
            true
        }
        InputEvent::HandleEsc => {
            handle_esc(state, output_tx);
            true
        }
        InputEvent::HandleCtrlZ => {
            handle_ctrl_z(state);
            true
        }
        InputEvent::BackgroundShell => {
            shell_handler::background(state);
            true
        }
        InputEvent::FocusShell => {
            shell_handler::foreground(state);
            true
        }
        InputEvent::ShellKill => {
            shell_handler::kill(state);
            if let Some(session) = state.shell.session_store.active_mut() {
                session.waiting_for_input = false;
            }
            true
        }
        InputEvent::AutoApproveCurrentTool => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::approve_current(&mut ctx);
            true
        }
        InputEvent::RejectCurrentTool => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::begin_reject_current(&mut ctx);
            true
        }
        InputEvent::ApproveAll => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::approve_all(&mut ctx);
            true
        }
        InputEvent::RejectAll => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::confirm_reject_all(&mut ctx);
            true
        }
        InputEvent::ShowCommandPalette => {
            state.command_palette.input.clear();
            state.command_palette.selected = 0;
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::CommandPalette);
            true
        }
        InputEvent::ShowModelSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = model_switcher::open(&mut ctx);
            true
        }
        InputEvent::ShowFileSearch => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = file_search::open(&mut ctx);
            true
        }
        InputEvent::ShowChangeset => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = changeset_handler::open(&mut ctx);
            true
        }
        InputEvent::ShowShortcuts => {
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::Shortcuts);
            true
        }
        InputEvent::ShowProfileSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = profile_switcher::open(&mut ctx);
            true
        }
        InputEvent::ShowIsolationSwitcher => {
            state.switchers.isolation_selected = 0;
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::IsolationSwitcher);
            true
        }
        InputEvent::ShowRulebookSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = rulebook_switcher::open(&mut ctx);
            true
        }
        InputEvent::ShowMessageActionPopup => {
            state.message_action_popup_selected = 0;
            state.message_action_target_id = state
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.id);
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::MessageAction);
            true
        }
        InputEvent::VilExprTypeHelp => {
            // PR-T12.1 stub: show a toast with type-info placeholder.
            // Full type inference deferred to Wave 4.
            let msg = if state.vil_expr_lint.pending_payload.is_some() {
                "vil-expr type inference coming soon (PR-T12 stub)"
            } else {
                "Alt+H: no vil-expr: payload detected"
            };
            state.toasts.push(crate::services::Toast::info(msg));
            true
        }
        InputEvent::ToggleSidePanel => {
            state.side_panel.visible = !state.side_panel.visible;
            if !state.side_panel.visible {
                state.side_panel.row_areas.clear();
            }
            true
        }
        InputEvent::ToggleAutoApprove => {
            state.auto_approve = !state.auto_approve;
            let msg = if state.auto_approve {
                "Permission Mode: AUTO-APPROVE (Low-risk tools will run without confirmation)"
            } else {
                "Permission Mode: PROMPT (You will be prompted for tool execution)"
            };
            state.add_assistant_message(msg.to_string());
            true
        }
        InputEvent::RequestSessionList => {
            let _ = output_tx.try_send(OutputEvent::ListSessions);
            true
        }
        InputEvent::NewSession => {
            let _ = output_tx.try_send(OutputEvent::NewSession);
            true
        }
        InputEvent::ReviewOpen => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = review_handler::open(&mut ctx);
            true
        }
        InputEvent::MouseDragStart(col, row) => {
            handle_mouse_drag_start(state, output_tx, *col, *row);
            true
        }
        InputEvent::MouseDrag(col, row) => {
            crate::services::text_selection::handle_drag(state, *col, *row);
            true
        }
        InputEvent::MouseDragEnd(col, row) => {
            crate::services::text_selection::handle_drag_end(state, *col, *row);
            true
        }
        InputEvent::MouseMove(col, row) => {
            // R7 / PR-T15 real-hover extension — route cursor moves to the
            // hover dispatcher. Handled=true only when popup state actually
            // changed so we avoid spurious repaints on every cell.
            crate::handlers::mouse::dispatch_hover(state, *col, *row)
        }
        _ => false,
    }
}

/// Esc precedence (deterministic, single source of truth):
///   1. Any overlay active → close topmost.
///   2. Shell popup with running command → background.
///   3. Streaming → cancel.
///   4. Review pane open → close.
fn handle_esc(state: &mut AppState, output_tx: &Sender<OutputEvent>) {
    if state.overlay_manager.any_active() {
        if let Some(id) = state.overlay_manager.topmost() {
            crate::overlay::close_overlay(state, id);
        }
    } else if state.shell.session_store.popup_visible
        && state
            .shell
            .session_store
            .active()
            .and_then(|s| s.command.as_ref())
            .is_some()
    {
        shell_handler::background(state);
    } else if state.streaming.is_streaming {
        let _ = output_tx.try_send(OutputEvent::CancelStream);
        state.streaming.is_streaming = false;
    } else if state.focus == WorkspaceFocus::Workbench
        && state.workbench_tab == crate::app::WorkbenchTab::Review
        && state.review.open
    {
        crate::overlay::close_overlay(state, crate::overlay::OverlayId::ReviewPane);
        state.review.diff = None;
    }
}

fn handle_ctrl_z(state: &mut AppState) {
    if state
        .shell
        .session_store
        .active()
        .and_then(|s| s.command.as_ref())
        .is_some()
    {
        if state.shell.session_store.popup_visible {
            shell_handler::background(state);
        } else {
            shell_handler::foreground(state);
        }
        if let Some(session) = state.shell.session_store.active_mut() {
            session.history_idx = None;
        }
    }
}

fn handle_mouse_drag_start(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    col: u16,
    row: u16,
) {
    let banner_active = state
        .banner.message
        .as_ref()
        .is_some_and(|m| !m.is_expired());

    if banner_active {
        if let Some(rect) = state.banner.dismiss_region {
            if col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
            {
                state.banner.message = None;
                state.banner.click_regions.clear();
                state.banner.dismiss_region = None;
                return;
            }
        }
        let mut banner_action: Option<String> = None;
        for (action, rect) in &state.banner.click_regions {
            if col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
            {
                banner_action = Some(action.clone());
                break;
            }
        }
        if let Some(action) = banner_action {
            state.banner.message = None;
            state.banner.click_regions.clear();
            state.banner.dismiss_region = None;
            let _ = output_tx.try_send(OutputEvent::UserMessage(action, None, Vec::new(), None));
            return;
        }
    } else if state.banner.message.is_some() {
        state.banner.message = None;
        state.banner.click_regions.clear();
        state.banner.dismiss_region = None;
    }

    if state.side_panel.visible {
        for (sec, rect) in &state.side_panel.header_areas {
            if col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
            {
                if state.side_panel.section_collapsed.contains(sec) {
                    state.side_panel.section_collapsed.remove(sec);
                } else {
                    state.side_panel.section_collapsed.insert(*sec);
                }
                return;
            }
        }
        for (action, rect) in &state.side_panel.row_areas {
            if col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
            {
                match action.clone() {
                    crate::app::SidePanelRowAction::SwitchSession(id) => {
                        let _ = output_tx.try_send(OutputEvent::SwitchToSession(id));
                    }
                    crate::app::SidePanelRowAction::ShowMcpDetail(name) => {
                        state.push_activity(
                            crate::app::ActivityKind::Mcp,
                            format!("MCP detail: {}", name),
                        );
                        state.focus = WorkspaceFocus::Workbench;
                        state.workbench_tab = crate::app::WorkbenchTab::Runtime;
                    }
                    crate::app::SidePanelRowAction::JumpToVilIssue(path) => {
                        state.review.selected_path = Some(path);
                        state.review.selected_idx = 0;
                        state.focus = WorkspaceFocus::Workbench;
                        state.workbench_tab = crate::app::WorkbenchTab::Review;
                    }
                }
                return;
            }
        }
    }

    // PR-T16 — workbench tab + task tray click dispatch.
    if crate::handlers::mouse::dispatch_click(state, output_tx, col, row) {
        return;
    }

    if row == 0 {
        let term_width = crossterm::terminal::size().map(|s| s.0).unwrap_or(80);
        if col > term_width.saturating_sub(40) {
            state.side_panel.visible = !state.side_panel.visible;
        }
    } else {
        crate::services::text_selection::handle_drag_start(state, col, row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{SidePanelRowAction, SidePanelSection, WorkbenchTab};
    use ratatui::layout::Rect;
    use tokio::sync::mpsc;

    fn make_state_with_channel() -> (
        AppState,
        mpsc::Sender<OutputEvent>,
        mpsc::Receiver<OutputEvent>,
    ) {
        let state = AppState::default();
        let (tx, rx) = mpsc::channel::<OutputEvent>(64);
        (state, tx, rx)
    }

    #[test]
    fn mouse_click_on_side_panel_header_toggles_section() {
        let (mut state, tx, _rx) = make_state_with_channel();
        state.side_panel.visible = true;
        state
            .side_panel.header_areas
            .insert(SidePanelSection::Sessions, Rect::new(1, 5, 20, 1));

        handle_input_event(&mut state, &tx, InputEvent::MouseDragStart(2, 5));
        assert!(
            state
                .side_panel.section_collapsed
                .contains(&SidePanelSection::Sessions)
        );

        handle_input_event(&mut state, &tx, InputEvent::MouseDragStart(2, 5));
        assert!(
            !state
                .side_panel.section_collapsed
                .contains(&SidePanelSection::Sessions)
        );
    }

    #[test]
    fn mouse_click_on_side_panel_session_row_emits_switch_session() {
        let (mut state, tx, mut rx) = make_state_with_channel();
        state.side_panel.visible = true;
        state.side_panel.row_areas.push((
            SidePanelRowAction::SwitchSession("session-42".to_string()),
            Rect::new(1, 8, 20, 1),
        ));

        handle_input_event(&mut state, &tx, InputEvent::MouseDragStart(3, 8));

        match rx.try_recv().expect("expected SwitchToSession event") {
            OutputEvent::SwitchToSession(id) => assert_eq!(id, "session-42"),
            other => panic!("unexpected output event: {other:?}"),
        }
    }

    #[test]
    fn mouse_click_on_side_panel_vil_issue_row_opens_review() {
        let (mut state, tx, _rx) = make_state_with_channel();
        state.side_panel.visible = true;
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Runtime;
        state.side_panel.row_areas.push((
            SidePanelRowAction::JumpToVilIssue("src/lib.rs".to_string()),
            Rect::new(1, 9, 20, 1),
        ));

        handle_input_event(&mut state, &tx, InputEvent::MouseDragStart(3, 9));

        assert_eq!(state.review.selected_path.as_deref(), Some("src/lib.rs"));
        assert_eq!(state.workbench_tab, WorkbenchTab::Review);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }
}
