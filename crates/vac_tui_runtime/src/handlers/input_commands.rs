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
            state.command_palette.shortcuts_mode = crate::app::ShortcutsPopupMode::Sessions;
            state.command_palette.shortcuts_scroll = 0;
            state.command_palette.input.clear();
            state.command_palette.selected = 0;
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::Shortcuts);
            true
        }
        CommandAction::OpenShortcuts => {
            state.command_palette.shortcuts_mode = crate::app::ShortcutsPopupMode::Shortcuts;
            state.command_palette.shortcuts_scroll = 0;
            state.command_palette.input.clear();
            state.command_palette.selected = 0;
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
                // Route through ActionSpec registry — no hardcoded if-chain.
                if let Some(spec) = crate::action_registry::spec_by_slash_alias(cmd_word) {
                    dispatch_action(spec.id, state, output_tx, cmd_args, &trimmed);
                } else {
                    // Fallback: forward unregistered BuiltIn as agent message.
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

    // Unknown slash command — show fuzzy palette suggestions.
    show_unknown_slash_suggestions(state, cmd_word);
    false
}

/// Dispatch a known ActionId to its TUI handler. Pure match — no string comparisons.
fn dispatch_action(
    id: crate::action_registry::ActionId,
    state: &mut AppState,
    output_tx: &tokio::sync::mpsc::Sender<OutputEvent>,
    cmd_args: Option<&str>,
    trimmed: &str,
) {
    use crate::action_registry::ActionId;
    match id {
        ActionId::Clear => {
            state.messages.clear();
            state
                .messages
                .extend(crate::services::helper_block::welcome_messages(None, state));
        }
        ActionId::Sessions => {
            state.workbench_tab = crate::app::WorkbenchTab::Sessions;
            state.focus = crate::app::WorkspaceFocus::Workbench;
            let _ = output_tx.try_send(OutputEvent::ListSessions);
        }
        ActionId::Runtime => {
            state.workbench_tab = crate::app::WorkbenchTab::Runtime;
            state.focus = crate::app::WorkspaceFocus::Workbench;
            let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
            let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
        }
        ActionId::Agents => {
            state.workbench_tab = crate::app::WorkbenchTab::Agents;
            state.focus = crate::app::WorkspaceFocus::Workbench;
            let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
            let _ = output_tx.try_send(OutputEvent::LoadAgentState);
        }
        ActionId::Vwfd => {
            state.workbench_tab = crate::app::WorkbenchTab::Vwfd;
            state.focus = crate::app::WorkspaceFocus::Workbench;
            // If the user passed a path after `/vwfd`, try to load it into the
            // inspector. Failures surface as a toast and in the inspector's
            // error pane, but the tab still opens.
            if let Some(raw) = cmd_args {
                let path_str = raw.trim();
                if !path_str.is_empty() {
                    let resolved = std::path::PathBuf::from(path_str);
                    let resolved = if resolved.is_absolute() {
                        resolved
                    } else {
                        state.project_root.join(&resolved)
                    };
                    match std::fs::read_to_string(&resolved) {
                        Ok(yaml) => match state.vwfd_inspector.load_yaml(&yaml) {
                            Ok(()) => {
                                state.vwfd_inspector.source_path =
                                    Some(resolved.display().to_string());
                                state.toasts.push(crate::services::Toast::info(format!(
                                    "Loaded VWFD: {}",
                                    resolved.display()
                                )));
                            }
                            Err(err) => {
                                state.toasts.push(crate::services::Toast::error(format!(
                                    "VWFD parse error: {err}"
                                )));
                            }
                        },
                        Err(err) => {
                            state.toasts.push(crate::services::Toast::error(format!(
                                "Failed to read {}: {err}",
                                resolved.display()
                            )));
                        }
                    }
                }
            }
        }
        ActionId::Shell => {
            state.add_user_message(trimmed.to_string());
            let shell_cmd = cmd_args.unwrap_or_default().to_string();
            if policy_gate_allows_shell_command(state, &shell_cmd) {
                let _ = output_tx.try_send(OutputEvent::ExecuteCommand(
                    shell_cmd,
                    state.switchers.active_isolation_mode.clone(),
                ));
            }
        }
        ActionId::ShellFocus => {
            shell_handler::foreground(state);
        }
        ActionId::ShellBackground => {
            shell_handler::background(state);
        }
        ActionId::ShellKill => {
            shell_handler::kill(state);
        }
        ActionId::Context => {
            state.add_user_message(trimmed.to_string());
            match cmd_args {
                Some(args) if args.starts_with("pin ") => {
                    let file = args.trim_start_matches("pin ").trim();
                    if file.is_empty() {
                        state.add_assistant_message("Usage: /context pin <file>".to_string());
                    } else if state.pins.files.iter().any(|p| p == file) {
                        state.add_assistant_message(format!("Already pinned in context: {file}"));
                    } else {
                        state.pins.files.push(file.to_string());
                        state.push_activity(
                            crate::app::ActivityKind::Status,
                            format!("Pinned context file: {file}"),
                        );
                        state.add_assistant_message(format!("Pinned file into context: {file}"));
                    }
                }
                _ => {
                    state.add_assistant_message("Usage: /context pin <file>".to_string());
                }
            }
        }
        ActionId::NewSession => {
            let _ = output_tx.try_send(OutputEvent::NewSession);
        }
        ActionId::Export => {
            state.add_user_message(trimmed.to_string());
            let output_path = cmd_args
                .map(std::path::PathBuf::from)
                .map(|p| {
                    if p.is_absolute() {
                        p
                    } else {
                        state.project_root.join(p)
                    }
                })
                .unwrap_or_else(|| state.project_root.join(".vac/exports/session.bundle.json"));
            state.toasts.push(crate::services::Toast::info(
                "Mengekspor bundle...".to_string(),
            ));
            let _ = output_tx.try_send(OutputEvent::ExportBundle(output_path));
        }
        ActionId::Import => {
            state.add_user_message(trimmed.to_string());
            let Some(arg) = cmd_args else {
                state.toasts.push(crate::services::Toast::error(
                    "Gunakan: /import <path>".to_string(),
                ));
                state.add_assistant_message("Gunakan: /import <path>".to_string());
                return;
            };
            let mut input_path = std::path::PathBuf::from(arg);
            if !input_path.is_absolute() {
                input_path = state.project_root.join(input_path);
            }
            state.toasts.push(crate::services::Toast::info(
                "Mengimpor bundle...".to_string(),
            ));
            let _ = output_tx.try_send(OutputEvent::ImportBundle(input_path));
        }
        ActionId::ReviewOpen => {
            state.add_user_message(trimmed.to_string());
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = review_handler::open(&mut ctx);
        }
        ActionId::SwitchModel => {
            state.add_user_message(trimmed.to_string());
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = model_switcher::open(&mut ctx);
        }
        ActionId::OpenFileSearch => {
            state.add_user_message(trimmed.to_string());
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = file_search::open(&mut ctx);
        }
        ActionId::Changes => {
            state.add_user_message(trimmed.to_string());
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = changeset_handler::open(&mut ctx);
        }
        ActionId::FileChanges => {
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::FileChanges);
            state.file_index.changes_selected = 0;
            state.file_index.changes_scroll = 0;
            state.file_index.changes_search.clear();
        }
        ActionId::OpenPlan => {
            // /plan — load or create plan.md and switch to Plan tab
            state.add_user_message(trimmed.to_string());
            let project_root = state.project_root.clone();
            if let Some((meta, content)) = crate::services::plan::read_plan_file(&project_root) {
                state.plan.metadata = Some(meta);
                state.plan.draft = content;
            } else {
                let title = state
                    .session_title
                    .clone()
                    .unwrap_or_else(|| "Session Plan".to_string());
                let tmpl = crate::services::plan::new_plan_template(&title);
                if let Err(e) = crate::services::plan::write_plan_file(&project_root, &tmpl) {
                    state.add_assistant_message(format!("Failed to create plan: {}", e));
                } else {
                    state.plan.metadata = crate::services::plan::parse_plan_front_matter(&tmpl);
                    state.plan.draft = tmpl;
                }
            }
            state.plan.mode_active = true;
            state.workbench_tab = crate::app::WorkbenchTab::Plan;
            state.focus = crate::app::WorkspaceFocus::Workbench;
        }
        ActionId::ApprovePlan => {
            // ApprovePlan is a workbench keybind — not reachable via slash.
            // Handled in workbench_input.rs. No-op here.
        }
        ActionId::OpenPlanReview | ActionId::PlanReview => {
            let project_root = state.project_root.clone();
            if let Some((meta, content)) = crate::services::plan::read_plan_file(&project_root) {
                state.plan.metadata = Some(meta);
                state.plan.draft = content;
                state.plan.review_open = true;
                state.plan.review_selected = 0;
                state.plan.review_scroll = 0;
            } else {
                state.add_assistant_message("No plan.md yet. Run /plan first.".to_string());
            }
        }
        ActionId::PlanEdit | ActionId::EditPlan => {
            state.add_user_message(trimmed.to_string());
            crate::handlers::input_editor::plan_open_editor(state);
        }
        ActionId::SwitchRulebook => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = rulebook_switcher::open(&mut ctx);
        }
        ActionId::SwitchProfile => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = profile_switcher::open(&mut ctx);
        }
        ActionId::OpenShortcuts => {
            state.command_palette.shortcuts_mode = crate::app::ShortcutsPopupMode::Shortcuts;
            state.command_palette.shortcuts_scroll = 0;
            state.command_palette.input.clear();
            state.command_palette.selected = 0;
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::Shortcuts);
        }
        ActionId::OpenFilePicker => {
            state.file_picker.query.clear();
            state.file_picker.selected = 0;
            state.file_picker.multi_selected.clear();
            crate::handlers::input_popup::refresh_file_picker_results_pub(state);
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::FilePicker);
        }
        ActionId::OpenTaskTray => {
            state.task_tray_selected = 0;
            state.task_tray_scroll = 0;
            let _ = output_tx.try_send(crate::app::OutputEvent::ListRuntimeJobs);
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::TaskTray);
        }
        ActionId::OpenThemePicker => {
            state.theme_picker_selected = 0;
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::ThemePicker);
        }
        ActionId::OpenSessionResume => {
            state.session_resume_query.clear();
            state.session_resume_selected = 0;
            let _ = output_tx.try_send(crate::app::OutputEvent::LoadSessionResumeList);
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::SessionResume);
        }
        // Actions not dispatched via slash — no-op here.
        _ => {}
    }
}

