//! Approval handler for tool execution approvals.

use super::{HandlerContext, HandlerResult};
use crate::app::{ActivityKind, OutputEvent};
use crate::services::Toast;

/// Open approval workbench tab.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.layout.workbench_tab = crate::app::WorkbenchTab::Approvals;
    ctx.state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    Ok(())
}

/// Select next approval in queue.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    if !ctx.state.execution.approvals.pending_approvals.is_empty() {
        ctx.state.execution.approvals.approval_selected_idx =
            (ctx.state.execution.approvals.approval_selected_idx + 1).min(
                ctx.state
                    .execution
                    .approvals
                    .pending_approvals
                    .len()
                    .saturating_sub(1),
            );
        ctx.state.execution.approvals.approval_detail_scroll = 0;
    }
    Ok(())
}

/// Select previous approval in queue.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.execution.approvals.approval_selected_idx = ctx
        .state
        .execution
        .approvals
        .approval_selected_idx
        .saturating_sub(1);
    ctx.state.execution.approvals.approval_detail_scroll = 0;
    Ok(())
}

/// Approve currently selected tool.
pub fn approve_current(ctx: &mut HandlerContext) -> HandlerResult {
    if let Some(tc) = ctx
        .state
        .execution
        .approvals
        .pending_approvals
        .get(ctx.state.execution.approvals.approval_selected_idx)
        .cloned()
    {
        ctx.state
            .execution
            .approvals
            .pending_approvals
            .retain(|t| t.id != tc.id);
        ctx.state
            .execution
            .approvals
            .approval_explanations
            .remove(&tc.id);
        ctx.state
            .execution
            .approvals
            .approved_tools
            .push(tc.clone());
        ctx.state.approval_normalize_selection();

        let tool_name = tc.function.name.clone();
        let _ = ctx.output_tx.try_send(OutputEvent::AcceptTool(tc));
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Approved: {}", tool_name));
        ctx.state
            .layout
            .toasts
            .push(Toast::success(format!("Approved: {}", tool_name)));
    }
    Ok(())
}

/// Reject currently selected tool.
pub fn reject_current(ctx: &mut HandlerContext) -> HandlerResult {
    if let Some(tc) = ctx
        .state
        .execution
        .approvals
        .pending_approvals
        .get(ctx.state.execution.approvals.approval_selected_idx)
        .cloned()
    {
        ctx.state
            .execution
            .approvals
            .pending_approvals
            .retain(|t| t.id != tc.id);
        ctx.state
            .execution
            .approvals
            .approval_explanations
            .remove(&tc.id);
        ctx.state
            .execution
            .approvals
            .rejected_tools
            .push(tc.clone());
        ctx.state.approval_normalize_selection();

        let tool_name = tc.function.name.clone();
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::RejectTool(tc, false, None));
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Rejected: {}", tool_name));
        ctx.state
            .layout
            .toasts
            .push(Toast::error(format!("Rejected: {}", tool_name)));
    }
    Ok(())
}

/// R2.b — Toggle the bulk-selected flag on the currently indexed
/// approval. Keyed by `ToolCall.id` so the selection survives row
/// reordering (new call arriving, single-approve shifts indices).
pub fn bulk_toggle_current(ctx: &mut HandlerContext) -> HandlerResult {
    let idx = ctx.state.execution.approvals.approval_selected_idx;
    ctx.state.execution.approvals.bulk_toggle(idx);
    Ok(())
}

/// R2.b — Clear the entire bulk selection (without approving).
pub fn bulk_clear(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.execution.approvals.bulk_clear();
    Ok(())
}

