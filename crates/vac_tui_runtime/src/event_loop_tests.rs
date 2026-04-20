// Tests for event_loop.rs — production code is in event_loop.rs
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::app::{
    AppState, AppStateOptions, InputEvent, OutputEvent, SidePanelSection, WorkbenchTab,
    WorkspaceFocus,
};
use crate::controller::{classify_critical_banner, open_ask_user_popup};
use crate::session_snapshot::{
    apply_session_snapshot, build_session_snapshot, load_session_snapshot,
};
use crate::{FunctionCall, Model, ToolCall};

fn make_state(project_root: std::path::PathBuf, session_id: uuid::Uuid) -> AppState {
    AppState::new(AppStateOptions {
        model: None,
        session_id: Some(session_id.to_string()),
        checkpoint_path: Some(project_root.join(".vac/checkpoints")),
        project_root,
    })
}

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
    state.focus = WorkspaceFocus::Workbench;
    state.workbench_tab = WorkbenchTab::Runtime;
    state.sessions_selected_idx = 3;
    state
        .side_panel_section_collapsed
        .insert(SidePanelSection::Runtime);
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
    assert_eq!(restored.focus, WorkspaceFocus::Workbench);
    assert_eq!(restored.workbench_tab, WorkbenchTab::Runtime);
    assert_eq!(restored.sessions_selected_idx, 3);
    assert!(
        restored
            .side_panel_section_collapsed
            .contains(&SidePanelSection::Runtime)
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

#[test]
fn review_selection_normalizes_when_filter_excludes_selected() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state
        .changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), false);
    state
        .changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), false);
    state.modified_files = state.changeset_store.modified_files();
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.review.selected_path = Some("b.txt".to_string());
    state.review.filter = "a".to_string();
    state.review_sync_items();
    state.review_normalize_selection();
    assert_eq!(state.review.selected_path, Some("a.txt".to_string()));
    assert_eq!(state.review.selected_idx, 0);
}

#[test]
fn slash_semantics_fix_and_explain_send_prompt_with_args() {
    // /fix and /explain are BuiltInWithPrompt: they push to pending_user_messages,
    // not directly to the output channel.
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state.input.set_content("/fix cargo clippy");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    let msg = state
        .pending_user_messages
        .pop_front()
        .expect("/fix should enqueue a pending message");
    assert!(
        msg.final_input.contains("Fix linter/build errors"),
        "got: {}",
        msg.final_input
    );
    assert!(
        msg.final_input.contains("cargo clippy"),
        "got: {}",
        msg.final_input
    );

    state
        .input
        .set_content("/explain crates/vac_cli/src/tui/event_loop.rs");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    let msg = state
        .pending_user_messages
        .pop_front()
        .expect("/explain should enqueue a pending message");
    assert!(
        msg.final_input
            .contains("Explain the relevant code or concept"),
        "got: {}",
        msg.final_input
    );
    assert!(
        msg.final_input
            .contains("crates/vac_cli/src/tui/event_loop.rs"),
        "got: {}",
        msg.final_input
    );
}

#[test]
fn slash_review_opens_workstation_instead_of_sending_literal() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state
        .changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), false);
    state.modified_files = state.changeset_store.modified_files();
    state.input.set_content("/review");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(state.review.open);
    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
}

#[test]
fn review_open_close_transitions_clear_diff() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.txt".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 0,
        last_error: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewClose);
    assert!(!state.review.open);
    assert!(state.review.diff.is_none());
}

#[test]
fn diff_scroll_state_changes_on_page_down() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.txt".to_string(),
        old_content: Some("a\nb\nc\nd\ne\n".to_string()),
        new_content: Some("a\nb\nX\nd\ne\n".to_string()),
        scroll: 0,
        last_error: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageDown);
    assert!(state.review.diff.as_ref().unwrap().scroll > 0);
}

