//! Approval handler for tool execution approvals.

use super::{HandlerContext, HandlerResult};
use crate::tui::app::{ActivityKind, OutputEvent};
use crate::tui::services::Toast;

/// Open approval workbench tab.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.workbench_tab = crate::tui::app::WorkbenchTab::Approvals;
    ctx.state.focus = crate::tui::app::WorkspaceFocus::Workbench;
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
        ctx.state.push_activity(
            ActivityKind::Approval,
            format!("Approved: {}", tool_name),
        );
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
        let _ = ctx.output_tx.try_send(OutputEvent::RejectTool(tc, false));
        ctx.state.push_activity(
            ActivityKind::Approval,
            format!("Rejected: {}", tool_name),
        );
        ctx.state
            .toasts
            .push(Toast::error(format!("Rejected: {}", tool_name)));
    }
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
