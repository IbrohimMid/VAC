//! Review handler for file change review and revert operations.

use super::{HandlerContext, HandlerResult};
use crate::tui::app::{ActivityKind, ReviewItemStatus};
use crate::tui::services::review;

/// Open review workstation.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.review_open = true;
    ctx.state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
    ctx.state.focus = crate::tui::app::WorkspaceFocus::Workbench;
    ctx.state.review_generation = ctx.state.review_generation.saturating_add(1);
    ctx.state.review_sync_items();
    ctx.state.review_normalize_selection();
    Ok(())
}

/// Close review workstation.
pub fn close(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.review_open = false;
    ctx.state.review_diff = None;
    Ok(())
}

/// Select next file in review.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    let prev = ctx.state.review_selected_path.clone();
    ctx.state.review_select_by_delta(1);
    if ctx.state.review_diff.is_some() && prev != ctx.state.review_selected_path {
        load_diff_for_selected(ctx)?;
    }
    Ok(())
}

/// Select previous file in review.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    let prev = ctx.state.review_selected_path.clone();
    ctx.state.review_select_by_delta(-1);
    if ctx.state.review_diff.is_some() && prev != ctx.state.review_selected_path {
        load_diff_for_selected(ctx)?;
    }
    Ok(())
}

/// Revert selected file.
pub fn revert_selected(ctx: &mut HandlerContext) -> HandlerResult {
    let Some(path) = ctx.state.review_selected_path.clone() else {
        return Ok(());
    };
    let Ok(session_id) = uuid::Uuid::parse_str(&ctx.state.session_id) else {
        ctx.state
            .add_assistant_message("Invalid session id; cannot restore snapshot.".to_string());
        return Ok(());
    };

    match vac_tools::journal::restore_snapshot(&ctx.state.project_root, session_id, &path) {
        Ok(()) => {
            ctx.state.changeset_store.revert_success(&path);
            ctx.state.modified_files = ctx.state.changeset_store.modified_files();
            ctx.state.review_items.entry(path.clone()).and_modify(|it| {
                it.status = ReviewItemStatus::Restored;
                it.last_error = None;
                it.dirty_generation = it.dirty_generation.saturating_add(1);
            });
            ctx.state
                .add_assistant_message(format!("Reverted file: {}", path));
            ctx.state
                .push_activity(ActivityKind::Review, format!("Reverted: {path}"));
        }
        Err(e) => {
            ctx.state.changeset_store.revert_failed(&path, e.clone());
            ctx.state.review_items.entry(path.clone()).and_modify(|it| {
                it.status = ReviewItemStatus::Failed;
                it.last_error = Some(e.clone());
                it.dirty_generation = it.dirty_generation.saturating_add(1);
            });
            ctx.state
                .add_assistant_message(format!("Failed to revert file: {}", path));
            ctx.state
                .push_activity(ActivityKind::Review, format!("Revert failed: {path}"));
        }
    }

    ctx.state.review_generation = ctx.state.review_generation.saturating_add(1);
    ctx.state.review_sync_items();
    ctx.state.review_normalize_selection();
    Ok(())
}

