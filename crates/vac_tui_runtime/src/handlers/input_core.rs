use crate::app::{AppState, InputEvent, OutputEvent};
use crate::handlers::HandlerContext;
use crate::handlers::input_commands::{dispatch_builtin_command, handle_paste_tray_key};
use crate::handlers::input_editor::{message_at_row, plan_open_editor, plan_write_status};
use crate::handlers::input_popup;
use crate::handlers::{
    approval, changeset as changeset_handler, file_search, model_switcher, profile_switcher,
    review as review_handler, rulebook_switcher, shell as shell_handler,
};
use tokio::sync::mpsc::Sender;

/// Returns true if any popup/overlay is currently active and should intercept input.
fn any_popup_active(state: &AppState) -> bool {
    state.reject_reason_input.is_some()
        || state.show_ask_user_popup
        || state.plan.review_open
        || state.show_file_changes_popup
        || (state.show_helper_dropdown && state.focus == crate::app::WorkspaceFocus::Input)
        || (state.at_trigger_active && state.focus == crate::app::WorkspaceFocus::Input)
        || state.show_command_palette
        || state.show_shortcuts
        || state.show_isolation_switcher
        || state.show_profile_switcher
        || state.show_rulebook_switcher
        || state.show_message_action_popup
        || state.show_model_switcher
        || state.show_file_search
        || state.show_changeset
        || (state.review.open
            && state.focus == crate::app::WorkspaceFocus::Workbench
            && state.workbench_tab == crate::app::WorkbenchTab::Review)
}

