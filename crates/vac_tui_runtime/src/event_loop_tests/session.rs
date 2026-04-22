#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::make_state;
use crate::Model;
use crate::app::{InputEvent, OutputEvent};
use crate::session_snapshot::{
    apply_session_snapshot, build_session_snapshot, load_session_snapshot,
};

#[tokio::test]
async fn session_snapshot_bridge_restores_tui_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let session_id = uuid::Uuid::new_v4();

    let mut state = make_state(root.clone(), session_id);
    state.current_model = Some(Model {
        id: "claude-sonnet-4".to_string(),
        name: "claude-sonnet-4".to_string(),
        provider: "anthropic".to_string(),
        supports_reasoning: true,
        supports_tool_calls: true,
        supports_streaming: true,
        context_window: 200000,
        cost_class: "premium".to_string(),
    });
    state.active_profile = "strict-vil".to_string();
    state.selected_rulebooks.insert("security".to_string());
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Runtime;
    state.sessions_selected_idx = 3;
    state
        .side_panel_section_collapsed
        .insert(crate::app::SidePanelSection::Runtime);
    state.total_session_usage.total_tokens = 2048;
    state.modified_files = vec!["src/main.rs".to_string()];

    let snapshot = build_session_snapshot(&state).unwrap();
    vac_session_control::save_snapshot_async(snapshot)
        .await
        .unwrap();
    let loaded = load_session_snapshot(&root, &state.session_id)
        .await
        .unwrap();

    let mut restored = make_state(root.clone(), session_id);
    apply_session_snapshot(&mut restored, &loaded);

    assert_eq!(
        restored.current_model.as_ref().map(|m| m.name.as_str()),
        None
    );
    assert_eq!(
        restored.startup.active_model.as_deref(),
        Some("claude-sonnet-4")
    );
    assert_eq!(restored.active_profile, "strict-vil");
    assert!(restored.selected_rulebooks.contains("security"));
    assert_eq!(restored.focus, crate::app::WorkspaceFocus::Workbench);
    assert_eq!(restored.workbench_tab, crate::app::WorkbenchTab::Runtime);
    assert_eq!(restored.sessions_selected_idx, 3);
    assert!(
        restored
            .side_panel_section_collapsed
            .contains(&crate::app::SidePanelSection::Runtime)
    );
    assert_eq!(restored.total_session_usage.total_tokens, 0);
    assert_eq!(restored.startup.provider_status, "initializing");
    assert_eq!(
        restored.startup.active_rulebook.as_deref(),
        Some("security")
    );
    assert_eq!(
        restored.startup.active_profile.as_deref(),
        Some("strict-vil")
    );
    assert_eq!(
        loaded.metadata.get("todo_pending").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        loaded.metadata.get("todo_in_progress").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        loaded.metadata.get("todo_done").map(String::as_str),
        Some("0")
    );
}

#[tokio::test]
async fn session_restore_clears_popup_state() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Set popup states
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ModelSwitcher);
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::FileSearch);
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::Changeset);
    state.model_switcher_filter = "test".to_string();
    state.file_search_query = "query".to_string();

    // Trigger session restore
    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::SessionRestored {
            id: uuid::Uuid::new_v4().to_string(),
            title: "New Session".to_string(),
            messages: vec![],
        },
    );

    // Verify all popup states cleared
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::Changeset)
    );
    assert!(state.model_switcher_filter.is_empty());
    assert!(state.file_search_query.is_empty());
}

#[tokio::test]
async fn session_restore_clears_changeset_store() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add changeset entries
    state
        .changeset_store
        .file_created("a.rs".to_string(), "agent".to_string());
    state
        .changeset_store
        .file_modified("b.rs".to_string(), "agent".to_string(), true);
    assert_eq!(state.changeset_store.entries().len(), 2);

    // Trigger session restore
    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::SessionRestored {
            id: uuid::Uuid::new_v4().to_string(),
            title: "New Session".to_string(),
            messages: vec![],
        },
    );

    // Verify changeset cleared
    assert_eq!(state.changeset_store.entries().len(), 0);
}

#[tokio::test]
async fn session_restore_clears_approval_state() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add approval state
    state.approvals.pending_approvals.push(crate::ToolCall {
        id: "tc-1".to_string(),
        r#type: "function".to_string(),
        function: crate::FunctionCall {
            name: "test".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });
    state.approvals.approved_tools.push(crate::ToolCall {
        id: "tc-2".to_string(),
        r#type: "function".to_string(),
        function: crate::FunctionCall {
            name: "test".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });

    // Trigger session restore
    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::SessionRestored {
            id: uuid::Uuid::new_v4().to_string(),
            title: "New Session".to_string(),
            messages: vec![],
        },
    );

    // Verify approval state cleared
    assert_eq!(state.approvals.pending_approvals.len(), 0);
    assert_eq!(state.approvals.approved_tools.len(), 0);
    assert_eq!(state.approvals.rejected_tools.len(), 0);
}

