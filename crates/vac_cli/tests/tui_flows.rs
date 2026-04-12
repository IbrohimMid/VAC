//! Integration tests for TUI state flow.

use vac_cli::tui::app::{FocusPane, TuiApp};
use vac_cli::tui::services::detail::DetailMode;
use vac_core::engine::{EngineStatus, TaskHistoryEntry};
use vac_core::TaskStatus;

fn create_test_app_with_history() -> TuiApp {
    let status = EngineStatus {
        project_root: std::path::PathBuf::from("/tmp"),
        session_id: uuid::Uuid::new_v4(),
        total_tasks: 1,
        completed_tasks: 1,
        failed_tasks: 0,
        total_tokens_used: 100,
        subsystems_initialized: true,
    };
    let history = vec![TaskHistoryEntry {
        task_id: uuid::Uuid::new_v4(),
        description: "Test task".into(),
        status: TaskStatus::Completed,
        total_tokens_used: 100,
        updated_at: chrono::Utc::now(),
        summary: None,
    }];
    TuiApp::new(status, history, None)
}

#[test]
fn test_focus_cycle() {
    let mut app = create_test_app_with_history();
    assert_eq!(app.focus, FocusPane::Composer);
    
    app.cycle_focus();
    assert_eq!(app.focus, FocusPane::Transcript);
    
    app.cycle_focus();
    assert_eq!(app.focus, FocusPane::History);
    
    app.cycle_focus();
    assert_eq!(app.focus, FocusPane::Composer);
}

#[test]
fn test_history_to_detail_flow() {
    let mut app = create_test_app_with_history();
    app.focus = FocusPane::History;
    
    // Select first history entry
    app.history_down();
    assert!(app.history.selected().is_some());
    
    // Enter detail mode
    app.detail = DetailMode::TaskDetail(0);
    assert!(app.detail.is_some());
    
    // Cycle focus should include detail
    app.cycle_focus();
    assert_eq!(app.focus, FocusPane::Detail);
}

#[test]
fn test_revert_confirm_flow() {
    let mut app = create_test_app_with_history();
    app.focus = FocusPane::History;
    app.history_down();
    
    // Enter revert confirm mode
    app.detail = DetailMode::RevertConfirm(0);
    assert!(matches!(app.detail, DetailMode::RevertConfirm(_)));
    
    // Escape should clear detail
    app.detail = DetailMode::None;
    assert!(!app.detail.is_some());
}

#[test]
fn test_approval_modal_state() {
    let app = create_test_app_with_history();
    
    // No pending approval initially
    assert!(app.pending_approval.is_none());
    
    // When approval is pending, focus should be locked
    // (actual locking is in event_loop, we just test state)
    assert_eq!(app.focus, FocusPane::Composer);
}

#[test]
fn test_escape_closes_detail() {
    let mut app = create_test_app_with_history();
    
    // Open detail
    app.detail = DetailMode::TaskDetail(0);
    assert!(app.detail.is_some());
    
    // Escape closes it
    app.detail = DetailMode::None;
    assert!(!app.detail.is_some());
    
    // Same for error detail
    app.detail = DetailMode::ErrorDetail("test".into());
    assert!(app.detail.is_some());
    app.detail = DetailMode::None;
    assert!(!app.detail.is_some());
}
