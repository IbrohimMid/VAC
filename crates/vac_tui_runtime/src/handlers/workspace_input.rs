//! Workspace (non-workbench) input handlers — dispatched from the main router
//! for Input, Conversation, and Activity focus.

use crate::app::{AppState, InputEvent, OutputEvent, WorkspaceFocus};
use crate::handlers::input_commands::{dispatch_builtin_command, handle_paste_tray_key};
use crate::handlers::input_editor::message_at_row;
use crate::handlers::shell as shell_handler;
use tokio::sync::mpsc::Sender;

pub fn handle(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    match event {
        InputEvent::InputChanged(c) => handle_char(state, output_tx, c),
        InputEvent::InputChangedNewline => {
            if state.focus == WorkspaceFocus::Input {
                state.input.newline();
                notify_vil_expr_lint(state);
            }
        }
        InputEvent::InputBackspace => handle_backspace(state),
        InputEvent::InputDelete => {
            if state.focus == WorkspaceFocus::Input {
                state.input.delete();
                if state
                    .overlay_manager
                    .is_active(crate::overlay::OverlayId::HelperDropdown)
                {
                    crate::services::helper_dropdown::filter_helpers_sync(state);
                }
                notify_vil_expr_lint(state);
            }
        }
        InputEvent::InputClear => {
            if state.focus == WorkspaceFocus::Input {
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
                notify_vil_expr_lint(state);
            }
        }
        InputEvent::InputSubmitted => handle_submit(state, output_tx),
        InputEvent::HandlePaste(text) => handle_paste(state, text),
        InputEvent::HandleClipboardImagePaste => handle_image_paste(state),
        InputEvent::CursorLeft => {
            if state.focus == WorkspaceFocus::Input {
                state.input.move_cursor_left();
            }
        }
        InputEvent::CursorRight => {
            if state.focus == WorkspaceFocus::Input {
                state.input.move_cursor_right();
            }
        }
        InputEvent::InputCursorStart => {
            if state.focus == WorkspaceFocus::Input {
                state.input.move_cursor_start();
            }
        }
        InputEvent::InputCursorEnd => {
            if state.focus == WorkspaceFocus::Input {
                state.input.move_cursor_end();
            }
        }
        InputEvent::Up => handle_up(state, output_tx),
        InputEvent::Down => handle_down(state, output_tx),
        InputEvent::ScrollUp => match state.focus {
            WorkspaceFocus::Conversation => state.scroll = state.scroll.saturating_sub(1),
            WorkspaceFocus::Activity => {
                state.activity_scroll = state.activity_scroll.saturating_add(1)
            }
            _ => {}
        },
        InputEvent::ScrollDown => match state.focus {
            WorkspaceFocus::Conversation => state.scroll = state.scroll.saturating_add(1),
            WorkspaceFocus::Activity => {
                state.activity_scroll = state.activity_scroll.saturating_sub(1)
            }
            _ => {}
        },
        InputEvent::MouseRightClick(_col, row) => {
            if let Some(msg_id) = message_at_row(state, row) {
                state.message_action_popup_selected = 0;
                state.message_action_target_id = Some(msg_id);
                crate::overlay::open_overlay(state, crate::overlay::OverlayId::MessageAction);
            }
        }
        _ => {}
    }
}

fn handle_char(state: &mut AppState, output_tx: &Sender<OutputEvent>, c: char) {
    if state.focus == WorkspaceFocus::Input
        && state.input.is_empty()
        && !state.pending_pastes.is_empty()
        && handle_paste_tray_key(state, c)
    {
        return;
    }

    if state.focus != WorkspaceFocus::Input {
        return;
    }

    if c == '/' && state.input.lines.join("").trim().is_empty() {
        crate::overlay::open_overlay(state, crate::overlay::OverlayId::HelperDropdown);
        state.input.input(c);
        crate::services::helper_dropdown::filter_helpers_sync(state);
        state.command_palette.helper_selected = 0;
        state.command_palette.helper_scroll = 0;
    } else if state
        .overlay_manager
        .is_active(crate::overlay::OverlayId::HelperDropdown)
    {
        state.input.input(c);
        crate::services::helper_dropdown::filter_helpers_sync(state);
        state.command_palette.helper_selected = 0;
        state.command_palette.helper_scroll = 0;
    } else if c == '@' && !state.at_mention.trigger_active {
        crate::overlay::open_overlay(state, crate::overlay::OverlayId::AtDropdown);
        state.at_mention.query = String::new();
        state.at_mention.selected_idx = 0;
        if state.file_index.all_files.is_empty() {
            state.file_index.all_files = crate::services::build_file_index(&state.project_root);
        }
        state.at_mention.results = crate::services::fuzzy_search_files("", &state.file_index.all_files, 8);
        state.input.input(c);
    } else if state.at_mention.trigger_active {
        state.at_mention.query.push(c);
        state.at_mention.selected_idx = 0;
        state.at_mention.results =
            crate::services::fuzzy_search_files(&state.at_mention.query, &state.file_index.all_files, 8);
        state.input.input(c);
    } else {
        state.input.input(c);
    }

    let _ = output_tx; // suppress unused warning when no send is needed here
    notify_vil_expr_lint(state);
}

