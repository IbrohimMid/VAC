use crate::app::{AppState, InputEvent, OutputEvent};
use tokio::sync::mpsc::Sender;

pub mod events;
pub mod helpers;

pub use helpers::{
    classify_critical_banner, estimate_context_percent, is_low_risk_tool, is_vil_tool,
    open_ask_user_popup,
};
pub(crate) use helpers::{
    policy_gate_allows_shell_command, push_banner_direct, truncate_banner_text,
};

pub fn flush_pending_user_messages_if_idle(
    state: &mut AppState,
    input_tx: &Sender<InputEvent>,
    output_tx: &Sender<OutputEvent>,
) {
    if state.loading_manager.is_loading() || state.loading || state.is_streaming {
        return;
    }

    // Bounded queue policy: if queue exceeds 64 items, drop oldest messages
    const MAX_QUEUE_SIZE: usize = 64;
    while state.pending_user_messages.len() > MAX_QUEUE_SIZE {
        state.pending_user_messages.pop_front();
        state.queue_metrics.total_dropped += 1;
        log::warn!("Dropped oldest pending user message (queue overflow)");
    }

    let mut merged = match state.pending_user_messages.pop_front() {
        Some(m) => m,
        None => return,
    };

    state.queue_metrics.total_queued += 1;

    let merge_count = state.pending_user_messages.len();
    while let Some(next) = state.pending_user_messages.pop_front() {
        merged.merge_from(next);
    }
    if merge_count > 0 {
        state.queue_metrics.total_merged += merge_count as u64;
    }

    let revert_index = state.pending_revert_index.take();

    if state.banner_message.is_some() {
        state.banner_message = None;
        state.banner_click_regions.clear();
        state.banner_dismiss_region = None;
    }

    match output_tx.try_send(OutputEvent::UserMessage(
        merged.final_input.clone(),
        merged.shell_tool_calls.clone(),
        merged.image_parts.clone(),
        revert_index,
    )) {
        Ok(()) => {
            // Reset flush retries on success
            state.queue_metrics.flush_retries = 0;
            state.queue_metrics.last_flush_error = None;

            if let Err(e) =
                input_tx.try_send(InputEvent::AddUserMessage(merged.user_message_text.clone()))
            {
                log::warn!("Failed to send AddUserMessage event: {}", e);
                state.add_user_message(merged.user_message_text);
            }
        }
        Err(_) => {
            state.queue_metrics.flush_retries += 1;
            let error_msg = "output channel unavailable".to_string();
            state.queue_metrics.last_flush_error = Some(error_msg.clone());
            log::warn!("Failed to flush buffered UserMessage event: {}", error_msg);
            state.pending_user_messages.push_front(merged);

            // On repeated flush failure (3+ retries), push a toast to notify the operator
            if state.queue_metrics.flush_retries >= 3 {
                state.toasts.push(crate::services::Toast::error(format!(
                    "Message queue flush failing ({} retries): {}",
                    state.queue_metrics.flush_retries, error_msg
                )));
                if state.toasts.len() > 3 {
                    state.toasts.drain(0..state.toasts.len().saturating_sub(3));
                }
            }
        }
    }
}