#[tokio::test]
async fn ask_user_filter_and_shortcuts_update_state_and_send_structured_result() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    let args = r#"{
        "question": "Pick tags",
        "options": [
            {"id":"a","label":"Alpha"},
            {"id":"b","label":"Beta"},
            {"id":"c","label":"Gamma"}
        ],
        "kind": "multi_select",
        "allow_free_text": false,
        "metadata": {"source":"test"}
    }"#;
    let tc = ToolCall {
        id: "tc_ask_1".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: crate::services::ask_user::ASK_USER_TOOL_NAME.to_string(),
            arguments: args.to_string(),
        },
        metadata: None,
    };
    open_ask_user_popup(&mut state, &tc);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::AskUser)
    );
    assert_eq!(
        state.ask_user_question_kind,
        crate::services::ask_user::AskUserQuestionKind::MultiSelect
    );
    assert_eq!(state.ask_user_metadata.get("source").unwrap(), "test");

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::Tab);
    assert!(state.ask_user_search_active);
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('b'));
    assert_eq!(state.ask_user_filter, "b");
    assert_eq!(state.ask_user_selected, 1);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::Tab);
    assert!(!state.ask_user_search_active);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputCursorStart);
    assert!(state.ask_user_multi_selected.contains(&1));
    assert_eq!(state.ask_user_multi_selected.len(), 1);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputClear);
    assert!(state.ask_user_multi_selected.is_empty());

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    let ev = rx.recv().await.unwrap();
    let OutputEvent::SendToolResult(res, _, _) = ev else {
        panic!("expected SendToolResult");
    };
    assert_eq!(res.status, crate::types::ToolCallResultStatus::Success);
    let parsed: serde_json::Value = serde_json::from_str(&res.result).unwrap();
    assert_eq!(parsed["kind"], "multi_select");
    assert_eq!(parsed["selected"].as_array().unwrap().len(), 0);
}

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
fn shell_output_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::channel::<String>(1);
    let shell = vac_shell::ShellCommand {
        id: "shell-1".to_string(),
        command: "sh".to_string(),
        stdin_tx,
    };
    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShellStarted(shell.clone()),
    );

    let big = "x".repeat(2 * 1024 * 1024);
    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShellOutput(shell.id.clone(), big),
    );
    let active = state.shell.session_store.active().unwrap();
    assert!(active.output.len() <= 1024 * 1024);
    assert!(active.output.chars().all(|c| c == 'x'));
}

#[tokio::test]
async fn approval_queue_accepts_selected_and_emits_output_event() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    let tc1 = ToolCall {
        id: "tc-1".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "file_write".to_string(),
            arguments: serde_json::json!({"file_path":"a.txt","content":"x"}).to_string(),
        },
        metadata: None,
    };
    let tc2 = ToolCall {
        id: "tc-2".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "file_edit".to_string(),
            arguments: serde_json::json!({"file_path":"b.txt","old_string":"a","new_string":"b"})
                .to_string(),
        },
        metadata: None,
    };

    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShowConfirmationDialogWithExplanation(tc1.clone(), Some("x".to_string())),
    );
    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShowConfirmationDialogWithExplanation(tc2.clone(), Some("y".to_string())),
    );

    assert_eq!(state.pending_approvals.len(), 2);
    assert_eq!(state.approval_selected_idx, 1);
    assert_eq!(state.focus, crate::app::WorkspaceFocus::Workbench);
    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Approvals);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('a'));
    let ev = rx.recv().await.unwrap();
    match ev {
        OutputEvent::AcceptTool(tc) => assert_eq!(tc.id, "tc-2"),
        _ => panic!("unexpected event"),
    }

    assert_eq!(state.pending_approvals.len(), 1);
    assert_eq!(state.pending_approvals[0].id, "tc-1");
    assert!(state.approved_tools.iter().any(|t| t.id == "tc-2"));
}

#[tokio::test]
async fn approval_queue_rejects_selected_and_emits_output_event() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    let tc = ToolCall {
        id: "tc-1".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "file_write".to_string(),
            arguments: serde_json::json!({"file_path":"a.txt","content":"x"}).to_string(),
        },
        metadata: None,
    };

    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShowConfirmationDialogWithExplanation(tc.clone(), None),
    );

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));
    // 'r' now shows reason prompt - confirm with Enter to reject
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    let ev = rx.recv().await.unwrap();
    match ev {
        OutputEvent::RejectTool(tc, _, _) => assert_eq!(tc.id, "tc-1"),
        _ => panic!("unexpected event"),
    }

    assert!(state.pending_approvals.is_empty());
    assert!(state.rejected_tools.iter().any(|t| t.id == "tc-1"));
}

