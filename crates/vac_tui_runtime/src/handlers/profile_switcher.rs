//! Profile switcher popup handler.

use super::{HandlerContext, HandlerResult};
use crate::app::events::OutputEvent;

/// Open profile switcher popup.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.switchers.available_profiles = vec![
        "default".to_string(),
        "strict-vil".to_string(),
        "migration".to_string(),
        "exploration".to_string(),
        "spec-hardening".to_string(),
    ];
    crate::overlay::open_overlay(ctx.state, crate::overlay::OverlayId::ProfileSwitcher);
    ctx.state.switchers.profile_search.clear();
    let filtered = ctx.state.profile_switcher_filtered();
    ctx.state.switchers.profile_selected = filtered
        .iter()
        .position(|p| p == &ctx.state.switchers.active_profile)
        .unwrap_or(0);
    Ok(())
}

/// Close profile switcher popup.
pub fn close(ctx: &mut HandlerContext) -> HandlerResult {
    crate::overlay::close_overlay(ctx.state, crate::overlay::OverlayId::ProfileSwitcher);
    ctx.state.switchers.profile_search.clear();
    ctx.state.switchers.profile_selected = 0;
    Ok(())
}

/// Update filter and refresh results.
pub fn update_filter(ctx: &mut HandlerContext, filter: String) -> HandlerResult {
    ctx.state.switchers.profile_search = filter;
    ctx.state.switchers.profile_selected = 0;
    Ok(())
}

/// Select next profile.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    let filtered = ctx.state.profile_switcher_filtered();
    if !filtered.is_empty() {
        ctx.state.switchers.profile_selected =
            (ctx.state.switchers.profile_selected + 1).min(filtered.len().saturating_sub(1));
    }
    Ok(())
}

/// Select previous profile.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.switchers.profile_selected = ctx.state.switchers.profile_selected.saturating_sub(1);
    Ok(())
}

/// Submit selected profile.
pub fn submit_selected(ctx: &mut HandlerContext) -> HandlerResult {
    let filtered = ctx.state.profile_switcher_filtered();
    if let Some(p) = filtered.get(ctx.state.switchers.profile_selected).cloned() {
        ctx.state.switchers.active_profile = p.clone();
        let _ = ctx
            .output_tx
            .try_send(OutputEvent::SwitchProfile(p.clone()));
        close(ctx)?;
    }
    Ok(())
}
