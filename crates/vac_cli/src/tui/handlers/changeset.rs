//! Changeset handler for file change tracking and review.

use super::{HandlerContext, HandlerResult};
use crate::tui::services::review;

/// Open changeset popup.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.show_changeset = true;
    ctx.state.changeset_selected_idx = 0;
    ctx.state.changeset_diff_scroll = 0;

    // Load diff for first entry if available
    let entries = ctx.state.changeset_store.entries();
    if let Some(entry) = entries.first() {
        if let Ok(session_id) = uuid::Uuid::parse_str(&ctx.state.session_id) {
            match review::load_diff(&ctx.state.project_root, session_id, &entry.path) {
                Ok(diff) => {
                    ctx.state.changeset_selected_path = Some(entry.path.clone());
                    ctx.state.changeset_diff = Some(crate::tui::app::ReviewDiffState {
                        path: diff.path,
                        old_content: Some(diff.old_content),
                        new_content: Some(diff.new_content),
                        scroll: 0,
                        last_error: None,
                    });
                }
                Err(e) => {
                    ctx.state.changeset_selected_path = Some(entry.path.clone());
                    ctx.state.changeset_diff = Some(crate::tui::app::ReviewDiffState {
                        path: entry.path.clone(),
                        old_content: None,
                        new_content: None,
                        scroll: 0,
                        last_error: Some(e),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Close changeset popup.
pub fn close(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.show_changeset = false;
    ctx.state.changeset_selected_idx = 0;
    ctx.state.changeset_diff_scroll = 0;
    ctx.state.changeset_selected_path = None;
    ctx.state.changeset_diff = None;
    Ok(())
}

/// Select next file in changeset.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    let entries = ctx.state.changeset_store.entries();
    if !entries.is_empty() {
        ctx.state.changeset_selected_idx = (ctx.state.changeset_selected_idx + 1)
            .min(entries.len().saturating_sub(1));
        load_diff_for_selected(ctx)?;
    }
    Ok(())
}

/// Select previous file in changeset.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.changeset_selected_idx = ctx.state.changeset_selected_idx.saturating_sub(1);
    load_diff_for_selected(ctx)?;
    Ok(())
}

/// Scroll diff preview down.
pub fn scroll_down(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.changeset_diff_scroll = ctx.state.changeset_diff_scroll.saturating_add(1);
    Ok(())
}

/// Scroll diff preview up.
pub fn scroll_up(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.changeset_diff_scroll = ctx.state.changeset_diff_scroll.saturating_sub(1);
    Ok(())
}

/// Load diff for currently selected file.
fn load_diff_for_selected(ctx: &mut HandlerContext) -> HandlerResult {
    let entries = ctx.state.changeset_store.entries();
    if let Some(entry) = entries.get(ctx.state.changeset_selected_idx) {
        if let Ok(session_id) = uuid::Uuid::parse_str(&ctx.state.session_id) {
            match review::load_diff(&ctx.state.project_root, session_id, &entry.path) {
                Ok(diff) => {
                    ctx.state.changeset_selected_path = Some(entry.path.clone());
                    ctx.state.changeset_diff = Some(crate::tui::app::ReviewDiffState {
                        path: diff.path,
                        old_content: Some(diff.old_content),
                        new_content: Some(diff.new_content),
                        scroll: 0,
                        last_error: None,
                    });
                }
                Err(e) => {
                    ctx.state.changeset_selected_path = Some(entry.path.clone());
                    ctx.state.changeset_diff = Some(crate::tui::app::ReviewDiffState {
                        path: entry.path.clone(),
                        old_content: None,
                        new_content: None,
                        scroll: 0,
                        last_error: Some(e),
                    });
                }
            }
        }
    }
    Ok(())
}
