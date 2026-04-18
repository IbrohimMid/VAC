
use crate::app::{AppState, InputEvent, OutputEvent};
use tokio::sync::mpsc::Sender;

pub fn flush_pending_user_messages_if_idle(
    state: &mut AppState,
    input_tx: &Sender<InputEvent>,
    output_tx: &Sender<OutputEvent>,
) {
    if state.loading_manager.is_loading() || state.loading || state.is_streaming {
        return;
    }

    let mut merged = match state.pending_user_messages.pop_front() {
        Some(m) => m,
        None => return,
    };

    while let Some(next) = state.pending_user_messages.pop_front() {
        merged.merge_from(next);
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
            if let Err(e) = input_tx.try_send(InputEvent::AddUserMessage(merged.user_message_text.clone())) {
                log::warn!("Failed to send AddUserMessage event: {}", e);
                state.add_user_message(merged.user_message_text);
            }
        }
        Err(_) => {
            log::warn!("Failed to flush buffered UserMessage event: output channel unavailable");
            state.pending_user_messages.push_front(merged);
        }
    }
}

fn estimate_context_percent(model: Option<&crate::types::Model>, tokens_used: u64) -> f32 {
    if tokens_used == 0 {
        return 0.0;
    }
    let window: u64 = match model {
        Some(m) => {
            let id = m.id.to_lowercase();
            let name = m.name.to_lowercase();
            if id.contains("claude")
                || name.contains("claude")
                || id.contains("sonnet")
                || id.contains("opus")
                || id.contains("haiku")
            {
                200_000
            } else if id.contains("gpt-4o") || name.contains("gpt-4o") {
                128_000
            } else if id.contains("gpt-4") || name.contains("gpt-4") {
                8_192
            } else if id.contains("gemini") {
                1_000_000
            } else {
                200_000
            }
        }
        None => 200_000,
    };
    ((tokens_used as f64 / window as f64) * 100.0).clamp(0.0, 100.0) as f32
}

/// Suspend the TUI, launch `$EDITOR` on plan.md, then re-read the file on
/// return. Mirrors `handlers::review::open_editor` so editor integration stays
/// consistent across the app.
pub fn open_ask_user_popup(state: &mut AppState, tc: &crate::types::ToolCall) {
    let args = crate::services::ask_user::parse_args(&tc.function.arguments);
    // Default policy: allow free-text iff the caller opts in OR no options
    // were supplied (otherwise the user would have no way to answer).
    let (question, options, allow_free_text, kind, metadata) = match args {
        Some(a) => {
            let k = a.effective_kind();
            let free = matches!(
                k,
                crate::services::ask_user::AskUserQuestionKind::FreeText
                    | crate::services::ask_user::AskUserQuestionKind::Mixed
            ) || a.allow_free_text
                || a.options.is_empty();
            (Some(a.question), a.options, free, k, a.metadata)
        }
        None => (
            Some("The assistant needs more information.".to_string()),
            Vec::new(),
            true,
            crate::services::ask_user::AskUserQuestionKind::FreeText,
            std::collections::HashMap::new(),
        ),
    };
    state.ask_user_question = question;
    state.ask_user_options = options;
    state.ask_user_selected = 0;
    state.ask_user_input.clear();
    state.ask_user_allow_free_text = allow_free_text;
    state.ask_user_question_kind = kind;
    state.ask_user_metadata = metadata;
    state.ask_user_multi_selected.clear();
    state.ask_user_filter.clear();
    state.ask_user_search_active = false;
    state.ask_user_scroll = 0;
    state.ask_user_tool_call_id = Some(tc.id.clone());
    state.show_ask_user_popup = true;
    state.push_activity(
        crate::app::ActivityKind::Approval,
        "Assistant requested input",
    );
}

fn is_low_risk_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_read"
            | "file_write"
            | "file_edit"
            | "glob"
            | "grep"
            | "search"
            | "vil_knowledge"
            | "vil_diagnostics"
            | "vil_status"
            | "vil_lsp_query"
            | "vil_ir_diff"
            | "vil_audit"
            | "vil_plumbing"
            | "vil_repair"
    )
}

fn is_vil_tool(tool_name: &str) -> bool {
    tool_name.starts_with("vil_")
}

