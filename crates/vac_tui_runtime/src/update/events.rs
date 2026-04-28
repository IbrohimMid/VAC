//! Large event-arm handlers extracted from `update.rs`. Each function
//! corresponds to one `InputEvent` variant and is called directly from the
//! thin top-level dispatcher.

use crate::app::{AppState, InputEvent};
use crate::update::helpers::{
    classify_critical_banner, estimate_context_percent, push_banner_direct, truncate_banner_text,
};

/// `InputEvent::SetRuntimeState` — detect state/environment transitions and
/// record them as activity + toasts before swapping the snapshot.
pub fn on_set_runtime_state(
    state: &mut AppState,
    snapshot: Option<vac_runtime::AutopilotStateFile>,
) {
    // Detect significant state changes for activity logging
    let prev_state = state.execution.runtime.snapshot.as_ref().map(|s| &s.state);
    let new_state = snapshot.as_ref().map(|s| &s.state);

    if let (Some(prev), Some(new)) = (prev_state, new_state) {
        match new {
            vac_runtime::AutopilotState::WaitingApproval { tool_call_id } => {
                if !matches!(prev, vac_runtime::AutopilotState::WaitingApproval { .. }) {
                    state.push_activity(
                        crate::app::ActivityKind::Approval,
                        format!("Runtime waiting for approval: {}", &tool_call_id[..8]),
                    );
                    state
                        .layout
                        .toasts
                        .push(crate::services::Toast::info(format!(
                            "Runtime waiting for approval: {}",
                            &tool_call_id[..8]
                        )));
                }
            }
            vac_runtime::AutopilotState::Backoff { until } => {
                if !matches!(prev, vac_runtime::AutopilotState::Backoff { .. }) {
                    state.push_activity(
                        crate::app::ActivityKind::Status,
                        format!("Runtime entered backoff until {}", until.format("%H:%M:%S")),
                    );
                    state
                        .layout
                        .toasts
                        .push(crate::services::Toast::info(format!(
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
        if let Some(prev_snapshot) = &state.execution.runtime.snapshot {
            if prev_snapshot.execution_environment != new_snapshot.execution_environment {
                let env_name = match new_snapshot.execution_environment {
                    vac_core::ExecutionEnvironment::Host => "host",
                    vac_core::ExecutionEnvironment::IsolatedBatch => "isolated-batch",
                    vac_core::ExecutionEnvironment::IsolatedInteractive => "isolated-interactive",
                };
                state.push_activity(
                    crate::app::ActivityKind::Status,
                    format!("Execution environment switched to {}", env_name),
                );
                state
                    .layout
                    .toasts
                    .push(crate::services::Toast::info(format!(
                        "Switched to {} environment",
                        env_name
                    )));
            }
        }
    }

    state.execution.runtime.snapshot = snapshot;
}

/// `InputEvent::ChangesetUpdated` — kick off a background VIL validation pass
/// over the current changeset and report the result via `VilStatusUpdated`.
pub fn on_changeset_updated(state: &mut AppState) {
    let files = state.workspace.changeset_store.modified_files();
    let project_root = state.core.project_root.clone();
    if let Some(tx) = state.core.input_tx.clone() {
        tokio::spawn(async move {
            if let Ok(pipeline) = vil_ir::IrPipeline::new_async(&project_root).await {
                if let Ok(report) = vil_validate::validate_changes(&pipeline, &files) {
                    let profile = vac_core::detector::VilProjectProfile::detect(&project_root);
                    let mut ir_metadata_files = vec![];
                    for (path, module) in pipeline.modules() {
                        let has_vil_attr = module.structs.iter().any(|s| !s.vil_attrs.is_empty())
                            || module.functions.iter().any(|f| !f.vil_attrs.is_empty());
                        if has_vil_attr {
                            ir_metadata_files.push(path.clone());
                        }
                    }

                    let config =
                        vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
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

/// `InputEvent::SessionRestored` — replace the current session view with the
/// restored messages and zero-out all transient UI state so nothing leaks
/// between sessions.
pub fn on_session_restored(
    state: &mut AppState,
    id: String,
    title: String,
    messages: Vec<crate::app::Message>,
) {
    state.session.session_id = id;
    state.session.session_meta.title = Some(title);
    state.transcript.messages = messages;
    state.core.loading = false;

    // Clear transient UI state to prevent leakage between sessions
    state.execution.approvals.pending_approvals.clear();
    state.execution.approvals.pending_tool_calls.clear();
    state.execution.approvals.approved_tools.clear();
    state.execution.approvals.rejected_tools.clear();
    state.execution.approvals.approval_explanations.clear();
    state.execution.approvals.approval_selected_idx = 0;
    state.execution.approvals.approval_detail_scroll = 0;
    state.execution.approvals.reject_reason_input = None;
    state.composer.at_mention.trigger_active = false;
    state.composer.at_mention.query.clear();
    state.composer.at_mention.results.clear();
    state.transcript.streaming.is_streaming = false;
    state.transcript.streaming.message_id = None;
    state.layout.scroll.messages = 0;
    state.composer.input.clear();
    state.workspace.review.open = false;
    state.workspace.review.filter.clear();
    state.workspace.review.diff = None;
    state.workspace.review.selected_idx = 0;
    state.workspace.review.selected_path = None;
    state.execution.shell = crate::app::ShellState::default();
    state.execution.runtime.jobs.clear();
    state.execution.runtime.selected_idx = 0;
    state.execution.runtime.filter.clear();
    state.execution.runtime.detail_scroll = 0;
    state.execution.runtime.snapshot = None;
    state.execution.activity.clear();
    state.layout.scroll.activity = 0;
    state.layout.toasts.clear();
    crate::overlay::close_overlay(state, crate::overlay::OverlayId::ModelSwitcher);
    state.layout.switchers.model_filter.clear();
    state.layout.switchers.model_selected = 0;
    crate::overlay::close_overlay(state, crate::overlay::OverlayId::FileSearch);
    state.workspace.file_index.search_query.clear();
    state.workspace.file_index.search_selected_idx = 0;
    state.workspace.file_index.search_results.clear();
    crate::overlay::close_overlay(state, crate::overlay::OverlayId::Changeset);
    state.workspace.changeset_ui.selected_idx = 0;
    state.workspace.changeset_ui.diff_scroll = 0;
    state.workspace.changeset_ui.selected_path = None;
    state.workspace.changeset_store.clear();
    state.workspace.modified_files = state.workspace.changeset_store.modified_files(); // derived: empty after clear
    state.workspace.changeset_ui.diff = None;
    state.vil_domain.vil.score_history.clear();
    state.vil_domain.vil.event_log.clear();
    state.vil_domain.vil.last_score = None;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Approvals;
    state.layout.focus = crate::app::WorkspaceFocus::Input;
    state.push_activity(crate::app::ActivityKind::Session, "Session restored");
}

/// `InputEvent::TaskCompleted` — record token usage, surface modified/created
/// files to the changeset store, and refresh any open review panes.
pub fn on_task_completed(state: &mut AppState, result: vac_core::task::TaskResult) {
    state.vil_domain.vil.last_score = result.validation_score;
    // Real usage wiring: `vac_core::task::TaskResult.total_tokens_used`
    // is the authoritative producer. We record this turn's total, add
    // to session running total, and derive a coarse context %.
    let turn_tokens = result.total_tokens_used;
    state.operator_config.billing.current_message = crate::app::TokenUsage {
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: turn_tokens,
    };
    state.operator_config.billing.total_session.total_tokens = state
        .operator_config
        .billing
        .total_session
        .total_tokens
        .saturating_add(turn_tokens);
    state.operator_config.billing.context_usage_percent = estimate_context_percent(
        state.operator_config.operator.current_model.as_ref(),
        turn_tokens,
    );

    let mut content = result.summary.clone();
    let mut changeset_updated = false;
    if !result.modified_files.is_empty() {
        content.push_str("\n\n**Modified Files**:\n");
        for file in &result.modified_files {
            content.push_str(&format!("- `{}`\n", file));
            state
                .workspace
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
                .workspace
                .changeset_store
                .file_created(file.clone(), "agent".to_string());
            changeset_updated = true;
        }
    }
    // Sync derived view from store (single source of truth)
    state.workspace.modified_files = state.workspace.changeset_store.modified_files();

    if changeset_updated {
        if let Some(tx) = state.core.input_tx.clone() {
            let _ = tx.try_send(InputEvent::ChangesetUpdated);
        }
    }

    if state.workspace.review.open {
        state.workspace.review.generation = state.workspace.review.generation.saturating_add(1);
        state.review_sync_items();
        state.review_normalize_selection();
    }
    state.add_assistant_message(content);
    state.core.loading = false;
    state.transcript.streaming.is_streaming = false;
    state.push_activity(crate::app::ActivityKind::Status, "Task completed");
}

/// `InputEvent::ToolResult` — reconcile pending/approved pools, surface error
/// banners, and route VIL-flavoured tool logs.
pub fn on_tool_result(state: &mut AppState, result: crate::types::ToolCallResult) {
    state
        .execution
        .approvals
        .pending_tool_calls
        .retain(|c| c.id != result.call.id);
    state
        .execution
        .approvals
        .approved_tools
        .retain(|c| c.id != result.call.id);
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
    if result.status == crate::types::ToolCallResultStatus::Success {
        let name = result.call.function.name.as_str();
        if name == "enter_plan_mode" {
            state
                .workspace
                .plan
                .mode_active
                .store(true, std::sync::atomic::Ordering::SeqCst);
        } else if name == "exit_plan_mode" {
            state
                .workspace
                .plan
                .mode_active
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    state.add_assistant_message(result.result);
    state.push_activity(
        crate::app::ActivityKind::Tool,
        format!("Tool result: {}", result.call.function.name),
    );
    if super::helpers::is_vil_tool(&result.call.function.name) {
        let status = match result.status {
            crate::types::ToolCallResultStatus::Success => "ok",
            crate::types::ToolCallResultStatus::Error => "error",
            crate::types::ToolCallResultStatus::Pending => "pending",
        };
        state.push_vil_log(format!("tool {status} {}", result.call.function.name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::types::{FunctionCall, ToolCall, ToolCallResult, ToolCallResultStatus};

    #[test]
    fn test_on_tool_result_updates_plan_mode() {
        let mut state = AppState::default();
        assert!(!state.workspace.plan.mode_active.load(std::sync::atomic::Ordering::SeqCst));

        let mut result = ToolCallResult {
            call: ToolCall {
                id: "1".into(),
                r#type: "function".into(),
                function: FunctionCall {
                    name: "enter_plan_mode".into(),
                    arguments: "{}".into(),
                },
                metadata: None,
            },
            result: "entered".into(),
            status: ToolCallResultStatus::Success,
            envelope: None,
        };

        on_tool_result(&mut state, result.clone());
        assert!(state.workspace.plan.mode_active.load(std::sync::atomic::Ordering::SeqCst));

        result.call.function.name = "exit_plan_mode".into();
        on_tool_result(&mut state, result);
        assert!(!state.workspace.plan.mode_active.load(std::sync::atomic::Ordering::SeqCst));
    }
}