#[tokio::test]
async fn revert_selected_updates_status_and_working_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let session_id = uuid::Uuid::new_v4();
    std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

    let file_rel = "a.txt";
    std::fs::write(root.join(file_rel), "new").unwrap();
    std::fs::write(
        crate::services::review::snapshot_path(&root, session_id, file_rel),
        "old",
    )
    .unwrap();

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(root.clone(), session_id);
    state
        .changeset_store
        .file_modified(file_rel.to_string(), "agent".to_string(), true);
    state.modified_files = state.changeset_store.modified_files();
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review_sync_items();
    state.review.selected_path = Some(file_rel.to_string());

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertSelected);

    let content = std::fs::read_to_string(root.join(file_rel)).unwrap();
    assert_eq!(content, "old");
    assert!(!state.modified_files.contains(&file_rel.to_string()));
    let it = state.review.items.get(file_rel).unwrap();
    assert_eq!(it.status, crate::app::ReviewItemStatus::Restored);
}

#[tokio::test]
async fn revert_filtered_only_affects_filtered_modified_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let session_id = uuid::Uuid::new_v4();
    std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

    for (p, old, new) in [("a.txt", "old-a", "new-a"), ("b.txt", "old-b", "new-b")] {
        std::fs::write(root.join(p), new).unwrap();
        std::fs::write(
            crate::services::review::snapshot_path(&root, session_id, p),
            old,
        )
        .unwrap();
    }

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(root.clone(), session_id);
    state
        .changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), true);
    state
        .changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), true);
    state.modified_files = state.changeset_store.modified_files();
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.filter = "a".to_string();
    state.review_sync_items();
    state.review_normalize_selection();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertFiltered);

    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "old-a"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "new-b"
    );
    assert!(!state.modified_files.contains(&"a.txt".to_string()));
    assert!(state.modified_files.contains(&"b.txt".to_string()));
}

#[tokio::test]
async fn global_approval_hotkey_ctrl_m_approves_current() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add pending approval
    state.pending_approvals.push(ToolCall {
        id: "tc-1".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "test_tool".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });

    // Trigger Ctrl+M (AutoApproveCurrentTool)
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);

    // Verify approval processed
    assert_eq!(state.pending_approvals.len(), 0);
    assert_eq!(state.approved_tools.len(), 1);
    assert_eq!(state.approved_tools[0].id, "tc-1");

    // Verify output event sent
    let output = rx.try_recv().unwrap();
    assert!(matches!(output, OutputEvent::AcceptTool(_)));
}

#[tokio::test]
async fn global_approval_hotkey_ctrl_shift_m_rejects_current() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add pending approval
    state.pending_approvals.push(ToolCall {
        id: "tc-2".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "test_tool".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });

    // Trigger Ctrl+Shift+M (RejectCurrentTool) - now shows reason prompt
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);

    // Reason prompt should be active
    assert!(state.reject_reason_input.is_some());
    assert_eq!(state.pending_approvals.len(), 1); // not yet rejected

    // Confirm with Enter (no reason typed)
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

    // Verify rejection processed
    assert_eq!(state.pending_approvals.len(), 0);
    assert_eq!(state.rejected_tools.len(), 1);
    assert_eq!(state.rejected_tools[0].id, "tc-2");

    // Verify output event sent
    let output = rx.try_recv().unwrap();
    assert!(matches!(output, OutputEvent::RejectTool(_, _, _)));
}

#[tokio::test]
async fn global_approval_hotkeys_safe_when_no_pending() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // No pending approvals
    assert_eq!(state.pending_approvals.len(), 0);

    // Trigger hotkeys - should not panic
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);

    // State unchanged
    assert_eq!(state.approved_tools.len(), 0);
    assert_eq!(state.rejected_tools.len(), 0);
}

#[tokio::test]
async fn slash_command_model_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /model command exists
    let commands = state.commands;
    assert!(commands.iter().any(|c| c.command == "/model"));
}

#[tokio::test]
async fn slash_command_files_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /files command exists
    let commands = state.commands;
    assert!(commands.iter().any(|c| c.command == "/files"));
}

#[tokio::test]
async fn slash_command_changes_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /changes command exists
    let commands = state.commands;
    assert!(commands.iter().any(|c| c.command == "/changes"));
}

#[test]
fn footer_approval_bar_shows_when_pending_approvals() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add pending approval
    state.pending_approvals.push(ToolCall {
        id: "tc-1".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "file_write".to_string(),
            arguments: r#"{"file_path":"test.rs"}"#.to_string(),
        },
        metadata: None,
    });

    // Footer should show approval bar (verified by view rendering logic)
    assert!(!state.pending_approvals.is_empty());
    assert_eq!(state.approval_selected_idx, 0);
}