pub(crate) fn truncate_banner_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars()
        .take(max_chars.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

pub fn classify_critical_banner(
    text: &str,
) -> Option<(
    crate::services::banner::BannerStyle,
    crate::services::banner::BannerSeverity,
)> {
    let lower = text.to_ascii_lowercase();
    let tls_related = lower.contains("tls")
        || lower.contains("certificate")
        || lower.contains("ca file")
        || lower.contains("mtls")
        || lower.contains("server_name")
        || lower.contains("identity");

    if lower.contains("warden blocked")
        || lower.contains("trust/policy violation")
        || lower.contains("permission denied")
        || lower.contains("blocked:")
        || lower.contains("blocked by")
    {
        return Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Blocking,
        ));
    }

    if lower.contains("failed to connect mcp server") || lower.contains("mcp error") {
        return Some(if tls_related {
            (
                crate::services::banner::BannerStyle::Error,
                crate::services::banner::BannerSeverity::Blocking,
            )
        } else {
            (
                crate::services::banner::BannerStyle::Warning,
                crate::services::banner::BannerSeverity::Suggested,
            )
        });
    }

    if lower.contains("agent loop exceeded")
        || lower.contains("iterations without completing")
        || lower.contains("max iterations")
    {
        return Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Blocking,
        ));
    }

    if lower.contains("rate limited") || lower.contains("retry after") || lower.contains("429") {
        return Some((
            crate::services::banner::BannerStyle::Warning,
            crate::services::banner::BannerSeverity::Suggested,
        ));
    }

    if lower.contains("all providers in fallback chain failed")
        || lower.contains("maximum retry attempts")
        || lower.contains("max retry")
    {
        return Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Suggested,
        ));
    }

    None
}

pub(crate) fn push_banner_direct(
    state: &mut AppState,
    text: String,
    style: crate::services::banner::BannerStyle,
    severity: crate::services::banner::BannerSeverity,
) {
    let msg = crate::services::banner::BannerMessage::new(text, style).with_severity(severity);
    state.banner_queue.push(msg);
    state.banner_message = state.banner_queue.current().cloned();
}

