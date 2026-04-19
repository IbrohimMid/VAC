//! Approval handler for tool execution approvals.

use super::{HandlerContext, HandlerResult};
use crate::app::{ActivityKind, OutputEvent};
use crate::services::Toast;

/// Open approval workbench tab.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.workbench_tab = crate::app::WorkbenchTab::Approvals;
    ctx.state.focus = crate::app::WorkspaceFocus::Workbench;
    Ok(())
}

/// Select next approval in queue.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    if !ctx.state.pending_approvals.is_empty() {
        ctx.state.approval_selected_idx = (ctx.state.approval_selected_idx + 1)
            .min(ctx.state.pending_approvals.len().saturating_sub(1));
        ctx.state.approval_detail_scroll = 0;
    }
    Ok(())
}

/// Select previous approval in queue.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.approval_selected_idx = ctx.state.approval_selected_idx.saturating_sub(1);
    ctx.state.approval_detail_scroll = 0;
    Ok(())
}

/// Approve currently selected tool.
pub fn approve_current(ctx: &mut HandlerContext) -> HandlerResult {
    if let Some(tc) = ctx
        .state
        .pending_approvals
        .get(ctx.state.approval_selected_idx)
        .cloned()
    {
        ctx.state.pending_approvals.retain(|t| t.id != tc.id);
        ctx.state.approval_explanations.remove(&tc.id);
        ctx.state.approved_tools.push(tc.clone());
        ctx.state.approval_normalize_selection();

        let tool_name = tc.function.name.clone();
        let _ = ctx.output_tx.try_send(OutputEvent::AcceptTool(tc));
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Approved: {}", tool_name));
        ctx.state
            .toasts
            .push(Toast::success(format!("Approved: {}", tool_name)));
    }
    Ok(())
}

/// Reject currently selected tool.
pub fn reject_current(ctx: &mut HandlerContext) -> HandlerResult {
    if let Some(tc) = ctx
        .state
        .pending_approvals
        .get(ctx.state.approval_selected_idx)
        .cloned()
    {
        ctx.state.pending_approvals.retain(|t| t.id != tc.id);
        ctx.state.approval_explanations.remove(&tc.id);
        ctx.state.rejected_tools.push(tc.clone());
        ctx.state.approval_normalize_selection();

        let tool_name = tc.function.name.clone();
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::RejectTool(tc, false, None));
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Rejected: {}", tool_name));
        ctx.state
            .toasts
            .push(Toast::error(format!("Rejected: {}", tool_name)));
    }
    Ok(())
}

/// Approve all pending tools at once.
pub fn approve_all(ctx: &mut HandlerContext) -> HandlerResult {
    let tools: Vec<_> = ctx.state.pending_approvals.drain(..).collect();
    if tools.is_empty() {
        return Ok(());
    }
    for tc in tools {
        ctx.state.approval_explanations.remove(&tc.id);
        let tool_name = tc.function.name.clone();
        let _ = ctx.output_tx.try_send(OutputEvent::AcceptTool(tc.clone()));
        ctx.state.approved_tools.push(tc);
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Approved: {}", tool_name));
    }
    ctx.state.approval_selected_idx = 0;
    ctx.state
        .toasts
        .push(Toast::success("All tools approved".to_string()));
    Ok(())
}

/// Activate reject reason prompt for current tool.
pub fn begin_reject_current(ctx: &mut HandlerContext) -> HandlerResult {
    if !ctx.state.pending_approvals.is_empty() {
        ctx.state.reject_reason_input = Some(String::new());
    }
    Ok(())
}

/// Append char to reject reason input.
pub fn reason_input_push(ctx: &mut HandlerContext, c: char) -> HandlerResult {
    if let Some(r) = &mut ctx.state.reject_reason_input {
        r.push(c);
    }
    Ok(())
}

/// Delete last char from reject reason input.
pub fn reason_input_pop(ctx: &mut HandlerContext) -> HandlerResult {
    if let Some(r) = &mut ctx.state.reject_reason_input {
        r.pop();
    }
    Ok(())
}

/// Confirm reason and reject current tool.
pub fn confirm_reject_current(ctx: &mut HandlerContext) -> HandlerResult {
    let reason = ctx.state.reject_reason_input.take();
    if let Some(tc) = ctx
        .state
        .pending_approvals
        .get(ctx.state.approval_selected_idx)
        .cloned()
    {
        ctx.state.pending_approvals.retain(|t| t.id != tc.id);
        ctx.state.approval_explanations.remove(&tc.id);
        ctx.state.rejected_tools.push(tc.clone());
        ctx.state.approval_normalize_selection();
        let tool_name = tc.function.name.clone();
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::RejectTool(tc, false, reason));
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Rejected: {}", tool_name));
        ctx.state
            .toasts
            .push(Toast::error(format!("Rejected: {}", tool_name)));
    }
    Ok(())
}

/// Confirm reason and reject all pending tools.
pub fn confirm_reject_all(ctx: &mut HandlerContext) -> HandlerResult {
    let reason = ctx.state.reject_reason_input.take();
    let tools: Vec<_> = ctx.state.pending_approvals.drain(..).collect();
    if tools.is_empty() {
        return Ok(());
    }
    for tc in tools {
        ctx.state.approval_explanations.remove(&tc.id);
        let tool_name = tc.function.name.clone();
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::RejectTool(tc.clone(), false, reason.clone()));
        ctx.state.rejected_tools.push(tc);
        ctx.state
            .push_activity(ActivityKind::Approval, format!("Rejected: {}", tool_name));
    }
    ctx.state.approval_selected_idx = 0;
    ctx.state
        .toasts
        .push(Toast::error("All tools rejected".to_string()));
    Ok(())
}

/// Toggle auto-approve mode.
pub fn toggle_auto_approve(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.auto_approve = !ctx.state.auto_approve;
    let status = if ctx.state.auto_approve {
        "enabled"
    } else {
        "disabled"
    };
    ctx.state
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
            ctx.state.workbench_tab,
            crate::app::WorkbenchTab::Approvals
        );
        assert_eq!(ctx.state.focus, crate::app::WorkspaceFocus::Workbench);
    }

    #[test]
    fn test_toggle_auto_approve() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        let initial = ctx.state.auto_approve;
        assert!(toggle_auto_approve(&mut ctx).is_ok());
        assert_eq!(ctx.state.auto_approve, !initial);

        assert!(toggle_auto_approve(&mut ctx).is_ok());
        assert_eq!(ctx.state.auto_approve, initial);
    }

    #[test]
    fn test_select_next_empty_queue() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        assert!(select_next(&mut ctx).is_ok());
        assert_eq!(ctx.state.approval_selected_idx, 0);
    }
}