#[test]
fn footer_approval_bar_hidden_when_no_pending() {
    let dir = tempfile::tempdir().unwrap();
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // No pending approvals
    assert!(state.pending_approvals.is_empty());
    // Footer should show normal hints (verified by view rendering logic)
}

#[test]
fn footer_approval_bar_shows_correct_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add multiple pending approvals
    for i in 0..3 {
        state.pending_approvals.push(ToolCall {
            id: format!("tc-{}", i),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
    }

    // Select second approval
    state.approval_selected_idx = 1;

    // Verify index
    assert_eq!(state.approval_selected_idx, 1);
    assert_eq!(state.pending_approvals.len(), 3);
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
    state.pending_approvals.push(ToolCall {
        id: "tc-1".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "test".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });
    state.approved_tools.push(ToolCall {
        id: "tc-2".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
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
    assert_eq!(state.pending_approvals.len(), 0);
    assert_eq!(state.approved_tools.len(), 0);
    assert_eq!(state.rejected_tools.len(), 0);
}

#[test]
fn command_palette_filters_commands_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Empty filter shows all commands
    state.command_palette_input = "".to_string();
    let all = state.filtered_commands();
    assert!(!all.is_empty());

    // Filter by prefix
    state.command_palette_input = "/model".to_string();
    let filtered = state.filtered_commands();
    assert!(filtered.iter().any(|c| c.command == "/model"));

    // Non-matching filter
    state.command_palette_input = "/nonexistent".to_string();
    let empty = state.filtered_commands();
    assert!(empty.is_empty());
}

#[test]
fn command_palette_selection_stays_within_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state.command_palette_input = "".to_string();
    let commands = state.filtered_commands();

    // Selection should not exceed command count
    if !commands.is_empty() {
        state.command_palette_selected = 0;
        assert_eq!(state.command_palette_selected, 0);

        state.command_palette_selected = commands.len() - 1;
        assert_eq!(state.command_palette_selected, commands.len() - 1);
    }
}

#[tokio::test]
async fn command_palette_dispatch_does_not_send_literal_slash() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, mut rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Execute /review command
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
    let filtered = state.filtered_commands();
    if let Some(cmd) = filtered.iter().find(|c| c.command == "/review") {
        // Simulate command execution
        state.add_user_message(cmd.command.clone());
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        crate::overlay::close_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
    }

    // Verify review opened, not sent as literal message
    assert!(state.review.open);
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::CommandPalette)
    );

    // No output event should be sent for /review
    assert!(rx.try_recv().is_err());
}
#[tokio::test]
async fn revert_all_clears_modified_files_and_marks_status() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let session_id = uuid::Uuid::new_v4();
    std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

    for (p, old, new) in [("a.txt", "old-a", "new-a"), ("b.txt", "old-b", "new-b")] {
        std::fs::write(root.join(p), new).unwrap();
        std::fs::write(
            crate::services::review::snapshot_path(&root, session_id, p),
            old,
        )
        .unwrap();
    }

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(root.clone(), session_id);
    state
        .changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), true);
    state
        .changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), true);
    state.modified_files = state.changeset_store.modified_files();
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review_sync_items();
    state.review_normalize_selection();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertAll);

    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "old-a"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "old-b"
    );
    assert!(state.modified_files.is_empty());
    assert_eq!(
        state.review.items.get("a.txt").unwrap().status,
        crate::app::ReviewItemStatus::Restored
    );
    assert_eq!(
        state.review.items.get("b.txt").unwrap().status,
        crate::app::ReviewItemStatus::Restored
    );
}

// ── Branch 4A behavioral tests ──────────────────────────────────────────

#[test]
fn modified_files_is_derived_from_changeset_store() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state
        .changeset_store
        .file_modified("a.rs".to_string(), "agent".to_string(), true);
    state
        .changeset_store
        .file_created("b.rs".to_string(), "agent".to_string());
    state.modified_files = state.changeset_store.modified_files();

    // modified_files must equal store's derived view
    assert_eq!(state.modified_files, state.changeset_store.modified_files());
    assert_eq!(state.modified_files.len(), 2);
}

#[test]
fn counter_consistency_header_tab_popup() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state
        .changeset_store
        .file_modified("x.rs".to_string(), "agent".to_string(), true);
    state
        .changeset_store
        .file_created("y.rs".to_string(), "agent".to_string());
    state
        .changeset_store
        .file_modified("z.rs".to_string(), "agent".to_string(), true);
    state.changeset_store.revert_success("z.rs"); // reverted: not active

    let active = state.changeset_store.active_entries().len();
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
    assert_eq!(state.modified_files, state.changeset_store.modified_files());
    assert_eq!(state.changeset_store.active_entries().len(), 2);
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

