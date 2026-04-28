#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::make_state;
use crate::app::{InputEvent, OutputEvent};
use crate::controller::classify_critical_banner;

#[test]
fn critical_banner_mcp_tls_is_blocking() {
    let msg = "Failed to connect MCP server 'github': MCP error: Failed to build HTTP client for MCP server 'github': error setting certificate verify locations";
    let out = classify_critical_banner(msg);
    assert_eq!(
        out,
        Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Blocking
        ))
    );
}

#[test]
fn critical_banner_governance_block_is_blocking() {
    let msg = "Tool execution error: Warden blocked: process substitution $() not allowed";
    let out = classify_critical_banner(msg);
    assert_eq!(
        out,
        Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Blocking
        ))
    );
}

#[test]
fn counter_consistency_header_tab_popup() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state
        .workspace
        .changeset_store
        .file_modified("x.rs".to_string(), "agent".to_string(), true);
    state
        .workspace
        .changeset_store
        .file_created("y.rs".to_string(), "agent".to_string());
    state
        .workspace
        .changeset_store
        .file_modified("z.rs".to_string(), "agent".to_string(), true);
    state.workspace.changeset_store.revert_success("z.rs"); // reverted: not active

    let active = state.workspace.changeset_store.active_entries().len();
    // All three surfaces must read the same count
    assert_eq!(active, 2); // x.rs + y.rs; z.rs is reverted
    // review_filtered_paths also driven by active_entries
    state.review_sync_items();
    assert_eq!(state.review_filtered_paths().len(), 2);
}

#[test]
fn task_completed_does_not_dual_write() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    let result = vac_core::task::TaskResult {
        task_id: vac_core::task::TaskId::new(),
        status: vac_core::task::TaskStatus::Completed,
        summary: "done".to_string(),
        modified_files: vec!["src/lib.rs".to_string()],
        created_files: vec!["src/new.rs".to_string()],
        validation_score: None,
        elapsed_ms: 0,
        total_tokens_used: 0,
        agent_contributions: vec![],
    };
    crate::controller::handle_backend_event(&mut state, &tx, InputEvent::TaskCompleted(result));

    // modified_files must equal store's derived view - no independent writes
    assert_eq!(
        state.workspace.modified_files,
        state.workspace.changeset_store.modified_files()
    );
    assert_eq!(state.workspace.changeset_store.active_entries().len(), 2);
}

#[test]
fn show_model_switcher_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowModelSwitcher);
    assert!(
        state
            .layout
            .overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
    assert!(state.layout.switchers.model_filter.is_empty());
}

#[test]
fn show_file_search_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowFileSearch);
    assert!(
        state
            .layout
            .overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
    assert!(state.workspace.file_index.search_query.is_empty());
}

#[test]
fn show_changeset_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowChangeset);
    assert!(
        state
            .layout
            .overlay_manager
            .is_active(crate::overlay::OverlayId::Changeset)
    );
}

#[tokio::test]
async fn runtime_tab_requests_refresh_on_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Sessions;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

    assert_eq!(state.layout.workbench_tab, crate::app::WorkbenchTab::Agents);
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::ListAgentTasks
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::LoadAgentState
    ));

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

    assert_eq!(
        state.layout.workbench_tab,
        crate::app::WorkbenchTab::Runtime
    );
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::ListRuntimeJobs
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::LoadRuntimeState
    ));
}
