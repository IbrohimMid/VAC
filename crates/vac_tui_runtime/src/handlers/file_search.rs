//! File search handler for fuzzy file navigation.

use super::{HandlerContext, HandlerResult};
use crate::services::{Toast, build_file_index, fuzzy_search_files};

/// Open file search popup.
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    if ctx.state.all_files.is_empty() {
        if let Some(tx) = ctx.state.input_tx.clone() {
            let root = ctx.state.project_root.clone();
            tokio::spawn(async move {
                let files = build_file_index(&root);
                let _ = tx
                    .send(crate::app::events::InputEvent::FileIndexReady(files))
                    .await;
            });
            ctx.state
                .toasts
                .push(Toast::info("Indexing files in background...".to_string()));
        } else {
            ctx.state.all_files = build_file_index(&ctx.state.project_root);
        }
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
        ctx.state.file_search_results = fuzzy_search_files(&query, &ctx.state.all_files, 50);
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
    ctx.state.file_search_selected_idx = ctx.state.file_search_selected_idx.saturating_sub(1);
    Ok(())
}

/// Insert selected file path into input.
pub fn insert_selected(ctx: &mut HandlerContext) -> HandlerResult {
    if let Some(path) = ctx
        .state
        .file_search_results
        .get(ctx.state.file_search_selected_idx)
    {
        let path = path.clone();
        ctx.state.input.insert_str(&path);
        ctx.state.focus = crate::app::WorkspaceFocus::Input;
        ctx.state
            .toasts
            .push(Toast::info(format!("Inserted: {}", path)));
        close(ctx)?;
    }
    Ok(())
}

#[cfg(test)]
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
    fn test_open_file_search() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        assert!(open(&mut ctx).is_ok());
        assert!(ctx.state.show_file_search);
        assert_eq!(ctx.state.file_search_selected_idx, 0);
    }

    #[test]
    fn test_close_file_search() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        ctx.state.show_file_search = true;
        ctx.state.file_search_query = "test".to_string();

        assert!(close(&mut ctx).is_ok());
        assert!(!ctx.state.show_file_search);
        assert!(ctx.state.file_search_query.is_empty());
    }

    #[test]
    fn test_update_query() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);

        assert!(update_query(&mut ctx, "main.rs".to_string()).is_ok());
        assert_eq!(ctx.state.file_search_query, "main.rs");
        assert_eq!(ctx.state.file_search_selected_idx, 0);
    }
}