pub(crate) fn policy_gate_allows_shell_command(state: &mut AppState, cmd: &str) -> bool {
    let Ok(config) = vac_core::VacConfig::load_with_fallback(&state.project_root) else {
        return true;
    };
    let Some(action) = vac_core::policy_gate::classify_shell_command(cmd) else {
        return true;
    };
    let decision =
        vac_core::policy_gate::evaluate(&config.policy_gate, action, state.vil.last_score);
    match decision {
        vac_core::policy_gate::PolicyGateDecision::Allow => true,
        vac_core::policy_gate::PolicyGateDecision::Warn(msg) => {
            push_banner_direct(
                state,
                truncate_banner_text(&msg, 140),
                crate::services::banner::BannerStyle::Warning,
                crate::services::banner::BannerSeverity::Suggested,
            );
            true
        }
        vac_core::policy_gate::PolicyGateDecision::Block(msg) => {
            push_banner_direct(
                state,
                truncate_banner_text(&msg, 140),
                crate::services::banner::BannerStyle::Error,
                crate::services::banner::BannerSeverity::Blocking,
            );
            false
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
            state.is_streaming = true;
            state.streaming_message_id = Some(id);
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
            let msg = crate::services::banner::BannerMessage::new(text, style)
                .with_severity(severity);
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
            state.push_activity(
                crate::app::ActivityKind::Status,
                "Runtime jobs updated",
            );
        }
        InputEvent::SetRuntimeState(snapshot) => {
            // Detect significant state changes for activity logging
            let prev_state = state.runtime.snapshot.as_ref().map(|s| &s.state);
            let new_state = snapshot.as_ref().map(|s| &s.state);

            if let (Some(prev), Some(new)) = (prev_state, new_state) {
                match new {
                    vac_runtime::AutopilotState::WaitingApproval { tool_call_id } => {
                        if !matches!(prev, vac_runtime::AutopilotState::WaitingApproval { .. }) {
                            state.push_activity(
                                crate::app::ActivityKind::Approval,
                                format!("Runtime waiting for approval: {}", &tool_call_id[..8]),
                            );
                            state.toasts.push(crate::services::Toast::info(format!(
                                "Runtime waiting for approval: {}",
                                &tool_call_id[..8]
                            )));
                        }
                    }
                    vac_runtime::AutopilotState::Backoff { until } => {
                        if !matches!(prev, vac_runtime::AutopilotState::Backoff { .. }) {
                            state.push_activity(
                                crate::app::ActivityKind::Status,
                                format!(
                                    "Runtime entered backoff until {}",
                                    until.format("%H:%M:%S")
                                ),
                            );
                            state.toasts.push(crate::services::Toast::info(format!(
                                "Runtime backoff until {}",
                                until.format("%H:%M:%S")
                            )));
                        }
                    }
                    _ => {}
                }
            }

            // Detect execution environment changes
            if let Some(new_snapshot) = &snapshot {
                if let Some(prev_snapshot) = &state.runtime.snapshot {
                    if prev_snapshot.execution_environment != new_snapshot.execution_environment {
                        let env_name = match new_snapshot.execution_environment {
                            vac_core::ExecutionEnvironment::Host => "host",
                            vac_core::ExecutionEnvironment::IsolatedBatch => "isolated-batch",
                            vac_core::ExecutionEnvironment::IsolatedInteractive => {
                                "isolated-interactive"
                            }
                        };
                        state.push_activity(
                            crate::app::ActivityKind::Status,
                            format!("Execution environment switched to {}", env_name),
                        );
                        state.toasts.push(crate::services::Toast::info(format!(
                            "Switched to {} environment",
                            env_name
                        )));
                    }
                }
            }

            state.runtime.snapshot = snapshot;
        }
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

                session.prompt_ready =
                    vac_shell::detect_prompt_ready(&session.output);
                session.password_mode =
                    vac_shell::detect_password_prompt(&text);
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
                session.lifecycle =
                    vac_shell::ShellLifecycle::Error(text.clone());
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
        InputEvent::ChangesetUpdated => {
            let files = state.changeset_store.modified_files();
            let project_root = state.project_root.clone();
            if let Some(tx) = state.input_tx.clone() {
                tokio::spawn(async move {
                    if let Ok(pipeline) = vil_ir::IrPipeline::new(&project_root) {
                        if let Ok(report) = vil_validate::validate_changes(&pipeline, &files) {
                            let profile =
                                vac_core::detector::VilProjectProfile::detect(&project_root);
                            let mut ir_metadata_files = vec![];
                            for (path, module) in pipeline.modules() {
                                let has_vil_attr =
                                    module.structs.iter().any(|s| !s.vil_attrs.is_empty())
                                        || module.functions.iter().any(|f| !f.vil_attrs.is_empty());
                                if has_vil_attr {
                                    ir_metadata_files.push(path.clone());
                                }
                            }

                            let config = vac_core::VacConfig::load_with_fallback(&project_root)
                                .unwrap_or_default();
                            let semantic_mode = config.memory.enable_semantic;

                            let active_rulebook = {
                                let books = vac_core::rulebook::RulebookLoader::load_all(
                                    &project_root,
                                    &config.rulebook.paths,
                                );
                                if books.is_empty() {
                                    None
                                } else {
                                    Some(
                                        books
                                            .iter()
                                            .map(|b| b.id.clone())
                                            .collect::<Vec<_>>()
                                            .join(", "),
                                    )
                                }
                            };

                            let snapshot = crate::app::VilStatusSnapshot {
                                profile: Some(profile),
                                validation_score: report.score,
                                validation_issues: report
                                    .issues
                                    .into_iter()
                                    .map(crate::app::VilIssue::from_raw)
                                    .collect(),
                                active_rulebook,
                                semantic_mode,
                                ir_generation_active: true,
                                ir_metadata_files,
                            };
                            let _ = tx.send(InputEvent::VilStatusUpdated(snapshot)).await;
                        }
                    }
                });
            }
        }
        InputEvent::StartupHydrated(snapshot) => {
            state.startup = snapshot;
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
        } => {
            state.session_id = id;
            state.session_title = Some(title);
            state.messages = messages;
            state.loading = false;

            // Clear transient UI state to prevent leakage between sessions
            state.pending_approvals.clear();
            state.pending_tool_calls.clear();
            state.approved_tools.clear();
            state.rejected_tools.clear();
            state.approval_explanations.clear();
            state.approval_selected_idx = 0;
            state.approval_detail_scroll = 0;
            state.reject_reason_input = None;
            state.at_trigger_active = false;
            state.at_query.clear();
            state.at_results.clear();
            state.is_streaming = false;
            state.streaming_message_id = None;
            state.scroll = 0;
            state.input.clear();
            state.review.open = false;
            state.review.filter.clear();
            state.review.diff = None;
            state.review.selected_idx = 0;
            state.review.selected_path = None;
            state.shell = crate::app::ShellState::default();
            state.runtime.jobs.clear();
            state.runtime.selected_idx = 0;
            state.runtime.filter.clear();
            state.runtime.detail_scroll = 0;
            state.runtime.snapshot = None;
            state.activity.clear();
            state.activity_scroll = 0;
            state.toasts.clear();
            state.show_model_switcher = false;
            state.model_switcher_filter.clear();
            state.model_switcher_selected_idx = 0;
            state.show_file_search = false;
            state.file_search_query.clear();
            state.file_search_selected_idx = 0;
            state.file_search_results.clear();
            state.show_changeset = false;
            state.changeset_selected_idx = 0;
            state.changeset_diff_scroll = 0;
            state.changeset_selected_path = None;
            state.changeset_store.clear();
            state.modified_files = state.changeset_store.modified_files(); // derived: empty after clear
            state.changeset_diff = None;
            state.vil.score_history.clear();
            state.vil.event_log.clear();
            state.vil.last_score = None;
            state.workbench_tab = crate::app::WorkbenchTab::Approvals;
            state.focus = crate::app::WorkspaceFocus::Input;
            state.push_activity(crate::app::ActivityKind::Session, "Session restored");
        }
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
        InputEvent::ToolResult(result) => {
            state.pending_tool_calls.retain(|c| c.id != result.call.id);
            state.approved_tools.retain(|c| c.id != result.call.id);
            if result.status == crate::types::ToolCallResultStatus::Error {
                if let Some((style, severity)) = classify_critical_banner(&result.result) {
                    push_banner_direct(
                        state,
                        truncate_banner_text(&result.result, 140),
                        style,
                        severity,
                    );
                }
            }
            state.add_assistant_message(result.result);
            state.push_activity(
                crate::app::ActivityKind::Tool,
                format!("Tool result: {}", result.call.function.name),
            );
            if is_vil_tool(&result.call.function.name) {
                let status = match result.status {
                    crate::types::ToolCallResultStatus::Success => "ok",
                    crate::types::ToolCallResultStatus::Error => "error",
                    crate::types::ToolCallResultStatus::Pending => "pending",
                };
                state.push_vil_log(format!("tool {status} {}", result.call.function.name));
            }
        }
        InputEvent::TaskCompleted(result) => {
            state.vil.last_score = result.validation_score;
            // Real usage wiring: `vac_core::task::TaskResult.total_tokens_used`
            // is the authoritative producer. We record this turn's total, add
            // to session running total, and derive a coarse context %.
            let turn_tokens = result.total_tokens_used;
            state.current_message_usage = crate::app::TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: turn_tokens,
            };
            state.total_session_usage.total_tokens = state
                .total_session_usage
                .total_tokens
                .saturating_add(turn_tokens);
            state.context_usage_percent =
                estimate_context_percent(state.current_model.as_ref(), turn_tokens);

            let mut content = result.summary.clone();
            let mut changeset_updated = false;
            if !result.modified_files.is_empty() {
                content.push_str("\n\n**Modified Files**:\n");
                for file in &result.modified_files {
                    content.push_str(&format!("- `{}`\n", file));
                    state
                        .changeset_store
                        .file_modified(file.clone(), "agent".to_string(), true);
                    changeset_updated = true;
                }
            }
            if !result.created_files.is_empty() {
                content.push_str("\n**Created Files**:\n");
                for file in &result.created_files {
                    content.push_str(&format!("- `{}`\n", file));
                    state
                        .changeset_store
                        .file_created(file.clone(), "agent".to_string());
                    changeset_updated = true;
                }
            }
            // Sync derived view from store (single source of truth)
            state.modified_files = state.changeset_store.modified_files();

            if changeset_updated {
                if let Some(tx) = state.input_tx.clone() {
                    let _ = tx.try_send(InputEvent::ChangesetUpdated);
                }
            }

            if state.review.open {
                state.review.generation = state.review.generation.saturating_add(1);
                state.review_sync_items();
                state.review_normalize_selection();
            }
            state.add_assistant_message(content);
            state.loading = false;
            state.is_streaming = false;
            state.push_activity(crate::app::ActivityKind::Status, "Task completed");
        }
        _ => {}
    }
}

