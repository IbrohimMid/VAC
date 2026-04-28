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
    if state.core.loading_manager.is_loading()
        || state.core.loading
        || state.transcript.streaming.is_streaming
    {
        return;
    }

    // Bounded queue policy: if queue exceeds 64 items, drop oldest messages
    const MAX_QUEUE_SIZE: usize = 64;
    while state.transcript.pending_user_messages.len() > MAX_QUEUE_SIZE {
        state.transcript.pending_user_messages.pop_front();
        state.execution.queue_metrics.total_dropped += 1;
        log::warn!("Dropped oldest pending user message (queue overflow)");
    }

    let mut merged = match state.transcript.pending_user_messages.pop_front() {
        Some(m) => m,
        None => return,
    };

    state.execution.queue_metrics.total_queued += 1;

    let merge_count = state.transcript.pending_user_messages.len();
    while let Some(next) = state.transcript.pending_user_messages.pop_front() {
        merged.merge_from(next);
    }
    if merge_count > 0 {
        state.execution.queue_metrics.total_merged += merge_count as u64;
    }

    let revert_index = state.layout.message_ui.pending_revert_index.take();

    if state.layout.banner.message.is_some() {
        state.layout.banner.message = None;
        state.layout.banner.click_regions.clear();
        state.layout.banner.dismiss_region = None;
    }

    match output_tx.try_send(OutputEvent::UserMessage(
        merged.final_input.clone(),
        merged.shell_tool_calls.clone(),
        merged.image_parts.clone(),
        revert_index,
    )) {
        Ok(()) => {
            // Reset flush retries on success
            state.execution.queue_metrics.flush_retries = 0;
            state.execution.queue_metrics.last_flush_error = None;

            if let Err(e) =
                input_tx.try_send(InputEvent::AddUserMessage(merged.user_message_text.clone()))
            {
                log::warn!("Failed to send AddUserMessage event: {}", e);
                state.add_user_message(merged.user_message_text);
            }
        }
        Err(_) => {
            state.execution.queue_metrics.flush_retries += 1;
            let error_msg = "output channel unavailable".to_string();
            state.execution.queue_metrics.last_flush_error = Some(error_msg.clone());
            log::warn!("Failed to flush buffered UserMessage event: {}", error_msg);
            state.transcript.pending_user_messages.push_front(merged);

            // On repeated flush failure (3+ retries), push a toast to notify the operator
            if state.execution.queue_metrics.flush_retries >= 3 {
                state
                    .layout
                    .toasts
                    .push(crate::services::Toast::error(format!(
                        "Message queue flush failing ({} retries): {}",
                        state.execution.queue_metrics.flush_retries, error_msg
                    )));
                if state.layout.toasts.len() > 3 {
                    state
                        .layout
                        .toasts
                        .drain(0..state.layout.toasts.len().saturating_sub(3));
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
                state.transcript.todos = extracted;
            }
            state.add_assistant_message(msg);
            state.core.loading = false;
            state.push_activity(crate::app::ActivityKind::Status, "Assistant message");
        }
        InputEvent::StreamAssistantMessage(id, chunk) => {
            if !state.transcript.streaming.is_streaming {
                state.transcript.streaming.start = Some(std::time::Instant::now());
                state.transcript.streaming.tokens = 0;
            }
            state.transcript.streaming.is_streaming = true;
            state.transcript.streaming.message_id = Some(id);
            // Approximate token count: one token ≈ one space-delimited word.
            state.transcript.streaming.tokens += chunk.split_whitespace().count() as u64;
            if let Some(last) = state.transcript.messages.last_mut() {
                if last.role == "assistant" {
                    last.content.push_str(&chunk);
                    return;
                }
            }
            state.add_assistant_message(chunk);
        }
        InputEvent::StartLoadingOperation(op) => {
            let op_label = format!("{op:?}");
            state.core.loading_manager.start_operation(op);
            state.core.loading = true;
            state.push_activity(
                crate::app::ActivityKind::Status,
                format!("Loading: {op_label}"),
            );
        }
        InputEvent::EndLoadingOperation(op) => {
            let op_label = format!("{op:?}");
            state.core.loading_manager.end_operation(op);
            state.core.loading = state.core.loading_manager.is_loading();
            state.transcript.streaming.is_streaming = false;
            state.transcript.streaming.start = None;
            state.transcript.streaming.tokens = 0;
            // Stream/LLM turns end here; scan the most recent assistant message
            // for a `<todo>` block and refresh the side-panel surface.
            if let Some(last) = state
                .transcript
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "assistant")
            {
                let extracted = crate::services::todo_extractor::extract_todos(&last.content);
                if !extracted.is_empty() {
                    state.transcript.todos = extracted;
                }
            }
            state.push_activity(
                crate::app::ActivityKind::Status,
                format!("Done: {op_label}"),
            );
        }
        InputEvent::SpeculationReady(prompt, context) => {
            state.speculation.predicted_submit = Some(prompt);
            state.speculation.precomputed_context = context;
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
                .layout
                .toasts
                .push(crate::services::Toast::error(msg.clone()));
            if state.layout.toasts.len() > 3 {
                state
                    .layout
                    .toasts
                    .drain(0..state.layout.toasts.len().saturating_sub(3));
            }
            state.core.loading = false;
            state.transcript.streaming.is_streaming = false;
            state.push_activity(crate::app::ActivityKind::Error, msg);
        }
        InputEvent::SetCurrentModel(model) => {
            state.operator_config.operator.current_model = Some(model);
        }
        InputEvent::AvailableModelsLoaded(models) => {
            state.layout.switchers.available_models = models;
        }
        InputEvent::ValidationResult(score, issues) => {
            state.layout.lsp_ui.validation_score = Some(score);
            state.layout.lsp_ui.validation_issues = issues;
        }
        InputEvent::LspStatus(available, _binary_path) => {
            state.layout.lsp_ui.lsp_available = available;
        }
        InputEvent::LspDiagnostics(snapshot) => {
            state.layout.lsp_ui.lsp_diagnostics = Some(snapshot);
        }
        InputEvent::TaskCancelled => {
            state.push_activity(crate::app::ActivityKind::Status, "Task cancelled");
        }
        InputEvent::ShowBanner(text, style, severity) => {
            let msg =
                crate::services::banner::BannerMessage::new(text, style).with_severity(severity);
            state.layout.banner.queue.push(msg);
            state.layout.banner.message = state.layout.banner.queue.current().cloned();
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
            state.layout.toasts.push(toast);
            if state.layout.toasts.len() > 3 {
                state
                    .layout
                    .toasts
                    .drain(0..state.layout.toasts.len().saturating_sub(3));
            }
        }
        InputEvent::SetSessions(sessions) => {
            state.session.sessions = sessions;
            state.operator_config.operator.sessions_selected_idx = 0;
            state.push_activity(crate::app::ActivityKind::Session, "Sessions updated");
        }
        InputEvent::SetSessionResumeList(entries) => {
            state.layout.session_resume.list = entries;
            state.layout.session_resume.selected = 0;
            crate::handlers::input_popup::refresh_session_resume_filtered(state);
        }
        InputEvent::SetAgentTasks(tasks) => {
            state.execution.runtime.agent_tasks = tasks;
            if state.execution.runtime.agent_selected >= state.execution.runtime.agent_tasks.len() {
                state.execution.runtime.agent_selected =
                    state.execution.runtime.agent_tasks.len().saturating_sub(1);
            }
            state.push_activity(crate::app::ActivityKind::Status, "Agent queue updated");
        }
        InputEvent::SetAgentState(snapshot) => {
            state.execution.runtime.agent_snapshot = snapshot;
            state.push_activity(crate::app::ActivityKind::Status, "Agent state updated");
        }
        InputEvent::SetRuntimeJobs(jobs) => {
            // L2 — per-job signal buffer: push a line for every observed
            // job on each refresh, keyed by job id. Keeps a rolling
            // transition log the MCP signal_tail tool can recall.
            for job in &jobs {
                let buf = state
                    .execution
                    .mcp_maps
                    .runtime_signals
                    .entry(job.id)
                    .or_insert_with(|| {
                        vac_signal::SignalBuffer::new(vac_signal::SignalStreamKind::RuntimeJob, 200)
                    });
                buf.push_line(format!(
                    "{:?} kind={:?} retries={}",
                    job.status, job.kind, job.retry_count
                ));
            }
            state.execution.runtime.jobs = jobs;
            if state.execution.runtime.selected_idx >= state.execution.runtime.jobs.len() {
                state.execution.runtime.selected_idx =
                    state.execution.runtime.jobs.len().saturating_sub(1);
            }
            state.push_activity(crate::app::ActivityKind::Status, "Runtime jobs updated");
        }
        InputEvent::SetRuntimeState(snapshot) => events::on_set_runtime_state(state, snapshot),
        InputEvent::SetTaskGraphProjection(projection) => {
            state.execution.runtime.task_projection = projection;
            state.push_activity(
                crate::app::ActivityKind::Status,
                "Task graph projection updated",
            );
        }
        InputEvent::FileIndexReady(files, bm25_index) => {
            state.workspace.file_index.all_files = files;
            state.workspace.file_index.bm25_index = bm25_index;
            state.workspace.file_index.search_results = state
                .workspace
                .file_index
                .all_files
                .iter()
                .take(50)
                .cloned()
                .collect();
            state.layout.toasts.push(crate::services::Toast::success(
                "File index ready".to_string(),
            ));
        }
        InputEvent::ShellStarted(shell) => {
            let label = if shell.command.trim().is_empty() {
                format!(
                    "shell-{}",
                    state.execution.shell.session_store.sessions.len() + 1
                )
            } else {
                shell.command.clone()
            };
            let idx = state.execution.shell.session_store.push_new(label);
            if let Some(session) = state.execution.shell.session_store.sessions.get_mut(idx) {
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
            state.execution.shell.session_store.popup_visible = true;
            state.push_activity(
                crate::app::ActivityKind::Shell,
                format!("Shell started: {}", shell.command),
            );
        }
        InputEvent::ShellOutput(id, text) => {
            let target = if id == "system" {
                state.execution.shell.session_store.active_mut()
            } else {
                state
                    .execution
                    .shell
                    .session_store
                    .find_by_command_id_mut(&id)
            };
            if let Some(session) = target {
                session.output.push_str(&text);
                session.output_signal.push_chunk(&text);

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
                state.execution.shell.session_store.active_mut()
            } else {
                state
                    .execution
                    .shell
                    .session_store
                    .find_by_command_id_mut(&id)
            };
            if let Some(session) = target {
                if !session.output.ends_with('\n') && !session.output.is_empty() {
                    session.output.push('\n');
                }
                let err_line = format!("[shell error] {text}\n");
                session.output.push_str(&err_line);
                session
                    .output_signal
                    .push_line(format!("[shell error] {text}"));

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
                state.execution.shell.session_store.active_mut()
            } else {
                state
                    .execution
                    .shell
                    .session_store
                    .find_by_command_id_mut(&id)
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
                state.execution.shell.session_store.active_mut()
            } else {
                state
                    .execution
                    .shell
                    .session_store
                    .find_by_command_id_mut(&id)
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
            if conn_state.state == vac_mcp_core::McpConnectionState::Failed {
                let reason = conn_state.reason.clone();
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
            // L5 — also route status into the signal pipeline so
            // `vac signal tail --stream mcp:<name>` can recall it.
            let buf = state
                .execution
                .mcp_maps
                .server_signals
                .entry(name.clone())
                .or_insert_with(|| {
                    vac_signal::SignalBuffer::new(vac_signal::SignalStreamKind::Mcp, 200)
                });
            buf.push_line(format!("status: {:?}", conn_state.state));
            state
                .execution
                .mcp_maps
                .server_states
                .insert(name, conn_state);
        }
        InputEvent::VilStatusUpdated(snapshot) => {
            state.record_vil_score(snapshot.validation_score);
            state.push_vil_log(format!(
                "vil_status score={:.2} issues={}",
                snapshot.validation_score,
                snapshot.validation_issues.len()
            ));
            state.vil_domain.vil.status = snapshot;
            state.push_activity(
                crate::app::ActivityKind::Status,
                "VIL status updated".to_string(),
            );
        }
        InputEvent::ChangesetUpdated => events::on_changeset_updated(state),
        InputEvent::ThemeReloaded(theme) => {
            state.core.theme = theme;
        }
        InputEvent::StartupHydrated(snapshot) => {
            state.core.startup = snapshot;
            state.core.hydrated = true;
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
        InputEvent::SessionSnapshotLoaded(boxed) => {
            // O1 — Apply deferred snapshot then clear the loading flag
            // so the footer spinner dismisses.
            if let Some(snapshot) = boxed.as_ref() {
                crate::session_snapshot::apply_session_snapshot(state, snapshot);
            }
            state.session.session_meta.loading = false;
        }
        InputEvent::ShowConfirmationDialog(tc) => {
            if tc.function.name == crate::services::ask_user::ASK_USER_TOOL_NAME {
                open_ask_user_popup(state, &tc);
                return;
            }
            state.execution.approvals.pending_approvals.push(tc.clone());
            state.execution.approvals.approval_selected_idx = state
                .execution
                .approvals
                .pending_approvals
                .len()
                .saturating_sub(1);
            state
                .execution
                .approvals
                .approval_explanations
                .insert(tc.id.clone(), None);
            state.approval_normalize_selection();
            state.layout.workbench_tab = crate::app::WorkbenchTab::Approvals;
            state.layout.focus = crate::app::WorkspaceFocus::Workbench;
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
            if state.core.view_flags.auto_approve && is_low_risk_tool(&tc.function.name) {
                state.execution.approvals.approved_tools.push(tc.clone());
                let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                state.push_activity(
                    crate::app::ActivityKind::Approval,
                    "Auto-approved low risk tool",
                );
            } else {
                state.execution.approvals.pending_approvals.push(tc.clone());
                state.execution.approvals.approval_selected_idx = state
                    .execution
                    .approvals
                    .pending_approvals
                    .len()
                    .saturating_sub(1);
                state
                    .execution
                    .approvals
                    .approval_explanations
                    .insert(tc.id.clone(), explanation);
                state.approval_normalize_selection();
                state.layout.workbench_tab = crate::app::WorkbenchTab::Approvals;
                state.layout.focus = crate::app::WorkspaceFocus::Workbench;
                state.push_activity(
                    crate::app::ActivityKind::Approval,
                    format!("Approval required: {}", tc.function.name),
                );
            }
        }
        InputEvent::RunToolCall(tc) => {
            state
                .execution
                .approvals
                .pending_tool_calls
                .push(tc.clone());
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
        // T14: vil dev runner events → state + activity panel + task tray
        InputEvent::VilDevEvent(runner_event) => {
            use crate::services::vil_dev_runner::RunnerEvent;
            use vac_runtime::{Job, JobKind, JobStatus};
            match runner_event {
                RunnerEvent::Started { pid } => {
                    state.vil_domain.vil_dev.pid = Some(pid);
                    state.push_activity(
                        crate::app::ActivityKind::Status,
                        format!("vil dev: started (PID {pid})"),
                    );
                    let mut job = Job::new(JobKind::RunTask {
                        description: "vil dev".to_string(),
                    });
                    job.status = JobStatus::Running;
                    state.vil_domain.vil_dev.job_id = Some(job.id);
                    state.execution.runtime.jobs.push(job);
                }
                RunnerEvent::Stdout(line) => {
                    state.vil_domain.vil_dev.output.push_line(line);
                }
                RunnerEvent::Stderr(line) => {
                    let first_line = line.lines().next().unwrap_or(&line).to_string();
                    state
                        .vil_domain
                        .vil_dev
                        .output
                        .push_line(format!("[stderr] {line}"));
                    state.push_activity(
                        crate::app::ActivityKind::Error,
                        format!("vil dev: {first_line}"),
                    );
                }
                RunnerEvent::Checkpoint { session_id, ts } => {
                    state
                        .vil_domain
                        .vil_dev
                        .checkpoints
                        .push((session_id.clone(), ts.clone()));
                    state.push_activity(
                        crate::app::ActivityKind::Status,
                        format!("vil checkpoint: {session_id} @ {ts}"),
                    );
                }
                RunnerEvent::Exited { code, signal } => {
                    state.vil_domain.vil_dev.pid = None;
                    let (msg, is_error) = match (code, signal) {
                        (Some(0), _) => ("vil dev: exited (code 0)".to_string(), false),
                        (Some(c), _) => (format!("vil dev: exited (code {c})"), true),
                        (None, Some(s)) => (format!("vil dev: killed (signal {s})"), true),
                        _ => ("vil dev: exited".to_string(), false),
                    };
                    let kind = if is_error {
                        crate::app::ActivityKind::Error
                    } else {
                        crate::app::ActivityKind::Status
                    };
                    state.push_activity(kind, msg.clone());
                    if let Some(job_id) = state.vil_domain.vil_dev.job_id.take() {
                        if let Some(job) = state
                            .execution
                            .runtime
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == job_id)
                        {
                            job.status = if is_error {
                                JobStatus::Failed(msg)
                            } else {
                                JobStatus::Completed
                            };
                        }
                    }
                }
                RunnerEvent::Error(msg) => {
                    state.push_activity(
                        crate::app::ActivityKind::Error,
                        format!("vil dev: error — {msg}"),
                    );
                    if let Some(job_id) = state.vil_domain.vil_dev.job_id.take() {
                        if let Some(job) = state
                            .execution
                            .runtime
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == job_id)
                        {
                            job.status = JobStatus::Failed(msg.clone());
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