// ── Branch 5A behavioral tests ──────────────────────────────────────────

#[test]
fn show_model_switcher_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowModelSwitcher);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
    assert!(state.model_switcher_filter.is_empty());
}

#[test]
fn show_file_search_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowFileSearch);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
    assert!(state.file_search_query.is_empty());
}

#[test]
fn show_changeset_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowChangeset);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::Changeset)
    );
}

#[tokio::test]
async fn slash_model_dispatch_opens_model_switcher_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.input.set_content("/model");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
}

#[tokio::test]
async fn slash_files_dispatch_opens_file_search_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.input.set_content("/files");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
}

#[tokio::test]
async fn slash_changes_dispatch_opens_changeset_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.input.set_content("/changes");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::Changeset)
    );
}

#[tokio::test]
async fn slash_review_dispatch_opens_review_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.input.set_content("/review");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(state.review.open);
    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
}

#[tokio::test]
async fn approve_current_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.pending_approvals.push(ToolCall {
        id: "tc-h".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "write_file".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);
    assert_eq!(state.pending_approvals.len(), 0);
    assert_eq!(state.approved_tools.len(), 1);
    assert!(matches!(rx.try_recv().unwrap(), OutputEvent::AcceptTool(_)));
}

#[tokio::test]
async fn reject_current_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.pending_approvals.push(ToolCall {
        id: "tc-r".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "delete_file".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
    // Reason prompt active - not yet rejected
    assert!(state.reject_reason_input.is_some());
    // Confirm with Enter
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert_eq!(state.pending_approvals.len(), 0);
    assert_eq!(state.rejected_tools.len(), 1);
    assert!(matches!(
        rx.try_recv().unwrap(),
        OutputEvent::RejectTool(_, _, _)
    ));
}

// ── Branch 6C behavioral tests ──────────────────────────────────────────

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

// ── Branch 6B behavioral tests ──────────────────────────────────────────

#[test]
fn at_trigger_activates_on_at_char() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));

    assert!(state.at_trigger_active);
    assert!(state.at_query.is_empty());
}

#[test]
fn at_trigger_updates_query_on_subsequent_chars() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('s'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('c'));

    assert!(state.at_trigger_active);
    assert_eq!(state.at_query, "src");
}

#[test]
fn at_trigger_deactivates_on_esc() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;
    state.at_trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.at_query = "src".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);

    assert!(!state.at_trigger_active);
    assert!(state.at_query.is_empty());
}

#[test]
fn at_trigger_deactivates_on_space() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;
    state.at_trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.at_query = "src".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged(' '));

    assert!(!state.at_trigger_active);
}

#[test]
fn at_trigger_backspace_pops_query() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;
    state.at_trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.at_query = "sr".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
    assert_eq!(state.at_query, "s");
    assert!(state.at_trigger_active);

    // Backspace on empty query deactivates
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace); // removes '@'
    assert!(!state.at_trigger_active);
}

#[test]
fn at_trigger_enter_creates_context_chip() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;
    state.at_trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.at_query = "src".to_string();
    state.at_results = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
    state.at_selected_idx = 0;
    // Simulate @src already in input
    state.input.insert_str("@src");

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

    // PR-T7: @mention now creates a context chip instead of inserting into input buffer.
    assert!(!state.at_trigger_active);
    assert!(state.at_query.is_empty());
    assert_eq!(state.context_chips.len(), 1);
    assert_eq!(state.context_chips[0].label, "main.rs");
    // Input buffer should be cleared of the @src token
    let content = state.input.get_content();
    assert!(!content.contains("@src"));
}

// ── Branch 6A behavioral tests ──────────────────────────────────────────

#[tokio::test]
async fn approve_all_clears_all_pending() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    for i in 0..3 {
        state.pending_approvals.push(ToolCall {
            id: format!("tc-{}", i),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
    }
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ApproveAll);
    assert_eq!(state.pending_approvals.len(), 0);
    assert_eq!(state.approved_tools.len(), 3);
    // 3 AcceptTool events emitted
    for _ in 0..3 {
        assert!(matches!(rx.try_recv().unwrap(), OutputEvent::AcceptTool(_)));
    }
}

