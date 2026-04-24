//! Popup/overlay input dispatcher.
//!
//! Dispatch order: `topmost()` from `OverlayManager` determines which overlay
//! captures input. Each overlay is handled by a dedicated function, many extracted
//! to submodules for clarity.

mod ask_user;
mod at_mention;
mod file_search;
mod misc_overlays;
mod model_switcher;
mod session_resume;

pub use file_search::refresh_file_picker_results_pub;
pub(crate) use session_resume::refresh_session_resume_filtered;

use crate::app::{AppState, InputEvent, OutputEvent};
use crate::handlers::HandlerContext;
use crate::handlers::{
    approval, changeset as changeset_handler, isolation_switcher, message_action, profile_switcher,
    review as review_handler, rulebook_switcher,
};
use crate::overlay::OverlayId;
use tokio::sync::mpsc::Sender;

/// Dispatch event to the topmost active overlay. Always consumes the event.
pub fn dispatch_popup_event(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) -> bool {
    match state.layout.overlay_manager.topmost() {
        Some(OverlayId::RejectReason) => {
            handle_reject_reason(state, output_tx, event);
            true
        }
        Some(OverlayId::AskUser) => {
            ask_user::handle_ask_user(state, output_tx, event);
            true
        }
        Some(OverlayId::PlanReview) => {
            crate::handlers::plan::handle_plan_review_key(state, &event);
            true
        }
        Some(OverlayId::FileChanges) => {
            misc_overlays::handle_file_changes(state, output_tx, event);
            true
        }
        Some(OverlayId::HelperDropdown) => {
            misc_overlays::handle_helper_dropdown(state, event);
            true
        }
        Some(OverlayId::AtDropdown) => {
            at_mention::handle_at_dropdown(state, event);
            true
        }
        Some(OverlayId::CommandPalette) => {
            misc_overlays::handle_command_palette(state, output_tx, event);
            true
        }
        Some(OverlayId::Shortcuts) => {
            misc_overlays::handle_shortcuts(state, output_tx, event);
            true
        }
        Some(OverlayId::IsolationSwitcher) => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = isolation_switcher::handle_event(&mut ctx, event);
            true
        }
        Some(OverlayId::ProfileSwitcher) => {
            handle_profile_switcher(state, output_tx, event);
            true
        }
        Some(OverlayId::RulebookSwitcher) => {
            handle_rulebook_switcher(state, output_tx, event);
            true
        }
        Some(OverlayId::MessageAction) => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = message_action::handle_event(&mut ctx, event);
            true
        }
        Some(OverlayId::ModelSwitcher) => {
            model_switcher::handle_model_switcher(state, output_tx, event);
            true
        }
        Some(OverlayId::FileSearch) => {
            file_search::handle_file_search(state, output_tx, event);
            true
        }
        Some(OverlayId::Changeset) => {
            handle_changeset(state, output_tx, event);
            true
        }
        Some(OverlayId::ReviewPane) => {
            handle_review_pane(state, output_tx, event);
            true
        }
        Some(OverlayId::ShellPopup) => {
            crate::handlers::shell::handle_shell_key(state, output_tx, &event);
            true
        }
        Some(OverlayId::TaskTray) => {
            misc_overlays::handle_task_tray(state, output_tx, event);
            true
        }
        Some(OverlayId::ThemePicker) => {
            misc_overlays::handle_theme_picker(state, event);
            true
        }
        Some(OverlayId::SessionResume) => {
            session_resume::handle_session_resume(state, output_tx, event);
            true
        }
        Some(OverlayId::FilePicker) => {
            file_search::handle_file_picker(state, output_tx, event);
            true
        }
        Some(OverlayId::Elicitation) => {
            handle_elicitation(state, event);
            true
        }
        None => false,
    }
}

// ── Per-overlay handlers ─────────────────────────────────────────────────────

fn handle_elicitation(state: &mut AppState, event: InputEvent) {
    use crate::services::elicitation;
    match event {
        InputEvent::InputSubmitted => {
            let url = state
                .layout
                .elicitation
                .as_ref()
                .map(|p| p.url.clone());
            if let Some(url) = url {
                if let Err(e) = elicitation::launch_url(&url) {
                    tracing::warn!(
                        target: "vac_mcp_core::channel",
                        error = %e,
                        "elicitation: open::that failed",
                    );
                }
            }
            elicitation::accept_current(state);
            crate::overlay::close_overlay(state, OverlayId::Elicitation);
        }
        InputEvent::HandleEsc => {
            crate::services::elicitation::cancel_current(state);
            crate::overlay::close_overlay(state, OverlayId::Elicitation);
        }
        _ => {}
    }
}