/// Revert all filtered files.
pub fn revert_filtered(ctx: &mut HandlerContext) -> HandlerResult {
    let files: Vec<String> = ctx
        .state
        .review_filtered_paths()
        .into_iter()
        .filter(|p| ctx.state.changeset_store.active_entries().iter().any(|e| &e.path == p))
        .collect();

    if files.is_empty() {
        ctx.state
            .add_assistant_message("No files to revert.".to_string());
        return Ok(());
    }

    let Ok(session_id) = uuid::Uuid::parse_str(&ctx.state.session_id) else {
        ctx.state
            .add_assistant_message("Invalid session id; cannot restore snapshot.".to_string());
        return Ok(());
    };

    let mut success_count = 0usize;
    for file in &files {
        match vac_tools::journal::restore_snapshot(&ctx.state.project_root, session_id, file) {
            Ok(()) => {
                success_count += 1;
                ctx.state.changeset_store.revert_success(file);
                ctx.state.review_items.entry(file.clone()).and_modify(|it| {
                    it.status = ReviewItemStatus::Restored;
                    it.last_error = None;
                    it.dirty_generation = it.dirty_generation.saturating_add(1);
                });
            }
            Err(e) => {
                ctx.state.changeset_store.revert_failed(file, e.clone());
                ctx.state.review_items.entry(file.clone()).and_modify(|it| {
                    it.status = ReviewItemStatus::Failed;
                    it.last_error = Some(e);
                    it.dirty_generation = it.dirty_generation.saturating_add(1);
                });
            }
        }
    }
    ctx.state.modified_files = ctx.state.changeset_store.modified_files();

    ctx.state.add_assistant_message(format!(
        "Reverted {}/{} files.",
        success_count,
        files.len()
    ));
    ctx.state.push_activity(
        ActivityKind::Review,
        format!("Reverted filtered: {success_count}/{}", files.len()),
    );
    ctx.state.review_generation = ctx.state.review_generation.saturating_add(1);
    ctx.state.review_sync_items();
    ctx.state.review_normalize_selection();
    Ok(())
}

/// Revert all modified files.
pub fn revert_all(ctx: &mut HandlerContext) -> HandlerResult {
    let files = ctx.state.changeset_store.modified_files();
    if files.is_empty() {
        ctx.state
            .add_assistant_message("No files to revert.".to_string());
        return Ok(());
    }

    let Ok(session_id) = uuid::Uuid::parse_str(&ctx.state.session_id) else {
        ctx.state
            .add_assistant_message("Invalid session id; cannot restore snapshot.".to_string());
        return Ok(());
    };

    let mut success_count = 0usize;
    for file in &files {
        match vac_tools::journal::restore_snapshot(&ctx.state.project_root, session_id, file) {
            Ok(()) => {
                success_count += 1;
                ctx.state.changeset_store.revert_success(file);
                ctx.state.review_items.entry(file.clone()).and_modify(|it| {
                    it.status = ReviewItemStatus::Restored;
                    it.last_error = None;
                    it.dirty_generation = it.dirty_generation.saturating_add(1);
                });
            }
            Err(e) => {
                ctx.state.changeset_store.revert_failed(file, e.clone());
                ctx.state.review_items.entry(file.clone()).and_modify(|it| {
                    it.status = ReviewItemStatus::Failed;
                    it.last_error = Some(e);
                    it.dirty_generation = it.dirty_generation.saturating_add(1);
                });
            }
        }
    }

    ctx.state.modified_files = ctx.state.changeset_store.modified_files();
    ctx.state.review_diff = None;
    ctx.state.review_selected_idx = 0;
    ctx.state.review_selected_path = None;
    ctx.state.add_assistant_message(format!(
        "Reverted {}/{} files.",
        success_count,
        files.len()
    ));
    ctx.state.push_activity(
        ActivityKind::Review,
        format!("Reverted all: {success_count}/{}", files.len()),
    );
    ctx.state.review_generation = ctx.state.review_generation.saturating_add(1);
    ctx.state.review_sync_items();
    ctx.state.review_normalize_selection();
    Ok(())
}

/// Load diff for currently selected file.
fn load_diff_for_selected(ctx: &mut HandlerContext) -> HandlerResult {
    if let (Some(path), Ok(session_id)) = (
        ctx.state.review_selected_path.clone(),
        uuid::Uuid::parse_str(&ctx.state.session_id),
    ) {
        match review::load_diff(&ctx.state.project_root, session_id, &path) {
            Ok(diff) => {
                ctx.state.review_diff = Some(crate::tui::app::ReviewDiffState {
                    path: diff.path,
                    old_content: Some(diff.old_content),
                    new_content: Some(diff.new_content),
                    scroll: 0,
                    last_error: None,
                });
            }
            Err(e) => {
                ctx.state.review_diff = Some(crate::tui::app::ReviewDiffState {
                    path: path.clone(),
                    old_content: None,
                    new_content: None,
                    scroll: 0,
                    last_error: Some(e),
                });
            }
        }
    }
    Ok(())
}