pub fn handle_backend_event(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    match event {
        InputEvent::AddUserMessage(msg) => {
            state.add_user_message(msg);
        }
        InputEvent::AssistantMessage(msg) => {
            let extracted = crate::services::todo_extractor::extract_todos(&msg);
            if !extracted.is_empty() {
                state.todos = extracted;
            }
            state.add_assistant_message(msg);
            state.loading = false;
            state.push_activity(crate::app::ActivityKind::Status, "Assistant message");
        }
        InputEvent::StreamAssistantMessage(id, chunk) => {
            if !state.is_streaming {
                state.streaming_start = Some(std::time::Instant::now());
                state.streaming_tokens = 0;
            }
            state.is_streaming = true;
            state.streaming_message_id = Some(id);
            // Approximate token count: one token ≈ one space-delimited word.
            state.streaming_tokens += chunk.split_whitespace().count() as u64;
            if let Some(last) = state.messages.last_mut() {
                if last.role == "assistant" {
                    last.content.push_str(&chunk);
                    return;
                }
            }
            state.add_assistant_message(chunk);
        }
        InputEvent::StartLoadingOperation(op) => {
            let op_label = format!("{op:?}");
            state.loading_manager.start_operation(op);
            state.loading = true;
            state.push_activity(
                crate::app::ActivityKind::Status,
                format!("Loading: {op_label}"),
            );
        }
        InputEvent::EndLoadingOperation(op) => {
            let op_label = format!("{op:?}");
            state.loading_manager.end_operation(op);
            state.loading = state.loading_manager.is_loading();
            state.is_streaming = false;
            state.streaming_start = None;
            state.streaming_tokens = 0;
            // Stream/LLM turns end here; scan the most recent assistant message
            // for a `<todo>` block and refresh the side-panel surface.
            if let Some(last) = state.messages.iter().rev().find(|m| m.role == "assistant") {
                let extracted = crate::services::todo_extractor::extract_todos(&last.content);
                if !extracted.is_empty() {
                    state.todos = extracted;
                }
            }
            state.push_activity(
                crate::app::ActivityKind::Status,
                format!("Done: {op_label}"),
            );
        }
        InputEvent::Error(msg) => {
            if let Some((style, severity)) = classify_critical_banner(&msg) {
                push_banner_direct(state, truncate_banner_text(&msg, 140), style, severity);
            }
            let lower = msg.to_ascii_lowercase();
            if lower.contains("vil") || lower.contains("ir") || lower.contains("rulebook") {
                state.push_vil_log(format!("error: {}", truncate_banner_text(&msg, 120)));
            }
            state.add_assistant_message(format!("Error: {}", msg));
            state
                .toasts
                .push(crate::services::Toast::error(msg.clone()));
            if state.toasts.len() > 3 {
                state.toasts.drain(0..state.toasts.len().saturating_sub(3));
            }
            state.loading = false;
            state.is_streaming = false;
            state.push_activity(crate::app::ActivityKind::Error, msg);
        }
        InputEvent::SetCurrentModel(model) => {
            state.current_model = Some(model);
        }
        InputEvent::AvailableModelsLoaded(models) => {
            state.available_models = models;
        }
        InputEvent::ValidationResult(score, issues) => {
            state.validation_score = Some(score);
            state.validation_issues = issues;
        }
        InputEvent::LspStatus(available, _binary_path) => {
            state.lsp_available = available;
        }
        InputEvent::LspDiagnostics(snapshot) => {
            state.lsp_diagnostics = Some(snapshot);
        }
        InputEvent::TaskCancelled => {
            state.push_activity(crate::app::ActivityKind::Status, "Task cancelled");
        }
        InputEvent::ShowBanner(text, style, severity) => {
            let msg =
                crate::services::banner::BannerMessage::new(text, style).with_severity(severity);
            state.banner_queue.push(msg);
            state.banner_message = state.banner_queue.current().cloned();
        }
        InputEvent::ShowToast(toast) => {
            let lower = toast.message.to_ascii_lowercase();
            if lower.contains("vil")
                || lower.contains("rulebook")
                || lower.contains("archetype")
                || lower.contains("ir")
            {
                state.push_vil_log(toast.message.clone());
            }
            state.toasts.push(toast);
            if state.toasts.len() > 3 {
                state.toasts.drain(0..state.toasts.len().saturating_sub(3));
            }
        }
        InputEvent::SetSessions(sessions) => {
            state.sessions = sessions;
            state.sessions_selected_idx = 0;
            state.push_activity(crate::app::ActivityKind::Session, "Sessions updated");
        }
        InputEvent::SetSessionResumeList(entries) => {
            state.session_resume_list = entries;
            state.session_resume_selected = 0;
            crate::handlers::input_popup::refresh_session_resume_filtered(state);
        }
        InputEvent::SetAgentTasks(tasks) => {
            state.runtime.agent_tasks = tasks;
            if state.runtime.agent_selected >= state.runtime.agent_tasks.len() {
                state.runtime.agent_selected = state.runtime.agent_tasks.len().saturating_sub(1);
            }
            state.push_activity(crate::app::ActivityKind::Status, "Agent queue updated");
        }
        InputEvent::SetAgentState(snapshot) => {
            state.runtime.agent_snapshot = snapshot;
            state.push_activity(crate::app::ActivityKind::Status, "Agent state updated");
        }
        InputEvent::SetRuntimeJobs(jobs) => {
            state.runtime.jobs = jobs;
            if state.runtime.selected_idx >= state.runtime.jobs.len() {
                state.runtime.selected_idx = state.runtime.jobs.len().saturating_sub(1);
            }
            state.push_activity(crate::app::ActivityKind::Status, "Runtime jobs updated");
        }
        InputEvent::SetRuntimeState(snapshot) => events::on_set_runtime_state(state, snapshot),
        InputEvent::SetTaskGraphProjection(projection) => {
            state.runtime.task_projection = projection;
            state.push_activity(
                crate::app::ActivityKind::Status,
                "Task graph projection updated",
            );
        }
        InputEvent::FileIndexReady(files) => {
            state.all_files = files;
            state.file_search_results = state.all_files.iter().take(50).cloned().collect();
            state.toasts.push(crate::services::Toast::success(
                "File index ready".to_string(),
            ));
        }
        InputEvent::ShellStarted(shell) => {
            let label = if shell.command.trim().is_empty() {
                format!("shell-{}", state.shell.session_store.sessions.len() + 1)
            } else {
                shell.command.clone()
            };
            let idx = state.shell.session_store.push_new(label);
            if let Some(session) = state.shell.session_store.sessions.get_mut(idx) {
                session.command = Some(shell.clone());
                session.backgrounded = false;
                session.waiting_for_input = false;
                session.exit_code = None;
                session.last_error = None;
                session.output.clear();
                session.history_idx = None;
                session.prompt_ready = false;
                session.password_mode = false;
                session.lifecycle = vac_shell::ShellLifecycle::Running;
            }
            state.shell.session_store.popup_visible = true;
            state.push_activity(
                crate::app::ActivityKind::Shell,
                format!("Shell started: {}", shell.command),
            );
        }
        InputEvent::ShellOutput(id, text) => {
            let target = if id == "system" {
                state.shell.session_store.active_mut()
            } else {
                state.shell.session_store.find_by_command_id_mut(&id)
            };
            if let Some(session) = target {
                session.output.push_str(&text);

                let max_size = 1024 * 1024;
                if session.output.len() > max_size {
                    let keep_len = max_size - 128 * 1024;
                    let mut safe_idx = session.output.len() - keep_len;
                    while safe_idx < session.output.len()
                        && !session.output.is_char_boundary(safe_idx)
                    {
                        safe_idx += 1;
                    }
                    session.output = session.output[safe_idx..].to_string();
                }

                session.prompt_ready = vac_shell::detect_prompt_ready(&session.output);
                session.password_mode = vac_shell::detect_password_prompt(&text);
                session.lifecycle = if session.prompt_ready {
                    vac_shell::ShellLifecycle::PromptReady
                } else {
                    vac_shell::ShellLifecycle::Running
                };
            }
        }
        InputEvent::ShellError(id, text) => {
            let target = if id == "system" {
                state.shell.session_store.active_mut()
            } else {
                state.shell.session_store.find_by_command_id_mut(&id)
            };
            if let Some(session) = target {
                if !session.output.ends_with('\n') && !session.output.is_empty() {
                    session.output.push('\n');
                }
                session.output.push_str(&format!("[shell error] {text}\n"));

                let max_size = 1024 * 1024;
                if session.output.len() > max_size {
                    let keep_len = max_size - 128 * 1024;
                    let mut safe_idx = session.output.len() - keep_len;
                    while safe_idx < session.output.len()
                        && !session.output.is_char_boundary(safe_idx)
                    {
                        safe_idx += 1;
                    }
                    session.output = session.output[safe_idx..].to_string();
                }

                session.last_error = Some(text.clone());
                session.lifecycle = vac_shell::ShellLifecycle::Error(text.clone());
            }
            state.push_activity(
                crate::app::ActivityKind::Shell,
                format!("Shell error: {}", text),
            );
        }
        InputEvent::ShellCompleted(id, code) => {
            let target = if id == "system" {
                state.shell.session_store.active_mut()
            } else {
                state.shell.session_store.find_by_command_id_mut(&id)
            };
            if let Some(session) = target {
                session.exit_code = Some(code);
                session.waiting_for_input = false;
                session.command = None;
                session.prompt_ready = false;
                session.password_mode = false;
                session.lifecycle = if code == -1 || code == 137 {
                    vac_shell::ShellLifecycle::Killed
                } else {
                    vac_shell::ShellLifecycle::Exited(code)
                };
            }
            let status = if code == 0 { "success" } else { "failed" };
            state.push_activity(
                crate::app::ActivityKind::Shell,
                format!("Shell {} (exit code {})", status, code),
            );
        }
        InputEvent::ShellWaitingForInput(id) => {
            let target = if id == "system" {
                state.shell.session_store.active_mut()
            } else {
                state.shell.session_store.find_by_command_id_mut(&id)
            };
            if let Some(session) = target {
                session.waiting_for_input = true;
            }
        }
        InputEvent::McpConnected { name, tools } => {
            state.push_activity(
                crate::app::ActivityKind::Mcp,
                format!("MCP server '{}' connected ({} tools)", name, tools),
            );
        }
        InputEvent::McpFailed { name, error } => {
            state.push_activity(
                crate::app::ActivityKind::Mcp,
                format!("MCP server '{}' failed: {}", name, error),
            );
        }
        InputEvent::McpServerState(name, conn_state) => {
            if matches!(
                conn_state.status,
                vac_tools::mcp::McpConnectionStatus::Unreachable(_)
            ) {
                let reason = match &conn_state.status {
                    vac_tools::mcp::McpConnectionStatus::Unreachable(r) => r.clone(),
                    _ => String::new(),
                };
                push_banner_direct(
                    state,
                    truncate_banner_text(
                        &format!("MCP server '{name}' unreachable: {reason}"),
                        140,
                    ),
                    crate::services::banner::BannerStyle::Warning,
                    crate::services::banner::BannerSeverity::Suggested,
                );
            }
            state.mcp_server_states.insert(name, conn_state);
        }
        InputEvent::VilStatusUpdated(snapshot) => {
            state.record_vil_score(snapshot.validation_score);
            state.push_vil_log(format!(
                "vil_status score={:.2} issues={}",
                snapshot.validation_score,
                snapshot.validation_issues.len()
            ));
            state.vil.status = snapshot;
            state.push_activity(
                crate::app::ActivityKind::Status,
                "VIL status updated".to_string(),
            );
        }
        InputEvent::ChangesetUpdated => events::on_changeset_updated(state),
        InputEvent::StartupHydrated(snapshot) => {
            state.startup = snapshot;
            state.hydrated = true;
        }
        InputEvent::IsolationBoundary {
            action,
            environment,
        } => {
            state.push_activity(
                crate::app::ActivityKind::Isolation,
                format!("Isolation: {} in {}", action, environment),
            );
        }
        InputEvent::SessionRestored {
            id,
            title,
            messages,
        } => events::on_session_restored(state, id, title, messages),
        InputEvent::ShowConfirmationDialog(tc) => {
            if tc.function.name == crate::services::ask_user::ASK_USER_TOOL_NAME {
                open_ask_user_popup(state, &tc);
                return;
            }
            state.pending_approvals.push(tc.clone());
            state.approval_selected_idx = state.pending_approvals.len().saturating_sub(1);
            state.approval_explanations.insert(tc.id.clone(), None);
            state.approval_normalize_selection();
            state.workbench_tab = crate::app::WorkbenchTab::Approvals;
            state.focus = crate::app::WorkspaceFocus::Workbench;
            state.push_activity(
                crate::app::ActivityKind::Approval,
                format!("Approval required: {}", tc.function.name),
            );
        }
        InputEvent::ShowConfirmationDialogWithExplanation(tc, explanation) => {
            if tc.function.name == crate::services::ask_user::ASK_USER_TOOL_NAME {
                open_ask_user_popup(state, &tc);
                return;
            }
            if state.auto_approve && is_low_risk_tool(&tc.function.name) {
                state.approved_tools.push(tc.clone());
                let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                state.push_activity(
                    crate::app::ActivityKind::Approval,
                    "Auto-approved low risk tool",
                );
            } else {
                state.pending_approvals.push(tc.clone());
                state.approval_selected_idx = state.pending_approvals.len().saturating_sub(1);
                state
                    .approval_explanations
                    .insert(tc.id.clone(), explanation);
                state.approval_normalize_selection();
                state.workbench_tab = crate::app::WorkbenchTab::Approvals;
                state.focus = crate::app::WorkspaceFocus::Workbench;
                state.push_activity(
                    crate::app::ActivityKind::Approval,
                    format!("Approval required: {}", tc.function.name),
                );
            }
        }
        InputEvent::RunToolCall(tc) => {
            state.pending_tool_calls.push(tc.clone());
            state.push_activity(
                crate::app::ActivityKind::Tool,
                format!("Tool started: {}", tc.function.name),
            );
            if is_vil_tool(&tc.function.name) {
                state.push_vil_log(format!("tool start {}", tc.function.name));
            }
        }
        InputEvent::ToolResult(result) => events::on_tool_result(state, result),
        InputEvent::TaskCompleted(result) => events::on_task_completed(state, result),
        _ => {}
    }
}