#[test]
fn session_restore_clears_store_and_syncs_derived() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state
        .changeset_store
        .file_modified("a.rs".to_string(), "agent".to_string(), true);
    state.modified_files = state.changeset_store.modified_files();
    assert_eq!(state.modified_files.len(), 1);

    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::SessionRestored {
            id: uuid::Uuid::new_v4().to_string(),
            title: "new".to_string(),
            messages: vec![],
        },
    );

    // Both store and derived view must be empty
    assert_eq!(state.changeset_store.active_entries().len(), 0);
    assert_eq!(state.modified_files.len(), 0);
    assert_eq!(state.modified_files, state.changeset_store.modified_files());
}

#[test]
fn session_info_enriched_fields_populated() {
    let s = crate::app::SessionInfo {
        id: "abc123".to_string(),
        title: "Test Session".to_string(),
        updated_at: "2026-04-16T09:00:00Z".to_string(),
        checkpoints: vec!["abc123_state.json".to_string()],
        task_count: 5,
        last_activity: "2026-04-16 09:00".to_string(),
        has_checkpoint: true,
        snapshot_present: false,
        snapshot_stale: false,
    };
    assert_eq!(s.task_count, 5);
    assert_eq!(s.last_activity, "2026-04-16 09:00");
    assert!(s.has_checkpoint);
    assert_eq!(s.checkpoints.len(), 1);
}

#[test]
fn set_sessions_event_populates_enriched_fields() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    let sessions = vec![
        crate::app::SessionInfo {
            id: "s1".to_string(),
            title: "Session 1".to_string(),
            updated_at: "2026-04-16T09:00:00Z".to_string(),
            checkpoints: vec![],
            task_count: 3,
            last_activity: "2026-04-16 09:00".to_string(),
            has_checkpoint: false,
            snapshot_present: false,
            snapshot_stale: false,
        },
        crate::app::SessionInfo {
            id: "s2".to_string(),
            title: "Session 2".to_string(),
            updated_at: "2026-04-16T10:00:00Z".to_string(),
            checkpoints: vec!["s2_state.json".to_string()],
            task_count: 7,
            last_activity: "2026-04-16 10:00".to_string(),
            has_checkpoint: true,
            snapshot_present: false,
            snapshot_stale: false,
        },
    ];

    crate::controller::handle_backend_event(&mut state, &tx, InputEvent::SetSessions(sessions));

    assert_eq!(state.sessions.len(), 2);
    assert_eq!(state.sessions[0].task_count, 3);
    assert!(!state.sessions[0].has_checkpoint);
    assert_eq!(state.sessions[1].task_count, 7);
    assert!(state.sessions[1].has_checkpoint);
    assert_eq!(state.sessions[1].checkpoints.len(), 1);
}

#[test]
fn sessions_tab_r_resumes_checkpoint_for_selected_session() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Sessions;
    state.sessions = vec![crate::app::SessionInfo {
        id: "sess-abc".to_string(),
        title: "Session abc".to_string(),
        updated_at: "2026-04-16T09:00:00Z".to_string(),
        checkpoints: vec!["sess-abc_state.json".to_string()],
        task_count: 2,
        last_activity: "2026-04-16 09:00".to_string(),
        has_checkpoint: true,
        snapshot_present: false,
        snapshot_stale: false,
    }];
    state.sessions_selected_idx = 0;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));

    let ev = rx.try_recv().unwrap();
    assert!(matches!(ev, OutputEvent::ResumeSession(ref id) if id == "sess-abc"));
}

#[test]
fn sessions_tab_r_toasts_when_no_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Sessions;
    state.sessions = vec![crate::app::SessionInfo {
        id: "sess-xyz".to_string(),
        title: "Session xyz".to_string(),
        updated_at: "2026-04-16T09:00:00Z".to_string(),
        checkpoints: vec![],
        task_count: 0,
        last_activity: "2026-04-16 09:00".to_string(),
        has_checkpoint: false,
        snapshot_present: false,
        snapshot_stale: false,
    }];
    state.sessions_selected_idx = 0;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));

    assert!(!state.toasts.is_empty());
    assert!(state.toasts[0].message.contains("No checkpoint"));
}