/// R2.b — Approve every tool currently in the bulk selection. Drains
/// selection through the same per-call `AcceptTool` pipeline single
/// approve uses, so recorder + output bus stay consistent.
pub fn bulk_approve(ctx: &mut HandlerContext) -> HandlerResult {
    let picked = ctx.state.execution.approvals.drain_bulk_selection();
    if picked.is_empty() {
        return Ok(());
    }
    for tc in &picked {
        ctx.state
            .execution
            .approvals
            .pending_approvals
            .retain(|t| t.id != tc.id);
        ctx.state
            .execution
            .approvals
            .approval_explanations
            .remove(&tc.id);
        ctx.state
            .execution
            .approvals
            .approved_tools
            .push(tc.clone());
        let tool_name = tc.function.name.clone();
        let _ = ctx.output_tx.try_send(OutputEvent::AcceptTool(tc.clone()));
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Approved: {tool_name}"));
    }
    ctx.state.approval_normalize_selection();
    ctx.state
        .layout
        .toasts
        .push(Toast::success(format!("Approved {} tools", picked.len())));
    Ok(())
}

/// R2.b — Reject every tool currently in the bulk selection.
pub fn bulk_reject(ctx: &mut HandlerContext) -> HandlerResult {
    let picked = ctx.state.execution.approvals.drain_bulk_selection();
    if picked.is_empty() {
        return Ok(());
    }
    for tc in &picked {
        ctx.state
            .execution
            .approvals
            .pending_approvals
            .retain(|t| t.id != tc.id);
        ctx.state
            .execution
            .approvals
            .approval_explanations
            .remove(&tc.id);
        ctx.state
            .execution
            .approvals
            .rejected_tools
            .push(tc.clone());
        let tool_name = tc.function.name.clone();
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::RejectTool(tc.clone(), false, None));
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Rejected: {tool_name}"));
    }
    ctx.state.approval_normalize_selection();
    ctx.state
        .layout
        .toasts
        .push(Toast::error(format!("Rejected {} tools", picked.len())));
    Ok(())
}

/// Approve all pending tools at once.
pub fn approve_all(ctx: &mut HandlerContext) -> HandlerResult {
    let tools: Vec<_> = ctx
        .state
        .execution
        .approvals
        .pending_approvals
        .drain(..)
        .collect();
    if tools.is_empty() {
        return Ok(());
    }
    for tc in tools {
        ctx.state
            .execution
            .approvals
            .approval_explanations
            .remove(&tc.id);
        let tool_name = tc.function.name.clone();
        let _ = ctx.output_tx.try_send(OutputEvent::AcceptTool(tc.clone()));
        ctx.state.execution.approvals.approved_tools.push(tc);
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Approved: {}", tool_name));
    }
    ctx.state.execution.approvals.approval_selected_idx = 0;
    ctx.state
        .layout
        .toasts
        .push(Toast::success("All tools approved".to_string()));
    Ok(())
}

/// Activate reject reason prompt for current tool.
pub fn begin_reject_current(ctx: &mut HandlerContext) -> HandlerResult {
    if !ctx.state.execution.approvals.pending_approvals.is_empty() {
        crate::overlay::open_overlay(ctx.state, crate::overlay::OverlayId::RejectReason);
    }
    Ok(())
}

/// Append char to reject reason input.
pub fn reason_input_push(ctx: &mut HandlerContext, c: char) -> HandlerResult {
    if let Some(r) = &mut ctx.state.execution.approvals.reject_reason_input {
        r.push(c);
    }
    Ok(())
}

/// Delete last char from reject reason input.
pub fn reason_input_pop(ctx: &mut HandlerContext) -> HandlerResult {
    if let Some(r) = &mut ctx.state.execution.approvals.reject_reason_input {
        r.pop();
    }
    Ok(())
}