fn handle_backspace(state: &mut AppState) {
    if state.focus != WorkspaceFocus::Input {
        return;
    }
    if state
        .overlay_manager
        .is_active(crate::overlay::OverlayId::HelperDropdown)
    {
        state.input.backspace();
        crate::services::helper_dropdown::filter_helpers_sync(state);
        state.command_palette.helper_selected = 0;
        state.command_palette.helper_scroll = 0;
    } else if state.at_mention.trigger_active {
        if state.at_mention.query.is_empty() {
            state.at_mention.trigger_active = false;
            state.at_mention.results.clear();
            state.at_mention.selected_idx = 0;
        } else {
            state.at_mention.query.pop();
            state.at_mention.selected_idx = 0;
            state.at_mention.results =
                crate::services::fuzzy_search_files(&state.at_mention.query, &state.file_index.all_files, 8);
        }
        state.input.backspace();
    } else {
        state.input.backspace();
    }
    notify_vil_expr_lint(state);
}

fn handle_submit(state: &mut AppState, output_tx: &Sender<OutputEvent>) {
    if state.focus == WorkspaceFocus::Input
        && state.shell.session_store.popup_visible
        && state
            .shell
            .session_store
            .active()
            .and_then(|s| s.command.as_ref())
            .is_some()
    {
        let _ = shell_handler::handle_shell_key(state, output_tx, &InputEvent::InputSubmitted);
        return;
    }

    if state.focus != WorkspaceFocus::Input || state.input.is_empty() {
        return;
    }

    let msg = state.input.get_content();
    state.input.clear();
    notify_vil_expr_lint(state);

    // Prepend context chips to the message (PR-T7).
    let chip_prefix: String = if !state.context_chips.is_empty() {
        let mut prefix = String::new();
        for chip in &state.context_chips {
            prefix.push_str(&format!(
                "<context label=\"{}\">\n{}\n</context>\n\n",
                chip.label, chip.content
            ));
        }
        state.context_chips.clear();
        state.context_chip_cursor = None;
        prefix
    } else {
        String::new()
    };

    if msg.starts_with('/') {
        state.command_palette.recent_commands.add_command(msg.clone());
        let trimmed = msg.trim();
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let cmd_word = parts.next().unwrap_or(trimmed);
        let cmd_args = parts.next().map(|s| s.trim()).filter(|s| !s.is_empty());

        if !dispatch_builtin_command(state, output_tx, cmd_word, cmd_args) {
            let expanded = format!("{chip_prefix}{}", state.expand_pending_pastes(&msg));
            let image_parts = std::mem::take(&mut state.pending_image_parts);
            state
                .pending_user_messages
                .push_back(crate::app::PendingUserMessage::new(
                    expanded.clone(),
                    None,
                    image_parts,
                    expanded,
                ));
        }
    } else {
        let expanded = format!("{chip_prefix}{}", state.expand_pending_pastes(&msg));
        let image_parts = std::mem::take(&mut state.pending_image_parts);
        state
            .pending_user_messages
            .push_back(crate::app::PendingUserMessage::new(
                expanded.clone(),
                None,
                image_parts,
                expanded,
            ));
    }
}

fn handle_up(state: &mut AppState, output_tx: &Sender<OutputEvent>) {
    match state.focus {
        WorkspaceFocus::Input => {
            if shell_handler::handle_shell_key(state, output_tx, &InputEvent::Up) {
                state.input.move_cursor_end();
            } else {
                state.input.move_cursor_up();
            }
        }
        WorkspaceFocus::Conversation => state.scroll = state.scroll.saturating_sub(1),
        WorkspaceFocus::Activity => state.activity_scroll = state.activity_scroll.saturating_add(1),
        WorkspaceFocus::Workbench => {} // handled by workbench_input
    }
}