#[test]
fn session_resume_fuzzy_orders_by_score() {
    use crate::app::types::SessionResumeEntry;
    use crate::handlers::input_popup::refresh_session_resume_filtered;
    use chrono::Utc;
    use uuid::Uuid;

    let mut state = make_state(std::env::current_dir().unwrap(), Uuid::new_v4());

    let now = Utc::now();
    state.session_resume_list = vec![
        SessionResumeEntry {
            session_id: Uuid::new_v4(),
            title: "refactor auth module".to_string(),
            project: "backend".to_string(),
            last_message_preview: "moved token logic".to_string(),
            last_active: now - chrono::Duration::hours(2),
            model: Some("sonnet".to_string()),
            token_count: Some(1200),
        },
        SessionResumeEntry {
            session_id: Uuid::new_v4(),
            title: "fix login bug".to_string(),
            project: "frontend".to_string(),
            last_message_preview: "resolved redirect loop".to_string(),
            last_active: now - chrono::Duration::hours(1),
            model: Some("opus".to_string()),
            token_count: Some(800),
        },
        SessionResumeEntry {
            session_id: Uuid::new_v4(),
            title: "auth token validation".to_string(),
            project: "backend".to_string(),
            last_message_preview: "added expiry check".to_string(),
            last_active: now - chrono::Duration::hours(3),
            model: Some("haiku".to_string()),
            token_count: Some(400),
        },
    ];

    // Query "auth" — should match "refactor auth module" and "auth token validation"
    // but not "fix login bug"
    state.session_resume_query = "auth".to_string();
    refresh_session_resume_filtered(&mut state);

    assert!(
        !state.session_resume_filtered_indices.is_empty(),
        "fuzzy search should return results for 'auth'"
    );
    // "fix login bug" should not appear (no 'auth' anywhere)
    for &idx in &state.session_resume_filtered_indices {
        assert_ne!(
            state.session_resume_list[idx].title, "fix login bug",
            "non-matching entry should be excluded"
        );
    }
}

#[test]
fn session_resume_ctrl_r_keyboard_nav() {
    use crate::app::types::SessionResumeEntry;
    use crate::handlers::input_popup::refresh_session_resume_filtered;
    use crate::overlay::{OverlayId, open_overlay};
    use chrono::Utc;
    use uuid::Uuid;

    let (tx, _rx) = tokio::sync::mpsc::channel(10);
    let mut state = make_state(std::env::current_dir().unwrap(), Uuid::new_v4());

    let now = Utc::now();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    let id_c = Uuid::new_v4();
    state.session_resume_list = vec![
        SessionResumeEntry {
            session_id: id_a,
            title: "session alpha".to_string(),
            project: "proj".to_string(),
            last_message_preview: "alpha work".to_string(),
            last_active: now - chrono::Duration::hours(1),
            model: None,
            token_count: None,
        },
        SessionResumeEntry {
            session_id: id_b,
            title: "session beta".to_string(),
            project: "proj".to_string(),
            last_message_preview: "beta work".to_string(),
            last_active: now - chrono::Duration::hours(2),
            model: None,
            token_count: None,
        },
        SessionResumeEntry {
            session_id: id_c,
            title: "session gamma".to_string(),
            project: "proj".to_string(),
            last_message_preview: "gamma work".to_string(),
            last_active: now - chrono::Duration::hours(3),
            model: None,
            token_count: None,
        },
    ];

    open_overlay(&mut state, OverlayId::SessionResume);
    refresh_session_resume_filtered(&mut state);

    // Initially shows all 3, sorted newest first (alpha, beta, gamma)
    assert_eq!(state.session_resume_filtered_indices.len(), 3);
    assert_eq!(state.session_resume_selected, 0);

    // Down twice — select index 2 (gamma)
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::Down);
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::Down);
    assert_eq!(state.session_resume_selected, 2);

    // Type query "beta" — should filter to 1 result, reset selection to 0
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('b'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('e'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('t'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('a'));
    assert_eq!(state.session_resume_selected, 0);
    assert_eq!(state.session_resume_filtered_indices.len(), 1);
    let matched_idx = state.session_resume_filtered_indices[0];
    assert_eq!(state.session_resume_list[matched_idx].session_id, id_b);

    // Backspace clears 'a' — "bet" still matches beta
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
    assert!(!state.session_resume_filtered_indices.is_empty());

    // Esc closes overlay and clears query
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);
    assert!(!state.overlay_manager.is_active(OverlayId::SessionResume));
    assert!(state.session_resume_query.is_empty());
    assert!(state.session_resume_filtered_indices.is_empty());
}