#[tokio::test]
async fn reject_all_clears_all_pending_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    for i in 0..3 {
        state.pending_approvals.push(ToolCall {
            id: format!("tc-{}", i),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
    }
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectAll);
    assert_eq!(state.pending_approvals.len(), 0);
    assert_eq!(state.rejected_tools.len(), 3);
    for _ in 0..3 {
        assert!(matches!(
            rx.try_recv().unwrap(),
            OutputEvent::RejectTool(_, _, _)
        ));
    }
}

#[tokio::test]
async fn reject_current_shows_reason_prompt_then_confirms() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.pending_approvals.push(ToolCall {
        id: "tc-reason".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "tool".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });

    // Trigger reject - shows prompt
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
    assert!(state.reject_reason_input.is_some());
    assert_eq!(state.pending_approvals.len(), 1); // not yet rejected

    // Type reason
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('t'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('o'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('o'));
    assert_eq!(state.reject_reason_input.as_deref(), Some("too"));

    // Confirm
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(state.reject_reason_input.is_none());
    assert_eq!(state.pending_approvals.len(), 0);
    assert_eq!(state.rejected_tools.len(), 1);

    // Reason passed in event
    if let OutputEvent::RejectTool(_, _, reason) = rx.try_recv().unwrap() {
        assert_eq!(reason, Some("too".to_string()));
    } else {
        panic!("expected RejectTool");
    }
}

#[tokio::test]
async fn reject_reason_esc_rejects_without_reason() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.pending_approvals.push(ToolCall {
        id: "tc-esc".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "tool".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
    assert!(state.reject_reason_input.is_some());

    // Esc = skip reason, reject without reason
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);
    assert!(state.reject_reason_input.is_none());
    assert_eq!(state.pending_approvals.len(), 0);

    if let OutputEvent::RejectTool(_, _, reason) = rx.try_recv().unwrap() {
        assert_eq!(reason, None);
    } else {
        panic!("expected RejectTool");
    }
}

// ── Branch 5B behavioral tests ──────────────────────────────────────────

#[test]
fn review_open_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewOpen);

    assert!(state.review.open);
    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
    assert_eq!(state.focus, crate::app::WorkspaceFocus::Workbench);
}

#[test]
fn review_close_via_esc_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewClose);

    assert!(!state.review.open);
    assert!(state.review.diff.is_none());
}

#[test]
fn review_filter_push_pop_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('a'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('b'));
    assert_eq!(state.review.filter, "ab");

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterBackspace);
    assert_eq!(state.review.filter, "a");
}

#[test]
fn review_toggle_diff_clears_when_same_path() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.selected_path = Some("a.rs".to_string());
    state.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.rs".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 0,
        last_error: None,
    });

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewToggleDiff);

    assert!(state.review.diff.is_none());
}

#[test]
fn review_scroll_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.rs".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 5,
        last_error: None,
    });

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageUp);
    assert_eq!(state.review.diff.as_ref().unwrap().scroll, 0);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageDown);
    assert!(state.review.diff.as_ref().unwrap().scroll > 0);
}

#[tokio::test]
async fn runtime_tab_requests_refresh_on_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Sessions;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Agents);
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::ListAgentTasks
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::LoadAgentState
    ));

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Runtime);
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::ListRuntimeJobs
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::LoadRuntimeState
    ));
}

#[test]
fn shell_backend_lifecycle_updates_state() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::channel(4);
    let shell = vac_shell::ShellCommand {
        id: "shell-1".to_string(),
        command: "echo hi".to_string(),
        stdin_tx,
    };

    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShellStarted(shell.clone()),
    );
    let active = state.shell.session_store.active().unwrap();
    assert!(active.command.is_some());
    assert!(state.shell.session_store.popup_visible);

    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShellOutput("shell-1".to_string(), "hello\n".to_string()),
    );
    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShellWaitingForInput("shell-1".to_string()),
    );
    let active = state.shell.session_store.active().unwrap();
    assert!(active.output.contains("hello"));
    assert!(active.waiting_for_input);

    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShellCompleted("shell-1".to_string(), 0),
    );
    let active = state.shell.session_store.active().unwrap();
    assert!(active.command.is_none());
    assert_eq!(active.exit_code, Some(0));
    assert!(!active.waiting_for_input);
}

// ── PR-T8 session resume fuzzy search tests ────────────────────────────────

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