fn handle_down(state: &mut AppState, output_tx: &Sender<OutputEvent>) {
    match state.focus {
        WorkspaceFocus::Input => {
            if shell_handler::handle_shell_key(state, output_tx, &InputEvent::Down) {
                state.input.move_cursor_end();
            } else {
                state.input.move_cursor_down();
            }
        }
        WorkspaceFocus::Conversation => state.scroll = state.scroll.saturating_add(1),
        WorkspaceFocus::Activity => state.activity_scroll = state.activity_scroll.saturating_sub(1),
        WorkspaceFocus::Workbench => {} // handled by workbench_input
    }
}

fn handle_paste(state: &mut AppState, text: String) {
    use crate::services::clipboard_paste::{
        PastedItem, PastedKind, extract_file_paths_from_text, is_long_paste, make_paste_id,
        text_placeholder,
    };
    let paths = extract_file_paths_from_text(&text);
    if !paths.is_empty() {
        for path in paths {
            state.input.insert_str(&path.to_string_lossy());
            state.input.input(' ');
        }
    } else if is_long_paste(&text) {
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
        for c in text.chars() {
            if c == '\n' {
                state.input.newline();
            } else if c != '\r' {
                state.input.input(c);
            }
        }
    }
    notify_vil_expr_lint(state);
}

/// Notify the vil-expr linter of an input buffer change (PR-T12.1).
fn notify_vil_expr_lint(state: &mut AppState) {
    let text = state.input.lines.join("\n");
    state
        .vil_expr_lint
        .on_input_changed(&text, std::time::Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppStateOptions;
    use std::time::{Duration, Instant};

    fn make_test_state() -> AppState {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: None,
            checkpoint_path: None,
            project_root: dir.path().to_path_buf(),
        });
        state.focus = WorkspaceFocus::Input;
        // Leak the tempdir so it outlives the state (test-only).
        std::mem::forget(dir);
        state
    }

    #[test]
    fn input_changed_updates_vil_expr_lint_state() {
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_test_state();

        // Type "vil-expr: foo" character by character.
        for c in "vil-expr: foo".chars() {
            handle_char(&mut state, &tx, c);
        }

        // After typing, lint state should have a pending payload.
        assert_eq!(
            state.vil_expr_lint.pending_payload.as_deref(),
            Some("foo"),
            "expected pending payload 'foo' after typing 'vil-expr: foo'"
        );
        assert!(
            state.vil_expr_lint.deadline.is_some(),
            "deadline should be set after input change"
        );

        // Clear input → lint state should reset.
        state.input.clear();
        notify_vil_expr_lint(&mut state);
        assert!(
            state.vil_expr_lint.pending_payload.is_none(),
            "payload should be cleared when input has no vil-expr: prefix"
        );
        assert!(state.vil_expr_lint.is_clean());
    }

    #[test]
    fn tick_emits_redraw_request_after_debounce() {
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_test_state();
        let symbols = vil_expr::SymbolTable::new();

        // Simulate typing "vil-expr: unknown_ident".
        for c in "vil-expr: unknown_ident".chars() {
            handle_char(&mut state, &tx, c);
        }

        let now = Instant::now();
        // Override deadline to a known value for deterministic testing.
        state.vil_expr_lint.deadline = Some(now + Duration::from_millis(200));

        // Before debounce expires → tick should not fire.
        assert!(
            !state
                .vil_expr_lint
                .tick(&symbols, now + Duration::from_millis(100)),
            "tick should NOT fire before debounce window"
        );
        assert!(
            state.vil_expr_lint.issues().is_empty(),
            "issues should be empty before lint runs"
        );

        // After debounce expires → tick should fire and return true (= redraw needed).
        assert!(
            state
                .vil_expr_lint
                .tick(&symbols, now + Duration::from_millis(300)),
            "tick should fire after debounce window — return true signals redraw"
        );
        assert!(
            !state.vil_expr_lint.issues().is_empty(),
            "after tick, issues should contain the unknown identifier error"
        );
    }

    #[test]
    fn alt_h_on_identifier_opens_popup() {
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_test_state();

        // Type a vil-expr payload so there is a pending payload.
        for c in "vil-expr: some_ident".chars() {
            handle_char(&mut state, &tx, c);
        }

        // Dispatch VilExprTypeHelp (Alt+H).
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::VilExprTypeHelp);

        // Should have a toast with the stub message.
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("type inference coming soon")),
            "Alt+H with active payload should show stub type-info toast, got: {:?}",
            state.toasts.iter().map(|t| &t.message).collect::<Vec<_>>()
        );
    }
}

fn handle_image_paste(state: &mut AppState) {
    #[cfg(not(target_os = "android"))]
    {
        const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

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
