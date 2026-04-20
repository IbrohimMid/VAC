use crate::app::{AppState, OutputEvent};
use crate::handlers::HandlerContext;
use crate::handlers::{
    changeset as changeset_handler, file_search, model_switcher, profile_switcher,
    review as review_handler, rulebook_switcher, shell as shell_handler,
};
use crate::services::commands::{Command, CommandAction};
use crate::update::*;

pub fn handle_paste_tray_key(state: &mut AppState, c: char) -> bool {
    use crate::services::clipboard_paste as cp;
    let len = state.pending_pastes.len();
    match c {
        'j' if !state.pending_paste_reorder_mode => {
            state.pending_paste_selected = cp::select_next(state.pending_paste_selected, len);
            true
        }
        'k' if !state.pending_paste_reorder_mode => {
            state.pending_paste_selected = cp::select_prev(state.pending_paste_selected, len);
            true
        }
        'J' if state.pending_paste_reorder_mode => {
            state.pending_paste_selected =
                cp::swap_with_next(&mut state.pending_pastes, state.pending_paste_selected);
            true
        }
        'K' if state.pending_paste_reorder_mode => {
            state.pending_paste_selected =
                cp::swap_with_prev(&mut state.pending_pastes, state.pending_paste_selected);
            true
        }
        'd' | 'x' => {
            let sel = state.pending_paste_selected.min(len.saturating_sub(1));
            if sel < state.pending_pastes.len() {
                let placeholder = state.pending_pastes[sel].placeholder.clone();
                if !state.input.is_empty() {
                    let stripped = state.input.get_content().replace(&placeholder, "");
                    state.input.clear();
                    state.input.insert_str(&stripped);
                }
                state.pending_paste_selected = cp::remove_at(&mut state.pending_pastes, sel);
                if state.pending_pastes.is_empty() {
                    state.pending_paste_reorder_mode = false;
                    state.pending_paste_selected = 0;
                }
            }
            true
        }
        'r' => {
            state.pending_paste_reorder_mode = !state.pending_paste_reorder_mode;
            true
        }
        _ => false,
    }
}

pub fn execute_shortcuts_command(
    state: &mut AppState,
    output_tx: &tokio::sync::mpsc::Sender<OutputEvent>,
    command: &Command,
) -> bool {
    match &command.action {
        CommandAction::OpenProfileSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = profile_switcher::open(&mut ctx);
            true
        }
        CommandAction::OpenRulebookSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = rulebook_switcher::open(&mut ctx);
            true
        }
        CommandAction::OpenSessions => {
            state.shortcuts_mode = crate::app::ShortcutsPopupMode::Sessions;
            state.shortcuts_scroll = 0;
            state.command_palette_input.clear();
            state.command_palette_selected = 0;
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::Shortcuts);
            true
        }
        CommandAction::OpenShortcuts => {
            state.shortcuts_mode = crate::app::ShortcutsPopupMode::Shortcuts;
            state.shortcuts_scroll = 0;
            state.command_palette_input.clear();
            state.command_palette_selected = 0;
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::Shortcuts);
            true
        }
        CommandAction::ClearScreen => {
            state.messages.clear();
            state
                .messages
                .extend(crate::services::helper_block::welcome_messages(None, state));
            true
        }
        CommandAction::ToggleAutoApprove => {
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
            true
        }
        CommandAction::Quit => {
            state.cancel_requested = true;
            true
        }
        CommandAction::InsertSlashCommand(s) => dispatch_builtin_command(state, output_tx, s, None),
    }
}

