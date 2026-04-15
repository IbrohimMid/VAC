//! Model switcher handler for runtime model selection.

use super::{HandlerContext, HandlerResult};
use crate::tui::app::OutputEvent;

/// Open model switcher popup.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.show_model_switcher = true;
    ctx.state.model_switcher_filter.clear();
    ctx.state.model_switcher_selected_idx = 0;
    Ok(())
}

/// Close model switcher popup.
pub fn close(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.show_model_switcher = false;
    ctx.state.model_switcher_filter.clear();
    ctx.state.model_switcher_selected_idx = 0;
    Ok(())
}

/// Update filter and refresh results.
pub fn update_filter(ctx: &mut HandlerContext, filter: String) -> HandlerResult {
    ctx.state.model_switcher_filter = filter;
    ctx.state.model_switcher_selected_idx = 0;
    Ok(())
}

/// Select next model.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    let filtered = ctx.state.model_switcher_filtered();
    if !filtered.is_empty() {
        ctx.state.model_switcher_selected_idx = (ctx.state.model_switcher_selected_idx + 1)
            .min(filtered.len().saturating_sub(1));
    }
    Ok(())
}

/// Select previous model.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.model_switcher_selected_idx = ctx
        .state
        .model_switcher_selected_idx
        .saturating_sub(1);
    Ok(())
}

/// Submit selected model.
pub fn submit_selected(ctx: &mut HandlerContext) -> HandlerResult {
    let filtered = ctx.state.model_switcher_filtered();
    if let Some(selected) = filtered.get(ctx.state.model_switcher_selected_idx).cloned() {
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::SwitchToModel(selected));
        close(ctx)?;
    }
    Ok(())
}
