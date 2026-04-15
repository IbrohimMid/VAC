//! File search handler for fuzzy file navigation.

use super::{HandlerContext, HandlerResult};
use crate::tui::services::{build_file_index, fuzzy_search_files, Toast};

/// Open file search popup.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    if ctx.state.all_files.is_empty() {
        ctx.state.all_files = build_file_index(&ctx.state.project_root);
    }
    ctx.state.show_file_search = true;
    ctx.state.file_search_query.clear();
    ctx.state.file_search_results.clear();
    ctx.state.file_search_selected_idx = 0;
    Ok(())
}

/// Close file search popup.
pub fn close(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.show_file_search = false;
    ctx.state.file_search_query.clear();
    ctx.state.file_search_results.clear();
    ctx.state.file_search_selected_idx = 0;
    Ok(())
}

/// Update search query and refresh results.
pub fn update_query(ctx: &mut HandlerContext, query: String) -> HandlerResult {
    ctx.state.file_search_query = query.clone();
    if query.is_empty() {
        ctx.state.file_search_results.clear();
    } else {
        ctx.state.file_search_results =
            fuzzy_search_files(&query, &ctx.state.all_files, 50);
    }
    ctx.state.file_search_selected_idx = 0;
    Ok(())
}

/// Select next result.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    if !ctx.state.file_search_results.is_empty() {
        ctx.state.file_search_selected_idx = (ctx.state.file_search_selected_idx + 1)
            .min(ctx.state.file_search_results.len().saturating_sub(1));
    }
    Ok(())
}

/// Select previous result.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.file_search_selected_idx =
        ctx.state.file_search_selected_idx.saturating_sub(1);
    Ok(())
}

/// Insert selected file path into input.
pub fn insert_selected(ctx: &mut HandlerContext) -> HandlerResult {
    if let Some(path) = ctx
        .state
        .file_search_results
        .get(ctx.state.file_search_selected_idx)
    {
        ctx.state.input.insert_str(path);
        ctx.state
            .toasts
            .push(Toast::info(format!("Inserted: {}", path)));
        close(ctx)?;
    }
    Ok(())
}
