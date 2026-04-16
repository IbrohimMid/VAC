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
        ctx.state.current_model = Some(selected.clone());
        let _ = ctx.output_tx.try_send(OutputEvent::SwitchToModel(selected.clone()));
        ctx.state.push_activity(
            crate::tui::app::ActivityKind::Status,
            format!("Model switched: {}", selected.name),
        );
        close(ctx)?;
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{AppState, AppStateOptions, OutputEvent};
    use tokio::sync::mpsc;

    fn create_test_context() -> (AppState, mpsc::Sender<OutputEvent>, mpsc::Receiver<OutputEvent>) {
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
    fn test_open_model_switcher() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        
        assert!(open(&mut ctx).is_ok());
        assert!(ctx.state.show_model_switcher);
        assert_eq!(ctx.state.model_switcher_selected_idx, 0);
    }

    #[test]
    fn test_close_model_switcher() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        
        ctx.state.show_model_switcher = true;
        ctx.state.model_switcher_filter = "test".to_string();
        
        assert!(close(&mut ctx).is_ok());
        assert!(!ctx.state.show_model_switcher);
        assert!(ctx.state.model_switcher_filter.is_empty());
    }

    #[test]
    fn test_update_filter() {
        let (mut state, tx, _rx) = create_test_context();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        
        assert!(update_filter(&mut ctx, "gpt".to_string()).is_ok());
        assert_eq!(ctx.state.model_switcher_filter, "gpt");
        assert_eq!(ctx.state.model_switcher_selected_idx, 0);
    }
}