/// Confirm reason and reject current tool.
pub fn confirm_reject_current(ctx: &mut HandlerContext) -> HandlerResult {
    let reason = ctx.state.execution.approvals.reject_reason_input.take();
    ctx.state
        .layout
        .overlay_manager
        .pop(crate::overlay::OverlayId::RejectReason);
    if let Some(tc) = ctx
        .state
        .execution
        .approvals
        .pending_approvals
        .get(ctx.state.execution.approvals.approval_selected_idx)
        .cloned()
    {
        ctx.state
            .execution
            .approvals
            .pending_approvals
            .retain(|t| t.id != tc.id);
        ctx.state
            .execution
            .approvals
            .approval_explanations
            .remove(&tc.id);
        ctx.state
            .execution
            .approvals
            .rejected_tools
            .push(tc.clone());
        ctx.state.approval_normalize_selection();
        let tool_name = tc.function.name.clone();
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::RejectTool(tc, false, reason));
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Rejected: {}", tool_name));
        ctx.state
            .layout
            .toasts
            .push(Toast::error(format!("Rejected: {}", tool_name)));
    }
    Ok(())
}

/// Confirm reason and reject all pending tools.
pub fn confirm_reject_all(ctx: &mut HandlerContext) -> HandlerResult {
    let reason = ctx.state.execution.approvals.reject_reason_input.take();
    ctx.state
        .layout
        .overlay_manager
        .pop(crate::overlay::OverlayId::RejectReason);
    let tools: Vec<_> = ctx
        .state
        .execution
        .approvals
        .pending_approvals
        .drain(..)
        .collect();
    if tools.is_empty() {
        return Ok(());
    }
    for tc in tools {
        ctx.state
            .execution
            .approvals
            .approval_explanations
            .remove(&tc.id);
        let tool_name = tc.function.name.clone();
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::RejectTool(tc.clone(), false, reason.clone()));
        ctx.state.execution.approvals.rejected_tools.push(tc);
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Rejected: {}", tool_name));
    }
    ctx.state.execution.approvals.approval_selected_idx = 0;
    ctx.state
        .layout
        .toasts
        .push(Toast::error("All tools rejected".to_string()));
    Ok(())
}

