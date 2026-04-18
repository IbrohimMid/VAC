import re

with open("crates/vac_cli/src/tui/controller.rs", "r") as f:
    content = f.read()

# I will write a function `dispatch_builtin_command`
func = """
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
    if let Some(cmd) = state.commands.iter().find(|c| c.command == cmd_word).cloned() {
        match cmd.source {
            crate::tui::app::CommandSource::BuiltIn => {
                if cmd.command == "/clear" {
                    state.messages.clear();
                    state.messages.extend(
                        crate::tui::services::helper_block::welcome_messages(
                            None, state,
                        ),
                    );
                } else if cmd.command == "/sessions" {
                    state.workbench_tab = crate::tui::app::WorkbenchTab::Sessions;
                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                    let _ = output_tx.try_send(OutputEvent::ListSessions);
                } else if cmd.command == "/runtime" {
                    state.workbench_tab = crate::tui::app::WorkbenchTab::Runtime;
                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                    let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                    let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
                } else if cmd.command == "/agents" {
                    state.workbench_tab = crate::tui::app::WorkbenchTab::Agents;
                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
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
                    if state.active_shell_command.is_some() {
                        state.shell_popup_visible = true;
                        state.shell_backgrounded = false;
                    }
                } else if cmd.command == "/shell-bg" {
                    if state.active_shell_command.is_some() {
                        state.shell_popup_visible = false;
                        state.shell_backgrounded = true;
                    }
                } else if cmd.command == "/shell-kill" {
                    if let Some(shell) = state.active_shell_command.clone() {
                        let _ = shell.kill();
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
                            state
                                .project_root
                                .join(".vac/exports/session.bundle.json")
                        });
                    state.toasts.push(crate::tui::services::Toast::info(
                        "Mengekspor bundle...".to_string(),
                    ));
                    let _ =
                        output_tx.try_send(OutputEvent::ExportBundle(output_path));
                } else if cmd.command == "/import" {
                    state.add_user_message(trimmed.clone());
                    let Some(arg) = cmd_args else {
                        state.toasts.push(crate::tui::services::Toast::error(
                            "Gunakan: /import <path>".to_string(),
                        ));
                        state.add_assistant_message(
                            "Gunakan: /import <path>".to_string(),
                        );
                        return true;
                    };
                    let mut input_path = std::path::PathBuf::from(arg);
                    if !input_path.is_absolute() {
                        input_path = state.project_root.join(input_path);
                    }
                    state.toasts.push(crate::tui::services::Toast::info(
                        "Mengimpor bundle...".to_string(),
                    ));
                    let _ =
                        output_tx.try_send(OutputEvent::ImportBundle(input_path));
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
                    state.show_file_changes_popup = true;
                    state.file_changes_selected = 0;
                    state.file_changes_scroll = 0;
                    state.file_changes_search.clear();
                } else if cmd.command == "/plan" {
                    state.add_user_message(trimmed.clone());
                    let project_root = state.project_root.clone();
                    if let Some((meta, content)) =
                        crate::tui::services::plan::read_plan_file(&project_root)
                    {
                        state.plan_metadata = Some(meta);
                        state.plan_draft = content;
                    } else {
                        let title = state
                            .session_title
                            .clone()
                            .unwrap_or_else(|| "Session Plan".to_string());
                        let tmpl =
                            crate::tui::services::plan::new_plan_template(&title);
                        if let Err(e) = crate::tui::services::plan::write_plan_file(
                            &project_root,
                            &tmpl,
                        ) {
                            state.add_assistant_message(format!(
                                "Failed to create plan: {}",
                                e
                            ));
                        } else {
                            state.plan_metadata =
                                crate::tui::services::plan::parse_plan_front_matter(
                                    &tmpl,
                                );
                            state.plan_draft = tmpl;
                        }
                    }
                    state.plan_mode_active = true;
                    state.workbench_tab = crate::tui::app::WorkbenchTab::Plan;
                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                } else if cmd.command == "/plan-review" {
                    let project_root = state.project_root.clone();
                    if let Some((meta, content)) =
                        crate::tui::services::plan::read_plan_file(&project_root)
                    {
                        state.plan_metadata = Some(meta);
                        state.plan_draft = content;
                        state.plan_review_open = true;
                        state.plan_review_selected = 0;
                        state.plan_review_scroll = 0;
                    } else {
                        state.add_assistant_message(
                            "No plan.md yet. Run /plan first.".to_string(),
                        );
                    }
                } else if cmd.command == "/plan-edit" {
                    state.add_user_message(trimmed.clone());
                    plan_open_editor(state);
                } else {
                    let expanded = state.expand_pending_pastes(&trimmed);
                    state.add_user_message(expanded.clone());
                    let parts = std::mem::take(&mut state.pending_image_parts);
                    let _ = output_tx.try_send(OutputEvent::UserMessage(
                        expanded, None, parts, None,
                    ));
                }
            }
            crate::tui::app::CommandSource::BuiltInWithPrompt {
                prompt_content,
            }
            | crate::tui::app::CommandSource::Custom { prompt_content } => {
                state.add_user_message(trimmed.clone());
                let prompt = match cmd_args {
                    Some(args) => format!("{}\\n\\n{}", prompt_content, args),
                    None => prompt_content,
                };
                let _ = output_tx.try_send(OutputEvent::UserMessage(
                    prompt,
                    None,
                    vec![],
                    None,
                ));
            }
        }
        return true;
    }
    false
}
"""

content += "\n" + func

# Replace the giant block in `InputEvent::CommandPaletteSelect`
p_start = content.find("if let Some(cmd) = filtered.get(state.command_palette_selected).cloned() {")
p_end = content.find("state.show_command_palette = false;", p_start)

if p_start != -1 and p_end != -1:
    content = content[:p_start] + """if let Some(cmd) = filtered.get(state.command_palette_selected).cloned() {
                    dispatch_builtin_command(state, output_tx, &cmd.command, None);
                }
                """ + content[p_end:]

# Replace the giant block in `InputEvent::InputSubmitted`
m_start = content.find("if let Some(cmd) = state")
m_end = content.find("} else {", m_start) # There are nested else, this might be tricky

with open("crates/vac_cli/src/tui/controller.rs", "w") as f:
    f.write(content)

