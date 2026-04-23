//! Rulebook switcher popup handler.

use super::{HandlerContext, HandlerResult};
use crate::app::events::OutputEvent;

/// Open rulebook switcher popup.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    // Load available rulebooks on open
    let config =
        vac_core::VacConfig::load_with_fallback(&ctx.state.core.project_root).unwrap_or_default();
    let books = vac_core::rulebook::RulebookLoader::load_all(
        &ctx.state.core.project_root,
        &config.rulebook.paths,
    );
    ctx.state.layout.switchers.available_rulebooks = books
        .into_iter()
        .map(|b| crate::types::ListRuleBook {
            id: b.id.clone(),
            name: b.name.unwrap_or_else(|| b.id.clone()),
            description: None,
            tags: vec![],
        })
        .collect();

    crate::overlay::open_overlay(ctx.state, crate::overlay::OverlayId::RulebookSwitcher);
    ctx.state.layout.switchers.rulebook_search.clear();
    let filtered = ctx.state.rulebook_switcher_filtered();
    ctx.state.layout.switchers.rulebook_selected =
        if let Some(active) = ctx.state.layout.switchers.selected_rulebooks.iter().next() {
            filtered.iter().position(|r| &r.id == active).unwrap_or(0)
        } else {
            0
        };
    Ok(())
}

/// Close rulebook switcher popup.
pub fn close(ctx: &mut HandlerContext) -> HandlerResult {
    crate::overlay::close_overlay(ctx.state, crate::overlay::OverlayId::RulebookSwitcher);
    ctx.state.layout.switchers.rulebook_search.clear();
    ctx.state.layout.switchers.rulebook_selected = 0;
    Ok(())
}

/// Update filter and refresh results.
pub fn update_filter(ctx: &mut HandlerContext, filter: String) -> HandlerResult {
    ctx.state.layout.switchers.rulebook_search = filter;
    ctx.state.layout.switchers.rulebook_selected = 0;
    Ok(())
}

/// Toggle selected rulebook on space.
pub fn toggle_selected(ctx: &mut HandlerContext) -> HandlerResult {
    let filtered = ctx.state.rulebook_switcher_filtered();
    if let Some(r) = filtered.get(ctx.state.layout.switchers.rulebook_selected) {
        if ctx.state.layout.switchers.selected_rulebooks.contains(&r.id) {
            ctx.state.layout.switchers.selected_rulebooks.remove(&r.id);
        } else {
            ctx.state.layout.switchers.selected_rulebooks.insert(r.id.clone());
        }
    }
    Ok(())
}

/// Select next rulebook.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    let filtered = ctx.state.rulebook_switcher_filtered();
    if !filtered.is_empty() {
        ctx.state.layout.switchers.rulebook_selected =
            (ctx.state.layout.switchers.rulebook_selected + 1).min(filtered.len().saturating_sub(1));
    }
    Ok(())
}

/// Select previous rulebook.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.layout.switchers.rulebook_selected = ctx.state.layout.switchers.rulebook_selected.saturating_sub(1);
    Ok(())
}

/// Submit selected rulebooks.
pub fn submit_selected(ctx: &mut HandlerContext) -> HandlerResult {
    let selected: Vec<String> = ctx.state.layout.switchers.selected_rulebooks.iter().cloned().collect();
    let _ = ctx
        .output_tx
        .try_send(OutputEvent::ApplyRulebooks(selected));
    close(ctx)?;
    Ok(())
}