/// Toggle auto-approve mode.
pub fn toggle_auto_approve(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.core.view_flags.auto_approve = !ctx.state.core.view_flags.auto_approve;
    let status = if ctx.state.core.view_flags.auto_approve {
        "enabled"
    } else {
        "disabled"
    };
    ctx.state
        .layout
        .toasts
        .push(Toast::info(format!("Auto-approve {}", status)));
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::app::{AppState, AppStateOptions, OutputEvent};
    use tokio::sync::mpsc;

    fn create_test_context() -> (
        AppState,
        mpsc::Sender<OutputEvent>,
        mpsc::Receiver<OutputEvent>,
    ) {
        let state = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });
        let (tx, rx) = mpsc::channel(10);
        (state, tx, rx)
    }

    #[test]
    fn test_open_approval() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        assert!(open(&mut ctx).is_ok());
        assert_eq!(
            ctx.state.layout.workbench_tab,
            crate::app::WorkbenchTab::Approvals
        );
        assert_eq!(
            ctx.state.layout.focus,
            crate::app::WorkspaceFocus::Workbench
        );
    }

    #[test]
    fn test_toggle_auto_approve() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        let initial = ctx.state.core.view_flags.auto_approve;
        assert!(toggle_auto_approve(&mut ctx).is_ok());
        assert_eq!(ctx.state.core.view_flags.auto_approve, !initial);

        assert!(toggle_auto_approve(&mut ctx).is_ok());
        assert_eq!(ctx.state.core.view_flags.auto_approve, initial);
    }

    #[test]
    fn test_select_next_empty_queue() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        assert!(select_next(&mut ctx).is_ok());
        assert_eq!(ctx.state.execution.approvals.approval_selected_idx, 0);
    }

    // ── J4 — Approval 1-keypress contract ────────────────────────────
    //
    // Invariant (Claude-Code-friction rule): approving the first pending
    // tool happens in a single `approve_current` call — no confirmation
    // step, no intermediate modal. Pressing `a` once moves the tool from
    // `pending_approvals` to `approved_tools`.

    use crate::types::{FunctionCall, ToolCall};

    fn push_pending(ctx: &mut HandlerContext<'_>, id: &str, name: &str) {
        ctx.state
            .execution
            .approvals
            .pending_approvals
            .push(ToolCall {
                id: id.to_string(),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: name.to_string(),
                    arguments: "{}".to_string(),
                },
                metadata: None,
            });
    }

    #[test]
    fn contract_approve_current_is_single_step() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        push_pending(&mut ctx, "tc-1", "file_read");

        assert_eq!(ctx.state.execution.approvals.pending_approvals.len(), 1);
        assert_eq!(ctx.state.execution.approvals.approved_tools.len(), 0);

        // ONE call ⇒ fully approved, no intermediate state.
        assert!(approve_current(&mut ctx).is_ok());

        assert_eq!(ctx.state.execution.approvals.pending_approvals.len(), 0);
        assert_eq!(ctx.state.execution.approvals.approved_tools.len(), 1);
        assert_eq!(ctx.state.execution.approvals.approved_tools[0].id, "tc-1");
    }

    #[test]
    fn contract_approve_on_empty_queue_is_noop() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        assert!(approve_current(&mut ctx).is_ok());
        assert_eq!(ctx.state.execution.approvals.approved_tools.len(), 0);
    }

    #[test]
    fn contract_approve_all_clears_queue_in_single_call() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        push_pending(&mut ctx, "tc-1", "file_read");
        push_pending(&mut ctx, "tc-2", "grep");
        push_pending(&mut ctx, "tc-3", "bash");

        assert!(approve_all(&mut ctx).is_ok());
        assert_eq!(ctx.state.execution.approvals.pending_approvals.len(), 0);
        assert_eq!(ctx.state.execution.approvals.approved_tools.len(), 3);
    }

    // ── M3 — Reject-reason modal cancelability contract ─────────────
    //
    // Invariant: opening the reject-reason prompt and then dismissing
    // it (via overlay close, i.e. Esc handler) must leave the tool in
    // `pending_approvals` — neither approved nor rejected. Confirms
    // the modal is non-destructive until Enter.

    #[test]
    fn contract_reject_reason_cancel_leaves_tool_pending() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        push_pending(&mut ctx, "tc-9", "bash");

        assert!(begin_reject_current(&mut ctx).is_ok());
        assert!(
            ctx.state
                .layout
                .overlay_manager
                .is_active(crate::overlay::OverlayId::RejectReason)
        );
        assert!(ctx.state.execution.approvals.reject_reason_input.is_some());

        // Cancel via close_overlay (what the Esc key handler invokes).
        crate::overlay::close_overlay(ctx.state, crate::overlay::OverlayId::RejectReason);

        assert!(
            !ctx.state
                .layout
                .overlay_manager
                .is_active(crate::overlay::OverlayId::RejectReason)
        );
        assert!(ctx.state.execution.approvals.reject_reason_input.is_none());
        assert_eq!(ctx.state.execution.approvals.pending_approvals.len(), 1);
        assert_eq!(ctx.state.execution.approvals.rejected_tools.len(), 0);
        assert_eq!(ctx.state.execution.approvals.approved_tools.len(), 0);
    }

    #[test]
    fn contract_reject_reason_reopening_starts_fresh() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        push_pending(&mut ctx, "tc-10", "bash");

        assert!(begin_reject_current(&mut ctx).is_ok());
        let _ = reason_input_push(&mut ctx, 't');
        let _ = reason_input_push(&mut ctx, 'o');
        let _ = reason_input_push(&mut ctx, 'o');
        assert_eq!(
            ctx.state.execution.approvals.reject_reason_input.as_deref(),
            Some("too")
        );

        crate::overlay::close_overlay(ctx.state, crate::overlay::OverlayId::RejectReason);
        assert!(ctx.state.execution.approvals.reject_reason_input.is_none());

        // Reopening yields an empty buffer, not the dismissed one.
        assert!(begin_reject_current(&mut ctx).is_ok());
        assert_eq!(
            ctx.state.execution.approvals.reject_reason_input.as_deref(),
            Some("")
        );
    }
}