/// Suggests similar slash commands when input doesn't match any registered command.
///
/// Matching strategy (in priority order):
/// 1. Prefix match on the bare command name (e.g. "she" matches "/shell")
/// 2. Subsequence match — every char of query appears in order in the command
/// 3. Levenshtein distance ≤ 2 for short queries
fn show_unknown_slash_suggestions(state: &mut AppState, cmd_word: &str) {
    let query = cmd_word.trim_start_matches('/').to_lowercase();

    let mut scored: Vec<(u32, String)> = state
        .commands
        .iter()
        .filter_map(|c| {
            let name = c.command.trim_start_matches('/').to_lowercase();
            let score = match_score(&query, &name)?;
            Some((score, c.command.clone()))
        })
        .collect();

    // Lower score = better match
    scored.sort_by_key(|(score, _)| *score);
    let suggestions: Vec<String> = scored.into_iter().take(5).map(|(_, cmd)| cmd).collect();

    if suggestions.is_empty() {
        state.add_assistant_message(format!(
            "Unknown command: `{cmd_word}`. Type `/help` to see available commands."
        ));
    } else {
        state.add_assistant_message(format!(
            "Unknown command: `{cmd_word}`. Did you mean: {}",
            suggestions.join(", ")
        ));
    }
}

/// Returns a match score (lower = better) or None if no match.
fn match_score(query: &str, name: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(u32::MAX); // empty query matches everything with worst score
    }
    // Prefix match
    if name.starts_with(query) {
        return Some(0);
    }
    // Subsequence match
    if is_subsequence(query, name) {
        return Some(1);
    }
    // Levenshtein ≤ 2 (only for short queries to avoid noise)
    if query.len() <= 8 {
        let dist = levenshtein(query, name);
        if dist <= 2 {
            return Some(10 + dist as u32);
        }
    }
    None
}

fn is_subsequence(query: &str, name: &str) -> bool {
    let mut name_chars = name.chars();
    for qc in query.chars() {
        if !name_chars.any(|nc| nc == qc) {
            return false;
        }
    }
    true
}

/// Simple iterative Levenshtein distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut row: Vec<usize> = (0..=n).collect();
    for i in 1..=m {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=n {
            let old = row[j];
            row[j] = if a[i - 1] == b[j - 1] {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j - 1])
            };
            prev = old;
        }
    }
    row[n]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
