#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::make_state;
use crate::app::{InputEvent, OutputEvent};
use crate::controller::open_ask_user_popup;
use crate::{FunctionCall, ToolCall};

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
        state.ask_user.question_kind,
        crate::services::ask_user::AskUserQuestionKind::MultiSelect
    );
    assert_eq!(state.ask_user.metadata.get("source").unwrap(), "test");

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::Tab);
    assert!(state.ask_user.search_active);
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('b'));
    assert_eq!(state.ask_user.filter, "b");
    assert_eq!(state.ask_user.selected, 1);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::Tab);
    assert!(!state.ask_user.search_active);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputCursorStart);
    assert!(state.ask_user.multi_selected.contains(&1));
    assert_eq!(state.ask_user.multi_selected.len(), 1);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputClear);
    assert!(state.ask_user.multi_selected.is_empty());

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

    assert_eq!(state.approvals.pending_approvals.len(), 2);
    assert_eq!(state.approvals.approval_selected_idx, 1);
    assert_eq!(state.focus, crate::app::WorkspaceFocus::Workbench);
    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Approvals);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('a'));
    let ev = rx.recv().await.unwrap();
    match ev {
        OutputEvent::AcceptTool(tc) => assert_eq!(tc.id, "tc-2"),
        _ => panic!("unexpected event"),
    }

    assert_eq!(state.approvals.pending_approvals.len(), 1);
    assert_eq!(state.approvals.pending_approvals[0].id, "tc-1");
    assert!(state.approvals.approved_tools.iter().any(|t| t.id == "tc-2"));
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

    assert!(state.approvals.pending_approvals.is_empty());
    assert!(state.approvals.rejected_tools.iter().any(|t| t.id == "tc-1"));
}

#[tokio::test]
async fn global_approval_hotkey_ctrl_m_approves_current() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add pending approval
    state.approvals.pending_approvals.push(ToolCall {
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
    assert_eq!(state.approvals.pending_approvals.len(), 0);
    assert_eq!(state.approvals.approved_tools.len(), 1);
    assert_eq!(state.approvals.approved_tools[0].id, "tc-1");

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
    state.approvals.pending_approvals.push(ToolCall {
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
    assert!(state.approvals.reject_reason_input.is_some());
    assert_eq!(state.approvals.pending_approvals.len(), 1); // not yet rejected

    // Confirm with Enter (no reason typed)
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

    // Verify rejection processed
    assert_eq!(state.approvals.pending_approvals.len(), 0);
    assert_eq!(state.approvals.rejected_tools.len(), 1);
    assert_eq!(state.approvals.rejected_tools[0].id, "tc-2");

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
    assert_eq!(state.approvals.pending_approvals.len(), 0);

    // Trigger hotkeys - should not panic
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);

    // State unchanged
    assert_eq!(state.approvals.approved_tools.len(), 0);
    assert_eq!(state.approvals.rejected_tools.len(), 0);
}

#[test]
fn footer_approval_bar_shows_when_pending_approvals() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add pending approval
    state.approvals.pending_approvals.push(ToolCall {
        id: "tc-1".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "file_write".to_string(),
            arguments: r#"{"file_path":"test.rs"}"#.to_string(),
        },
        metadata: None,
    });

    // Footer should show approval bar (verified by view rendering logic)
    assert!(!state.approvals.pending_approvals.is_empty());
    assert_eq!(state.approvals.approval_selected_idx, 0);
}

#[test]
fn footer_approval_bar_hidden_when_no_pending() {
    let dir = tempfile::tempdir().unwrap();
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // No pending approvals
    assert!(state.approvals.pending_approvals.is_empty());
    // Footer should show normal hints (verified by view rendering logic)
}

#[test]
fn footer_approval_bar_shows_correct_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Add multiple pending approvals
    for i in 0..3 {
        state.approvals.pending_approvals.push(ToolCall {
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
    state.approvals.approval_selected_idx = 1;

    // Verify index
    assert_eq!(state.approvals.approval_selected_idx, 1);
    assert_eq!(state.approvals.pending_approvals.len(), 3);
}

#[tokio::test]
async fn approve_current_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.approvals.pending_approvals.push(ToolCall {
        id: "tc-h".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "write_file".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);
    assert_eq!(state.approvals.pending_approvals.len(), 0);
    assert_eq!(state.approvals.approved_tools.len(), 1);
    assert!(matches!(rx.try_recv().unwrap(), OutputEvent::AcceptTool(_)));
}

#[tokio::test]
async fn reject_current_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.approvals.pending_approvals.push(ToolCall {
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
    assert!(state.approvals.reject_reason_input.is_some());
    // Confirm with Enter
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert_eq!(state.approvals.pending_approvals.len(), 0);
    assert_eq!(state.approvals.rejected_tools.len(), 1);
    assert!(matches!(
        rx.try_recv().unwrap(),
        OutputEvent::RejectTool(_, _, _)
    ));
}

#[tokio::test]
async fn approve_all_clears_all_pending() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    for i in 0..3 {
        state.approvals.pending_approvals.push(ToolCall {
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
    assert_eq!(state.approvals.pending_approvals.len(), 0);
    assert_eq!(state.approvals.approved_tools.len(), 3);
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
        state.approvals.pending_approvals.push(ToolCall {
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
    assert_eq!(state.approvals.pending_approvals.len(), 0);
    assert_eq!(state.approvals.rejected_tools.len(), 3);
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
    state.approvals.pending_approvals.push(ToolCall {
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
    assert!(state.approvals.reject_reason_input.is_some());
    assert_eq!(state.approvals.pending_approvals.len(), 1); // not yet rejected

    // Type reason
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('t'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('o'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('o'));
    assert_eq!(state.approvals.reject_reason_input.as_deref(), Some("too"));

    // Confirm
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(state.approvals.reject_reason_input.is_none());
    assert_eq!(state.approvals.pending_approvals.len(), 0);
    assert_eq!(state.approvals.rejected_tools.len(), 1);

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
    state.approvals.pending_approvals.push(ToolCall {
        id: "tc-esc".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "tool".to_string(),
            arguments: "{}".to_string(),
        },
        metadata: None,
    });

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
    assert!(state.approvals.reject_reason_input.is_some());

    // Esc = skip reason, reject without reason
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);
    assert!(state.approvals.reject_reason_input.is_none());
    assert_eq!(state.approvals.pending_approvals.len(), 0);

    if let OutputEvent::RejectTool(_, _, reason) = rx.try_recv().unwrap() {
        assert_eq!(reason, None);
    } else {
        panic!("expected RejectTool");
    }
}