/// Handle input events from user.
/// First dispatches to popup/overlay handlers, then falls through to normal handling.
pub fn handle_input_event(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    if any_popup_active(state) {
        if input_popup::dispatch_popup_event(state, output_tx, event) {
            return;
        }
        // If popup didn't consume the event, the event is gone (moved).
        // This matches original behavior where popups swallow all events.
        return;
    }

    // Normal input handling
    match event {
        InputEvent::ShowProfileSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = profile_switcher::open(&mut ctx);
        }
        InputEvent::ShowIsolationSwitcher => {
            state.show_isolation_switcher = true;
            state.isolation_switcher_selected = 0;
        }
        InputEvent::ShowRulebookSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = rulebook_switcher::open(&mut ctx);
        }
        InputEvent::ShowMessageActionPopup => {
            state.show_message_action_popup = true;
            state.message_action_popup_selected = 0;
            // Target the last user message if any
            state.message_action_target_id = state
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.id);
        }
        InputEvent::ToggleSidePanel => {
            state.side_panel_visible = !state.side_panel_visible;
            if !state.side_panel_visible {
                state.side_panel_row_areas.clear();
            }
        }
        InputEvent::MouseDragStart(col, row) => {
            // Banner click regions only react while the banner is still active.
            // Without this guard a click in the stale rect (between TTL expiry
            // and next render) would dismiss or dispatch phantom actions.
            let banner_active = state
                .banner_message
                .as_ref()
                .is_some_and(|m| !m.is_expired());
            if banner_active {
                if let Some(rect) = state.banner_dismiss_region {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        state.banner_message = None;
                        state.banner_click_regions.clear();
                        state.banner_dismiss_region = None;
                        return;
                    }
                }
                let mut banner_action: Option<String> = None;
                for (action, rect) in &state.banner_click_regions {
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
                    state.banner_message = None;
                    state.banner_click_regions.clear();
                    state.banner_dismiss_region = None;
                    let _ = output_tx.try_send(OutputEvent::UserMessage(
                        action,
                        None,
                        Vec::new(),
                        None,
                    ));
                    return;
                }
            } else if state.banner_message.is_some() {
                // TTL elapsed but not yet re-rendered — clear stale regions proactively.
                state.banner_message = None;
                state.banner_click_regions.clear();
                state.banner_dismiss_region = None;
            }

            let mut clicked_section = None;
            if state.side_panel_visible {
                for (sec, rect) in &state.side_panel_header_areas {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        clicked_section = Some(*sec);
                        break;
                    }
                }
            }

            if let Some(sec) = clicked_section {
                if state.side_panel_section_collapsed.contains(&sec) {
                    state.side_panel_section_collapsed.remove(&sec);
                } else {
                    state.side_panel_section_collapsed.insert(sec);
                }
            } else {
                // Check per-row click areas in side panel
                let mut handled = false;
                if state.side_panel_visible {
                    for (action, rect) in &state.side_panel_row_areas {
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
                                    state.focus = crate::app::WorkspaceFocus::Workbench;
                                    state.workbench_tab = crate::app::WorkbenchTab::Runtime;
                                }
                                crate::app::SidePanelRowAction::JumpToVilIssue(path) => {
                                    state.review.selected_path = Some(path);
                                    state.review.selected_idx = 0;
                                    state.focus = crate::app::WorkspaceFocus::Workbench;
                                    state.workbench_tab = crate::app::WorkbenchTab::Review;
                                }
                            }
                            handled = true;
                            break;
                        }
                    }
                }
                if !handled {
                    if row == 0 {
                        let term_width = crossterm::terminal::size().map(|s| s.0).unwrap_or(80);
                        if col > term_width.saturating_sub(40) {
                            state.side_panel_visible = !state.side_panel_visible;
                        }
                    } else {
                        crate::services::text_selection::handle_drag_start(state, col, row);
                    }
                }
            }
        }
        InputEvent::MouseDrag(col, row) => {
            crate::services::text_selection::handle_drag(state, col, row);
        }
        InputEvent::MouseDragEnd(col, row) => {
            crate::services::text_selection::handle_drag_end(state, col, row);
        }
        InputEvent::MouseRightClick(_col, row) => {
            // Resolve which message the click landed on and open popup for it
            if let Some(msg_id) = message_at_row(state, row) {
                state.show_message_action_popup = true;
                state.message_action_popup_selected = 0;
                state.message_action_target_id = Some(msg_id);
            }
        }
        InputEvent::Tab => {
            state.focus = state.focus.next();
        }
        InputEvent::WorkbenchNextTab => {
            if state.focus != crate::app::WorkspaceFocus::Workbench {
                state.focus = crate::app::WorkspaceFocus::Workbench;
            }
            state.workbench_tab = state.workbench_tab.next();
            if state.workbench_tab == crate::app::WorkbenchTab::Agents {
                let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
                let _ = output_tx.try_send(OutputEvent::LoadAgentState);
            } else if state.workbench_tab == crate::app::WorkbenchTab::Runtime {
                let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
            }
        }
        InputEvent::InputChanged(c) => {
            // Unit 5 (Wave 3.1): paste-tray keybindings are live when the
            // input is empty and there are pending pastes. Typing anything
            // into the input re-routes the key to normal handling.
            if state.focus == crate::app::WorkspaceFocus::Input
                && state.input.is_empty()
                && !state.pending_pastes.is_empty()
                && handle_paste_tray_key(state, c)
            {
                return;
            }
            match state.focus {
                crate::app::WorkspaceFocus::Input => {
                    if c == '/' && state.input.lines.join("").trim().is_empty() {
                        state.show_helper_dropdown = true;
                        state.input.input(c);
                        crate::services::helper_dropdown::filter_helpers_sync(state);
                        state.helper_selected = 0;
                        state.helper_scroll = 0;
                    } else if state.show_helper_dropdown {
                        state.input.input(c);
                        crate::services::helper_dropdown::filter_helpers_sync(state);
                        state.helper_selected = 0;
                        state.helper_scroll = 0;
                    } else if c == '@' && !state.at_trigger_active {
                        // Activate inline @ picker
                        state.at_trigger_active = true;
                        state.at_query = String::new();
                        state.at_selected_idx = 0;
                        if state.all_files.is_empty() {
                            state.all_files =
                                crate::services::build_file_index(&state.project_root);
                        }
                        state.at_results =
                            crate::services::fuzzy_search_files("", &state.all_files, 8);
                        state.input.input(c);
                    } else if state.at_trigger_active {
                        // Update query with new char
                        state.at_query.push(c);
                        state.at_selected_idx = 0;
                        state.at_results = crate::services::fuzzy_search_files(
                            &state.at_query,
                            &state.all_files,
                            8,
                        );
                        state.input.input(c);
                    } else {
                        state.input.input(c);
                    }
                }
                crate::app::WorkspaceFocus::Workbench => match state.workbench_tab {
                    crate::app::WorkbenchTab::Approvals => match c {
                        'a' => {
                            let mut ctx = HandlerContext::new(state, output_tx);
                            let _ = approval::approve_current(&mut ctx);
                        }
                        'r' => {
                            let mut ctx = HandlerContext::new(state, output_tx);
                            let _ = approval::begin_reject_current(&mut ctx);
                        }
                        _ => {}
                    },
                    crate::app::WorkbenchTab::Review => {}
                    crate::app::WorkbenchTab::Vil => {
                        use crate::handlers::vil_workbench;
                        let mut ctx = HandlerContext::new(state, output_tx);
                        match c {
                            'r' | 'R' => {
                                let _ = vil_workbench::run_repair(&mut ctx);
                            }
                            'a' | 'A' => {
                                let _ = vil_workbench::run_audit(&mut ctx);
                            }
                            'd' | 'D' => {
                                let _ = vil_workbench::run_ir_diff(&mut ctx);
                            }
                            'o' | 'O' => {
                                let _ = vil_workbench::open_in_editor(&mut ctx);
                            }
                            'b' | 'B' => {
                                let _ = vil_workbench::run_batch_campaign(&mut ctx);
                            }
                            _ => {}
                        }
                    }
                    crate::app::WorkbenchTab::Plan => match c {
                        'a' => {
                            plan_write_status(state, crate::services::plan::PlanStatus::Approved);
                            state.add_assistant_message("Plan approved.".to_string());
                        }
                        'r' => {
                            plan_write_status(state, crate::services::plan::PlanStatus::Drafting);
                            state.add_assistant_message("Plan marked for revision.".to_string());
                        }
                        'e' => {
                            plan_open_editor(state);
                        }
                        _ => {}
                    },
                    crate::app::WorkbenchTab::Sessions => {
                        if c == 'r' {
                            if let Some(sel) =
                                state.sessions.get(state.sessions_selected_idx).cloned()
                            {
                                if sel.has_checkpoint {
                                    let _ = output_tx
                                        .try_send(OutputEvent::ResumeSession(sel.id.clone()));
                                    state.push_activity(
                                        crate::app::ActivityKind::Session,
                                        format!(
                                            "Resuming checkpoint: {}",
                                            &sel.id[..8.min(sel.id.len())]
                                        ),
                                    );
                                } else {
                                    state.toasts.push(crate::services::Toast::info(
                                        "No checkpoint available for this session".to_string(),
                                    ));
                                }
                            }
                        } else if c == 'd' {
                            if let Some(sel) =
                                state.sessions.get(state.sessions_selected_idx).cloned()
                            {
                                match uuid::Uuid::parse_str(&sel.id) {
                                    Ok(session_id) => {
                                        let report = vac_session_control::cleanup_session(
                                            &state.project_root,
                                            session_id,
                                        );
                                        if report.snapshot_removed
                                            || report.checkpoint_removed
                                            || report.approvals_removed > 0
                                        {
                                            state.toasts.push(crate::services::Toast::success(
                                                format!(
                                                    "Cleaned session artifacts: snapshot {}, checkpoint {}, approvals {}",
                                                    report.snapshot_removed,
                                                    report.checkpoint_removed,
                                                    report.approvals_removed
                                                ),
                                            ));
                                        } else {
                                            state.toasts.push(crate::services::Toast::info(
                                                "No session artifacts found to clean".to_string(),
                                            ));
                                        }
                                        if !report.errors.is_empty() {
                                            state.toasts.push(crate::services::Toast::error(
                                                format!(
                                                    "Session cleanup completed with {} error(s)",
                                                    report.errors.len()
                                                ),
                                            ));
                                        }
                                        let _ = output_tx.try_send(OutputEvent::ListSessions);
                                    }
                                    Err(e) => {
                                        state.toasts.push(crate::services::Toast::error(format!(
                                            "Invalid session id: {e}"
                                        )));
                                    }
                                }
                            }
                        }
                    }
                    crate::app::WorkbenchTab::Agents => {
                        if c == 'r' {
                            let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
                            let _ = output_tx.try_send(OutputEvent::LoadAgentState);
                        }
                    }
                    crate::app::WorkbenchTab::Runtime => match c {
                        'r' => {
                            let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                            let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
                        }
                        'c' => {
                            if let Some(job) = state.runtime.jobs.get(state.runtime.selected_idx) {
                                let _ = output_tx.try_send(OutputEvent::CancelRuntimeJob(job.id));
                            }
                        }
                        't' => {
                            if let Some(job) = state.runtime.jobs.get(state.runtime.selected_idx) {
                                let _ = output_tx.try_send(OutputEvent::RetryRuntimeJob(job.id));
                            }
                        }
                        _ => {}
                    },
                },
                _ => {}
            }
        }
        InputEvent::InputChangedNewline => {
            if state.focus == crate::app::WorkspaceFocus::Input {
                state.input.newline();
            }
        }
        InputEvent::InputBackspace => {
            if state.focus == crate::app::WorkspaceFocus::Input {
                if state.show_helper_dropdown {
                    state.input.backspace();
                    crate::services::helper_dropdown::filter_helpers_sync(state);
                    state.helper_selected = 0;
                    state.helper_scroll = 0;
                } else if state.at_trigger_active {
                    if state.at_query.is_empty() {
                        state.at_trigger_active = false;
                        state.at_results.clear();
                        state.at_selected_idx = 0;
                    } else {
                        state.at_query.pop();
                        state.at_selected_idx = 0;
                        state.at_results = crate::services::fuzzy_search_files(
                            &state.at_query,
                            &state.all_files,
                            8,
                        );
                    }
                    state.input.backspace();
                } else {
                    state.input.backspace();
                }
            }
        }
        InputEvent::InputDelete => {
            if state.focus == crate::app::WorkspaceFocus::Input {
                state.input.delete();
                if state.show_helper_dropdown {
                    crate::services::helper_dropdown::filter_helpers_sync(state);
                }
            }
        }
        InputEvent::InputClear => {
            if state.focus == crate::app::WorkspaceFocus::Input {
                let had_pastes = !state.pending_pastes.is_empty();
                let had_images = !state.pending_image_parts.is_empty();
                state.input.clear();
                state.pending_pastes.clear();
                state.pending_image_parts.clear();
                if had_pastes || had_images {
                    state.toasts.push(crate::services::Toast::info(
                        "Input and pending attachments cleared.".to_string(),
                    ));
                }
            }
        }
        InputEvent::InputSubmitted => {
            if state.focus == crate::app::WorkspaceFocus::Input
                && state.shell.session_store.popup_visible
                && state
                    .shell
                    .session_store
                    .active()
                    .and_then(|session| session.command.as_ref())
                    .is_some()
            {
                let _ =
                    shell_handler::handle_shell_key(state, output_tx, &InputEvent::InputSubmitted);
                return;
            }

            if state.focus == crate::app::WorkspaceFocus::Workbench
                && state.workbench_tab == crate::app::WorkbenchTab::Approvals
            {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = approval::approve_current(&mut ctx);
                return;
            }

            if state.focus == crate::app::WorkspaceFocus::Workbench
                && state.workbench_tab == crate::app::WorkbenchTab::Sessions
            {
                if let Some(sel) = state.sessions.get(state.sessions_selected_idx).cloned() {
                    let _ = output_tx.try_send(OutputEvent::SwitchToSession(sel.id));
                    state.push_activity(crate::app::ActivityKind::Session, "Switch session");
                }
                return;
            }

            if state.focus != crate::app::WorkspaceFocus::Input || state.input.is_empty() {
                return;
            }

            if !state.input.is_empty() {
                let msg = state.input.get_content();
                state.input.clear();

                if msg.starts_with('/') {
                    state.recent_commands.add_command(msg.clone());
                    let trimmed = msg.trim();
                    let mut parts = trimmed.splitn(2, char::is_whitespace);
                    let cmd_word = parts.next().unwrap_or(trimmed);
                    let cmd_args = parts.next().map(|s| s.trim()).filter(|s| !s.is_empty());

                    if !dispatch_builtin_command(state, output_tx, cmd_word, cmd_args) {
                        let expanded = state.expand_pending_pastes(&msg);
                        let parts = std::mem::take(&mut state.pending_image_parts);
                        state
                            .pending_user_messages
                            .push_back(crate::app::PendingUserMessage::new(
                                expanded.clone(),
                                None,
                                parts,
                                expanded,
                            ));
                    }
                } else {
                    let expanded = state.expand_pending_pastes(&msg);
                    let parts = std::mem::take(&mut state.pending_image_parts);
                    state
                        .pending_user_messages
                        .push_back(crate::app::PendingUserMessage::new(
                            expanded.clone(),
                            None,
                            parts,
                            expanded,
                        ));
                }
            }
        }
        InputEvent::HandlePaste(text) => {
            use crate::services::clipboard_paste::{
                PastedItem, PastedKind, extract_file_paths_from_text, is_long_paste, make_paste_id,
                text_placeholder,
            };
            // First try to extract file paths
            let paths = extract_file_paths_from_text(&text);
            if !paths.is_empty() {
                for path in paths {
                    state.input.insert_str(&path.to_string_lossy());
                    state.input.input(' ');
                }
            } else if is_long_paste(&text) {
                // Long paste → collapse into a placeholder token; full content
                // is expanded at submit time via `expand_pending_pastes`.
                state.paste_counter += 1;
                let id = make_paste_id(state.paste_counter);
                let char_count = text.chars().count();
                let line_count = text.chars().filter(|c| *c == '\n').count() + 1;
                let placeholder = text_placeholder(&id, char_count, line_count);
                state.input.insert_str(&placeholder);
                state.input.input(' ');
                state.pending_pastes.push(PastedItem {
                    id,
                    placeholder,
                    kind: PastedKind::Text {
                        content: text,
                        line_count,
                        char_count,
                    },
                });
            } else {
                // Short paste → inline directly.
                for c in text.chars() {
                    if c == '\n' {
                        state.input.newline();
                    } else if c != '\r' {
                        state.input.input(c);
                    }
                }
            }
        }
        InputEvent::HandleClipboardImagePaste => {
            #[cfg(not(target_os = "android"))]
            {
                const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024; // 10MB limit

                match crate::services::clipboard_paste::paste_image_to_temp_png() {
                    Ok((path, info)) => {
                        if let Ok(bytes) = std::fs::read(&path) {
                            if bytes.len() > MAX_IMAGE_BYTES {
                                log::warn!(
                                    "Image too large ({} bytes), max {} bytes",
                                    bytes.len(),
                                    MAX_IMAGE_BYTES
                                );
                                state.input.insert_str("[image too large, max 10MB] ");
                            } else {
                                use crate::services::clipboard_paste::{
                                    PastedItem, PastedKind, image_placeholder, make_paste_id,
                                };
                                use base64::Engine as _;
                                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                let media_type = match path.extension().and_then(|e| e.to_str()) {
                                    Some("png") => "image/png",
                                    Some("jpg") | Some("jpeg") => "image/jpeg",
                                    Some("gif") => "image/gif",
                                    Some("webp") => "image/webp",
                                    _ => "image/png",
                                };
                                let part = crate::types::ContentPart {
                                    r#type: "image".to_string(),
                                    text: None,
                                    image_url: Some(crate::types::ImageUrl {
                                        url: format!("data:{};base64,{}", media_type, b64),
                                    }),
                                };
                                state.pending_image_parts.push(part);
                                state.paste_counter += 1;
                                let id = make_paste_id(state.paste_counter);
                                let placeholder = image_placeholder(&id, info.width, info.height);
                                state.input.insert_str(&placeholder);
                                state.input.input(' ');
                                state.pending_pastes.push(PastedItem {
                                    id,
                                    placeholder,
                                    kind: PastedKind::Image {
                                        width: info.width,
                                        height: info.height,
                                        byte_count: bytes.len(),
                                    },
                                });
                            }
                        } else {
                            state.input.insert_str(&path.to_string_lossy());
                            state.input.input(' ');
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to paste image from clipboard: {}", e);
                    }
                }
            }
        }
        InputEvent::CursorLeft => {
            if state.focus == crate::app::WorkspaceFocus::Input {
                state.input.move_cursor_left();
            }
        }
        InputEvent::CursorRight => {
            if state.focus == crate::app::WorkspaceFocus::Input {
                state.input.move_cursor_right();
            }
        }
        InputEvent::Up => match state.focus {
            crate::app::WorkspaceFocus::Input => {
                if shell_handler::handle_shell_key(state, output_tx, &InputEvent::Up) {
                    state.input.move_cursor_end();
                } else {
                    state.input.move_cursor_up();
                }
            }
            crate::app::WorkspaceFocus::Conversation => {
                state.scroll = state.scroll.saturating_sub(1);
            }
            crate::app::WorkspaceFocus::Activity => {
                state.activity_scroll = state.activity_scroll.saturating_add(1);
            }
            crate::app::WorkspaceFocus::Workbench => match state.workbench_tab {
                crate::app::WorkbenchTab::Approvals => {
                    state.approval_selected_idx = state.approval_selected_idx.saturating_sub(1);
                    state.approval_detail_scroll = 0;
                }
                crate::app::WorkbenchTab::Sessions => {
                    state.sessions_selected_idx = state.sessions_selected_idx.saturating_sub(1);
                }
                crate::app::WorkbenchTab::Agents => {
                    state.runtime.agent_selected = state.runtime.agent_selected.saturating_sub(1);
                    state.runtime.agent_detail_scroll = 0;
                }
                crate::app::WorkbenchTab::Runtime => {
                    state.runtime.selected_idx = state.runtime.selected_idx.saturating_sub(1);
                    state.runtime.detail_scroll = 0;
                }
                crate::app::WorkbenchTab::Review => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::select_prev(&mut ctx);
                }
                crate::app::WorkbenchTab::Plan => {}
                crate::app::WorkbenchTab::Vil => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = crate::handlers::vil_workbench::select_prev(&mut ctx);
                }
            },
        },
        InputEvent::Down => match state.focus {
            crate::app::WorkspaceFocus::Input => {
                if shell_handler::handle_shell_key(state, output_tx, &InputEvent::Down) {
                    state.input.move_cursor_end();
                } else {
                    state.input.move_cursor_down();
                }
            }
            crate::app::WorkspaceFocus::Conversation => {
                state.scroll = state.scroll.saturating_add(1);
            }
            crate::app::WorkspaceFocus::Activity => {
                state.activity_scroll = state.activity_scroll.saturating_sub(1);
            }
            crate::app::WorkspaceFocus::Workbench => match state.workbench_tab {
                crate::app::WorkbenchTab::Approvals => {
                    if state.approval_selected_idx + 1 < state.pending_approvals.len() {
                        state.approval_selected_idx += 1;
                        state.approval_detail_scroll = 0;
                    }
                }
                crate::app::WorkbenchTab::Sessions => {
                    if state.sessions_selected_idx + 1 < state.sessions.len() {
                        state.sessions_selected_idx += 1;
                    }
                }
                crate::app::WorkbenchTab::Agents => {
                    if state.runtime.agent_selected + 1 < state.runtime.agent_tasks.len() {
                        state.runtime.agent_selected += 1;
                        state.runtime.agent_detail_scroll = 0;
                    }
                }
                crate::app::WorkbenchTab::Runtime => {
                    if state.runtime.selected_idx + 1 < state.runtime.jobs.len() {
                        state.runtime.selected_idx += 1;
                        state.runtime.detail_scroll = 0;
                    }
                }
                crate::app::WorkbenchTab::Review => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::select_next(&mut ctx);
                }
                crate::app::WorkbenchTab::Plan => {}
                crate::app::WorkbenchTab::Vil => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = crate::handlers::vil_workbench::select_next(&mut ctx);
                }
            },
        },
        InputEvent::InputCursorStart => {
            if state.focus == crate::app::WorkspaceFocus::Input {
                state.input.move_cursor_start();
            }
        }
        InputEvent::InputCursorEnd => {
            if state.focus == crate::app::WorkspaceFocus::Input {
                state.input.move_cursor_end();
            }
        }
        InputEvent::AttemptQuit => {
            state.cancel_requested = true;
        }
        InputEvent::HandleEsc => {
            if state.show_model_switcher {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = model_switcher::close(&mut ctx);
            } else if state.show_file_search {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = file_search::close(&mut ctx);
            } else if state.show_changeset {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = changeset_handler::close(&mut ctx);
            } else if state.shell.session_store.popup_visible
                && state
                    .shell
                    .session_store
                    .active()
                    .and_then(|session| session.command.as_ref())
                    .is_some()
            {
                shell_handler::background(state);
            } else if state.is_streaming {
                let _ = output_tx.try_send(OutputEvent::CancelStream);
                state.is_streaming = false;
            } else if state.focus == crate::app::WorkspaceFocus::Workbench
                && state.workbench_tab == crate::app::WorkbenchTab::Review
            {
                state.review.open = false;
                state.review.diff = None;
            }
        }
        InputEvent::HandleCtrlZ => {
            if state
                .shell
                .session_store
                .active()
                .and_then(|session| session.command.as_ref())
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
        InputEvent::BackgroundShell => {
            shell_handler::background(state);
        }
        InputEvent::FocusShell => {
            shell_handler::foreground(state);
        }
        InputEvent::ShellKill => {
            shell_handler::kill(state);
            if let Some(session) = state.shell.session_store.active_mut() {
                session.waiting_for_input = false;
            }
        }
        InputEvent::AutoApproveCurrentTool => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::approve_current(&mut ctx);
        }
        InputEvent::RejectCurrentTool => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::begin_reject_current(&mut ctx);
        }
        InputEvent::ApproveAll => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::approve_all(&mut ctx);
        }
        InputEvent::RejectAll => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::confirm_reject_all(&mut ctx);
        }
        InputEvent::ScrollUp => match state.focus {
            crate::app::WorkspaceFocus::Conversation => {
                state.scroll = state.scroll.saturating_sub(1);
            }
            crate::app::WorkspaceFocus::Activity => {
                state.activity_scroll = state.activity_scroll.saturating_add(1);
            }
            _ => {}
        },
        InputEvent::ScrollDown => match state.focus {
            crate::app::WorkspaceFocus::Conversation => {
                state.scroll = state.scroll.saturating_add(1);
            }
            crate::app::WorkspaceFocus::Activity => {
                state.activity_scroll = state.activity_scroll.saturating_sub(1);
            }
            _ => {}
        },
        InputEvent::ShowCommandPalette => {
            state.show_command_palette = true;
            state.command_palette_input.clear();
            state.command_palette_selected = 0;
        }
        InputEvent::ShowModelSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = model_switcher::open(&mut ctx);
        }
        InputEvent::ShowFileSearch => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = file_search::open(&mut ctx);
        }
        InputEvent::ShowChangeset => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = changeset_handler::open(&mut ctx);
        }
        InputEvent::ShowShortcuts => {
            state.show_shortcuts = true;
        }
        InputEvent::ToggleAutoApprove => {
            state.auto_approve = !state.auto_approve;
            if state.auto_approve {
                state.add_assistant_message(
                    "Permission Mode: AUTO-APPROVE (Low-risk tools will run without confirmation)"
                        .to_string(),
                );
            } else {
                state.add_assistant_message(
                    "Permission Mode: PROMPT (You will be prompted for tool execution)".to_string(),
                );
            }
        }
        InputEvent::RequestSessionList => {
            let _ = output_tx.try_send(OutputEvent::ListSessions);
        }
        InputEvent::NewSession => {
            let _ = output_tx.try_send(OutputEvent::NewSession);
        }
        InputEvent::ReviewOpen => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = review_handler::open(&mut ctx);
        }
        _ => {}
    }
}