pub fn dispatch_builtin_command(
    state: &mut AppState,
    output_tx: &tokio::sync::mpsc::Sender<OutputEvent>,
    cmd_word: &str,
    cmd_args: Option<&str>,
) -> bool {
    let trimmed = if let Some(args) = cmd_args {
        format!("{} {}", cmd_word, args)
    } else {
        cmd_word.to_string()
    };
    if let Some(cmd) = state
        .commands
        .iter()
        .find(|c| c.command == cmd_word)
        .cloned()
    {
        match cmd.source {
            crate::app::CommandSource::BuiltIn => {
                if cmd.command == "/clear" {
                    state.messages.clear();
                    state
                        .messages
                        .extend(crate::services::helper_block::welcome_messages(None, state));
                } else if cmd.command == "/sessions" {
                    state.workbench_tab = crate::app::WorkbenchTab::Sessions;
                    state.focus = crate::app::WorkspaceFocus::Workbench;
                    let _ = output_tx.try_send(OutputEvent::ListSessions);
                } else if cmd.command == "/runtime" {
                    state.workbench_tab = crate::app::WorkbenchTab::Runtime;
                    state.focus = crate::app::WorkspaceFocus::Workbench;
                    let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                    let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
                } else if cmd.command == "/agents" {
                    state.workbench_tab = crate::app::WorkbenchTab::Agents;
                    state.focus = crate::app::WorkspaceFocus::Workbench;
                    let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
                    let _ = output_tx.try_send(OutputEvent::LoadAgentState);
                } else if cmd.command == "/shell" {
                    state.add_user_message(trimmed.clone());
                    let shell_cmd = cmd_args.unwrap_or_default().to_string();
                    if policy_gate_allows_shell_command(state, &shell_cmd) {
                        let _ = output_tx.try_send(OutputEvent::ExecuteCommand(
                            shell_cmd,
                            state.active_isolation_mode.clone(),
                        ));
                    }
                } else if cmd.command == "/shell-focus" {
                    shell_handler::foreground(state);
                } else if cmd.command == "/shell-bg" {
                    shell_handler::background(state);
                } else if cmd.command == "/shell-kill" {
                    shell_handler::kill(state);
                } else if cmd.command == "/context" {
                    state.add_user_message(trimmed.clone());
                    match cmd_args {
                        Some(args) if args.starts_with("pin ") => {
                            let file = args.trim_start_matches("pin ").trim();
                            if file.is_empty() {
                                state.add_assistant_message(
                                    "Usage: /context pin <file>".to_string(),
                                );
                            } else if state.pinned_files.iter().any(|p| p == file) {
                                state.add_assistant_message(format!(
                                    "Already pinned in context: {file}"
                                ));
                            } else {
                                state.pinned_files.push(file.to_string());
                                state.push_activity(
                                    crate::app::ActivityKind::Status,
                                    format!("Pinned context file: {file}"),
                                );
                                state.add_assistant_message(format!(
                                    "Pinned file into context: {file}"
                                ));
                            }
                        }
                        _ => {
                            state.add_assistant_message("Usage: /context pin <file>".to_string());
                        }
                    }
                } else if cmd.command == "/new" {
                    let _ = output_tx.try_send(OutputEvent::NewSession);
                } else if cmd.command == "/export" {
                    state.add_user_message(trimmed.clone());
                    let output_path = cmd_args
                        .map(std::path::PathBuf::from)
                        .map(|p| {
                            if p.is_absolute() {
                                p
                            } else {
                                state.project_root.join(p)
                            }
                        })
                        .unwrap_or_else(|| {
                            state.project_root.join(".vac/exports/session.bundle.json")
                        });
                    state.toasts.push(crate::services::Toast::info(
                        "Mengekspor bundle...".to_string(),
                    ));
                    let _ = output_tx.try_send(OutputEvent::ExportBundle(output_path));
                } else if cmd.command == "/import" {
                    state.add_user_message(trimmed.clone());
                    let Some(arg) = cmd_args else {
                        state.toasts.push(crate::services::Toast::error(
                            "Gunakan: /import <path>".to_string(),
                        ));
                        state.add_assistant_message("Gunakan: /import <path>".to_string());
                        return true;
                    };
                    let mut input_path = std::path::PathBuf::from(arg);
                    if !input_path.is_absolute() {
                        input_path = state.project_root.join(input_path);
                    }
                    state.toasts.push(crate::services::Toast::info(
                        "Mengimpor bundle...".to_string(),
                    ));
                    let _ = output_tx.try_send(OutputEvent::ImportBundle(input_path));
                } else if cmd.command == "/review" {
                    state.add_user_message(trimmed.clone());
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::open(&mut ctx);
                } else if cmd.command == "/model" {
                    state.add_user_message(trimmed.clone());
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = model_switcher::open(&mut ctx);
                } else if cmd.command == "/files" {
                    state.add_user_message(trimmed.clone());
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = file_search::open(&mut ctx);
                } else if cmd.command == "/changes" {
                    state.add_user_message(trimmed.clone());
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = changeset_handler::open(&mut ctx);
                } else if cmd.command == "/file-changes" {
                    crate::overlay::open_overlay(state, crate::overlay::OverlayId::FileChanges);
                    state.file_changes_selected = 0;
                    state.file_changes_scroll = 0;
                    state.file_changes_search.clear();
                } else if cmd.command == "/plan" {
                    state.add_user_message(trimmed.clone());
                    let project_root = state.project_root.clone();
                    if let Some((meta, content)) =
                        crate::services::plan::read_plan_file(&project_root)
                    {
                        state.plan.metadata = Some(meta);
                        state.plan.draft = content;
                    } else {
                        let title = state
                            .session_title
                            .clone()
                            .unwrap_or_else(|| "Session Plan".to_string());
                        let tmpl = crate::services::plan::new_plan_template(&title);
                        if let Err(e) = crate::services::plan::write_plan_file(&project_root, &tmpl)
                        {
                            state.add_assistant_message(format!("Failed to create plan: {}", e));
                        } else {
                            state.plan.metadata =
                                crate::services::plan::parse_plan_front_matter(&tmpl);
                            state.plan.draft = tmpl;
                        }
                    }
                    state.plan.mode_active = true;
                    state.workbench_tab = crate::app::WorkbenchTab::Plan;
                    state.focus = crate::app::WorkspaceFocus::Workbench;
                } else if cmd.command == "/plan-review" {
                    let project_root = state.project_root.clone();
                    if let Some((meta, content)) =
                        crate::services::plan::read_plan_file(&project_root)
                    {
                        state.plan.metadata = Some(meta);
                        state.plan.draft = content;
                        state.plan.review_open = true;
                        state.plan.review_selected = 0;
                        state.plan.review_scroll = 0;
                    } else {
                        state.add_assistant_message("No plan.md yet. Run /plan first.".to_string());
                    }
                } else if cmd.command == "/plan-edit" {
                    state.add_user_message(trimmed.clone());
                    crate::handlers::input_editor::plan_open_editor(state);
                } else {
                    let expanded = state.expand_pending_pastes(&trimmed);
                    state.add_user_message(expanded.clone());
                    let parts = std::mem::take(&mut state.pending_image_parts);
                    let _ =
                        output_tx.try_send(OutputEvent::UserMessage(expanded, None, parts, None));
                }
            }
            crate::app::CommandSource::BuiltInWithPrompt { prompt_content }
            | crate::app::CommandSource::Custom { prompt_content } => {
                let prompt = match cmd_args {
                    Some(args) => format!("{}\n\n{}", prompt_content, args),
                    None => prompt_content,
                };
                state
                    .pending_user_messages
                    .push_back(crate::app::PendingUserMessage::new(
                        prompt,
                        None,
                        vec![],
                        trimmed.clone(),
                    ));
            }
            crate::app::CommandSource::Passthrough => {
                let expanded = state.expand_pending_pastes(&trimmed);
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
        return true;
    }
    false
}