fn handle_reject_reason(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::InputSubmitted => {
            let _ = approval::confirm_reject_current(&mut ctx);
        }
        InputEvent::HandleEsc => {
            ctx.state.execution.approvals.reject_reason_input = None;
            ctx.state.layout.overlay_manager.pop(OverlayId::RejectReason);
            let _ = approval::reject_current(&mut ctx);
        }
        InputEvent::InputChanged(c) => {
            let _ = approval::reason_input_push(&mut ctx, c);
        }
        InputEvent::InputBackspace => {
            let _ = approval::reason_input_pop(&mut ctx);
        }
        _ => {}
    }
}

fn handle_profile_switcher(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => {
            let _ = profile_switcher::close(&mut ctx);
        }
        InputEvent::InputChanged(c) => {
            let mut f = ctx.state.layout.switchers.profile_search.clone();
            f.push(c);
            let _ = profile_switcher::update_filter(&mut ctx, f);
        }
        InputEvent::InputBackspace => {
            let mut f = ctx.state.layout.switchers.profile_search.clone();
            f.pop();
            let _ = profile_switcher::update_filter(&mut ctx, f);
        }
        InputEvent::Up => {
            let _ = profile_switcher::select_prev(&mut ctx);
        }
        InputEvent::Down => {
            let _ = profile_switcher::select_next(&mut ctx);
        }
        InputEvent::InputSubmitted => {
            let _ = profile_switcher::submit_selected(&mut ctx);
        }
        _ => {}
    }
}

fn handle_rulebook_switcher(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => {
            let _ = rulebook_switcher::close(&mut ctx);
        }
        InputEvent::InputChanged(c) => {
            if c == ' ' {
                let _ = rulebook_switcher::toggle_selected(&mut ctx);
            } else {
                let mut f = ctx.state.layout.switchers.rulebook_search.clone();
                f.push(c);
                let _ = rulebook_switcher::update_filter(&mut ctx, f);
            }
        }
        InputEvent::InputBackspace => {
            let mut f = ctx.state.layout.switchers.rulebook_search.clone();
            f.pop();
            let _ = rulebook_switcher::update_filter(&mut ctx, f);
        }
        InputEvent::Up => {
            let _ = rulebook_switcher::select_prev(&mut ctx);
        }
        InputEvent::Down => {
            let _ = rulebook_switcher::select_next(&mut ctx);
        }
        InputEvent::InputSubmitted => {
            let _ = rulebook_switcher::submit_selected(&mut ctx);
        }
        _ => {}
    }
}

fn handle_changeset(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => {
            let _ = changeset_handler::close(&mut ctx);
        }
        InputEvent::Up => {
            let _ = changeset_handler::select_prev(&mut ctx);
        }
        InputEvent::Down => {
            let _ = changeset_handler::select_next(&mut ctx);
        }
        InputEvent::ScrollUp => {
            let _ = changeset_handler::scroll_up(&mut ctx);
        }
        InputEvent::ScrollDown => {
            let _ = changeset_handler::scroll_down(&mut ctx);
        }
        _ => {}
    }
}

fn handle_review_pane(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc | InputEvent::ReviewClose => {
            let _ = review_handler::close(&mut ctx);
        }
        InputEvent::ReviewOpen => {
            let _ = review_handler::open(&mut ctx);
        }
        InputEvent::ReviewUp | InputEvent::Up | InputEvent::ScrollUp => {
            let _ = review_handler::select_prev(&mut ctx);
        }
        InputEvent::ReviewDown | InputEvent::Down | InputEvent::ScrollDown => {
            let _ = review_handler::select_next(&mut ctx);
        }
        InputEvent::ReviewFilterInput(c) | InputEvent::InputChanged(c) => {
            let _ = review_handler::filter_push(&mut ctx, c);
        }
        InputEvent::ReviewFilterBackspace | InputEvent::InputBackspace => {
            let _ = review_handler::filter_pop(&mut ctx);
        }
        InputEvent::ReviewToggleDiff | InputEvent::InputSubmitted => {
            let _ = review_handler::toggle_diff(&mut ctx);
        }
        InputEvent::PageUp => {
            let _ = review_handler::scroll_up(&mut ctx, 10);
        }
        InputEvent::PageDown => {
            let _ = review_handler::scroll_down(&mut ctx, 10);
        }
        InputEvent::ReviewRevertSelected => {
            let _ = review_handler::revert_selected(&mut ctx);
        }
        InputEvent::ReviewRevertFiltered => {
            let _ = review_handler::revert_filtered(&mut ctx);
        }
        InputEvent::ReviewRevertAll | InputEvent::HandleCtrlZ => {
            let _ = review_handler::revert_all(&mut ctx);
        }
        InputEvent::ReviewOpenEditor => {
            let _ = review_handler::open_editor(&mut ctx);
        }
        _ => {}
    }
}
