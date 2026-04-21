#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]

//! Integration tests for TUI state flow.

use std::collections::HashSet;
use std::fs;

use tempfile::tempdir;
use tokio::sync::mpsc;
use uuid::Uuid;

use vac_tui_runtime::{
    app::ShortcutsPopupMode,
    app::{AppState, AppStateOptions, InputEvent, OutputEvent, PendingUserMessage, SessionInfo},
    handlers::{HandlerContext, input_core, profile_switcher, rulebook_switcher},
    services::{
        commands::{CommandAction, CommandSurface},
        helper_block, shortcuts_popup,
    },
    update::flush_pending_user_messages_if_idle,
};

fn slash_commands_from_palette(state: &AppState) -> HashSet<String> {
    state
        .filtered_commands()
        .into_iter()
        .map(|cmd| cmd.command)
        .collect()
}

fn slash_commands_from_shortcuts(state: &AppState) -> HashSet<String> {
    shortcuts_popup::filter_commands("", state)
        .into_iter()
        .filter_map(|cmd| match cmd.action {
            CommandAction::InsertSlashCommand(s) => Some(s),
            _ => None,
        })
        .collect()
}

#[test]
fn welcome_copy_is_truthful_on_first_frame() {
    let state = AppState::default();
    let messages = helper_block::welcome_messages(None, &state);

    assert_eq!(messages.len(), 1);
    let content = &messages[0].content;
    assert!(content.contains("Vastar Agentic CLI"));
    assert!(content.contains("No active model configured"));
    assert!(!content.contains("unknown"));
    assert!(!content.contains("Model: none"));
}

#[test]
fn operator_surfaces_hide_passthrough_commands_and_match_parity() {
    let state = AppState::default();
    let palette = slash_commands_from_palette(&state);
    let shortcuts = slash_commands_from_shortcuts(&state);

    assert_eq!(palette, shortcuts);

    let registry = helper_block::vac_commands();
    for hidden in ["/vil", "/swarm", "/rulebook", "/resume", "/help"] {
        let spec = registry
            .iter()
            .find(|cmd| cmd.command == hidden)
            .unwrap_or_else(|| panic!("missing registry entry for {hidden}"));
        assert_eq!(
            spec.surface,
            CommandSurface::Hidden,
            "{hidden} should remain hidden from operator surfaces"
        );
        assert!(
            !palette.contains(hidden),
            "{hidden} leaked into command palette"
        );
        assert!(
            !shortcuts.contains(hidden),
            "{hidden} leaked into shortcuts popup"
        );
    }
}

#[test]
fn popup_precedence_blocks_lower_priority_open_requests() {
    let mut state = AppState::default();
    state.overlay_manager.push(vac_tui_runtime::overlay::OverlayId::AskUser, state.focus);

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::ShowProfileSwitcher);

    assert!(state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::AskUser));
    assert!(!state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::ProfileSwitcher));
}

#[test]
fn shortcuts_popup_swallows_input_without_touching_editor_state() {
    let mut state = AppState::default();
    state.overlay_manager.push(vac_tui_runtime::overlay::OverlayId::Shortcuts, state.focus);
    state.input.set_content("seed");

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::InputChanged('x'));

    assert!(state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::Shortcuts));
    assert_eq!(state.input.get_content(), "seed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shortcuts_popup_executes_slash_commands_directly() {
    let mut state = AppState::default();
    state.overlay_manager.push(vac_tui_runtime::overlay::OverlayId::Shortcuts, state.focus);
    state.shortcuts_mode = ShortcutsPopupMode::Commands;
    state.input.set_content("seed");

    let commands = shortcuts_popup::filter_commands("", &state);
    let model_idx = commands
        .iter()
        .position(|cmd| {
            matches!(
                &cmd.action,
                CommandAction::InsertSlashCommand(s) if s == "/model"
            )
        })
        .expect("/model should be present in shortcuts popup");
    state.shortcuts_scroll = model_idx;

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::InputSubmitted);

    assert!(state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::ModelSwitcher));
    assert!(!state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::Shortcuts));
    assert_eq!(state.input.get_content(), "seed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shortcuts_popup_can_switch_tabs_without_closing() {
    let mut state = AppState::default();
    state.overlay_manager.push(vac_tui_runtime::overlay::OverlayId::Shortcuts, state.focus);
    state.shortcuts_mode = ShortcutsPopupMode::Commands;
    state.command_palette_input = "stale filter".to_string();

    let commands = shortcuts_popup::filter_commands("", &state);
    let sessions_idx = commands
        .iter()
        .position(|cmd| matches!(&cmd.action, CommandAction::OpenSessions))
        .expect("Resume Session command should be present");
    state.shortcuts_scroll = sessions_idx;

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::InputSubmitted);

    assert!(state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::Shortcuts));
    assert_eq!(state.shortcuts_mode, ShortcutsPopupMode::Sessions);
    assert_eq!(state.shortcuts_scroll, 0);
    assert!(state.command_palette_input.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn buffered_messages_flush_as_a_single_user_message() {
    let mut state = AppState::default();
    state
        .pending_user_messages
        .push_back(PendingUserMessage::new(
            "first".to_string(),
            None,
            vec![],
            "first".to_string(),
        ));
    state
        .pending_user_messages
        .push_back(PendingUserMessage::new(
            "second".to_string(),
            None,
            vec![],
            "second".to_string(),
        ));

    let (input_tx, mut input_rx) = mpsc::channel(8);
    let (output_tx, mut output_rx) = mpsc::channel(8);

    flush_pending_user_messages_if_idle(&mut state, &input_tx, &output_tx);

    match output_rx
        .recv()
        .await
        .expect("output event should be emitted")
    {
        OutputEvent::UserMessage(final_input, shell_calls, image_parts, revert_idx) => {
            assert_eq!(final_input, "first\n\nsecond");
            assert!(shell_calls.is_none());
            assert!(image_parts.is_empty());
            assert!(revert_idx.is_none());
        }
        other => panic!("unexpected output event: {other:?}"),
    }

    match input_rx
        .recv()
        .await
        .expect("input event should be emitted")
    {
        InputEvent::AddUserMessage(text) => assert_eq!(text, "first\n\nsecond"),
        other => panic!("unexpected input event: {other:?}"),
    }

    assert_eq!(state.queue_metrics.total_queued, 1);
    assert_eq!(state.queue_metrics.total_merged, 1);
    assert!(state.pending_user_messages.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_switcher_request_on_open_and_submit_is_deterministic() {
    let mut state = AppState::default();
    state.active_profile = "migration".to_string();

    let (output_tx, mut output_rx) = mpsc::channel(8);

    input_core::handle_input_event(&mut state, &output_tx, InputEvent::ShowProfileSwitcher);
    {
        let mut ctx = HandlerContext::new(&mut state, &output_tx);
        assert!(ctx.state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::ProfileSwitcher));
        let filtered = ctx.state.profile_switcher_filtered();
        assert!(
            filtered.iter().any(|p| p == "migration"),
            "profile switcher should preselect from the active profile set"
        );
        assert_eq!(filtered[ctx.state.profile_switcher_selected], "migration");
        profile_switcher::submit_selected(&mut ctx).unwrap();
    }

    match output_rx.recv().await.unwrap() {
        OutputEvent::SwitchProfile(profile) => assert_eq!(profile, "migration"),
        other => panic!("unexpected output event: {other:?}"),
    }
    assert!(!state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::ProfileSwitcher));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rulebook_switcher_loads_disk_data_and_submits_active_rulebook() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join(".vac")).unwrap();
    fs::write(
        root.join(".vac/rules.toml"),
        r#"
id = "workspace"
name = "Workspace Rules"

[[constraints]]
id = "no-unwrap"
description = "Do not use unwrap in production code"
severity = "warn"
"#,
    )
    .unwrap();

    let mut state = AppState::new(AppStateOptions {
        model: None,
        session_id: Some(Uuid::new_v4().to_string()),
        checkpoint_path: None,
        project_root: root.to_path_buf(),
    });
    state.selected_rulebooks.insert("workspace".to_string());

    let (output_tx, mut output_rx) = mpsc::channel(8);

    input_core::handle_input_event(&mut state, &output_tx, InputEvent::ShowRulebookSwitcher);
    {
        let mut ctx = HandlerContext::new(&mut state, &output_tx);
        assert!(ctx.state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::RulebookSwitcher));
        assert_eq!(ctx.state.available_rulebooks.len(), 1);
        assert_eq!(ctx.state.available_rulebooks[0].id, "workspace");
        assert_eq!(ctx.state.rulebook_switcher_selected, 0);
        rulebook_switcher::submit_selected(&mut ctx).unwrap();
    }

    match output_rx.recv().await.unwrap() {
        OutputEvent::ApplyRulebooks(selected) => {
            assert_eq!(selected, vec!["workspace".to_string()]);
        }
        other => panic!("unexpected output event: {other:?}"),
    }
    assert!(!state.overlay_manager.is_active(vac_tui_runtime::overlay::OverlayId::RulebookSwitcher));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sessions_tab_cleans_selected_session_artifacts_and_refreshes() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let session_id = Uuid::new_v4();
    let session_id_str = session_id.to_string();
    fs::create_dir_all(root.join(".vac/sessions")).unwrap();
    fs::create_dir_all(root.join(".vac/checkpoints")).unwrap();
    fs::create_dir_all(root.join(".vac/approvals")).unwrap();

    fs::write(
        root.join(".vac/sessions")
            .join(format!("{session_id_str}.snapshot.json")),
        "{\"schema_version\":1}",
    )
    .unwrap();
    fs::write(
        root.join(".vac/checkpoints")
            .join(format!("{session_id_str}_state.json")),
        "{}",
    )
    .unwrap();
    fs::write(
        root.join(".vac/approvals").join("approval.json"),
        format!("{{\"session_id\":\"{session_id_str}\"}}"),
    )
    .unwrap();

    let mut state = AppState::new(AppStateOptions {
        model: None,
        session_id: Some(Uuid::new_v4().to_string()),
        checkpoint_path: Some(root.join(".vac/checkpoints")),
        project_root: root.to_path_buf(),
    });
    state.focus = vac_tui_runtime::app::WorkspaceFocus::Workbench;
    state.workbench_tab = vac_tui_runtime::app::WorkbenchTab::Sessions;
    state.sessions = vec![SessionInfo {
        title: "Session cleanup".to_string(),
        id: session_id_str.clone(),
        updated_at: "2026-04-18T00:00:00Z".to_string(),
        checkpoints: vec!["checkpoint.json".to_string()],
        task_count: 1,
        last_activity: "2026-04-18 00:00".to_string(),
        has_checkpoint: true,
        snapshot_present: true,
        snapshot_stale: false,
    }];
    state.sessions_selected_idx = 0;

    let (tx, mut rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &tx, InputEvent::InputChanged('d'));

    // Since PR-W25-9 the 'd' handler only dispatches `OutputEvent::CleanupSession`
    // (the actual filesystem work runs asynchronously in the runner task). The
    // test simulates that downstream step by invoking cleanup_session_async
    // directly, matching what runner::spawn_session_event_loop does.
    let cleanup_id = match rx.try_recv().unwrap() {
        OutputEvent::CleanupSession(id) => id,
        other => panic!("unexpected output event: {other:?}"),
    };
    assert_eq!(cleanup_id, session_id_str);

    let report = vac_session_control::cleanup_session_async(root.to_path_buf(), session_id)
        .await
        .expect("cleanup_session_async should succeed");
    assert!(report.snapshot_removed, "snapshot should be removed");
    assert!(report.checkpoint_removed, "checkpoint should be removed");
    assert_eq!(report.approvals_removed, 1, "one approval file should be removed");
    assert!(report.errors.is_empty(), "cleanup should not report errors: {:?}", report.errors);

    assert!(
        !root
            .join(".vac/sessions")
            .join(format!("{session_id_str}.snapshot.json"))
            .exists()
    );
    assert!(
        !root
            .join(".vac/checkpoints")
            .join(format!("{session_id_str}_state.json"))
            .exists()
    );
    assert!(!root.join(".vac/approvals").join("approval.json").exists());
}

// =======================================================================
// PR-T16 P2 — E2E mouse-dispatch coverage per surface
//
// Each test drives `handlers::mouse::dispatch_click` from the vac_cli
// integration-test boundary (= public crate surface), seeding the region
// fields exactly as a view render pass would, and asserting that the
// observable AppState mutation matches the documented behaviour in
// docs/tui/action_matrix.md.
// =======================================================================

mod pr_t16_mouse_dispatch_e2e {
    use ratatui::layout::Rect;
    use tokio::sync::mpsc;
    use vac_tui_runtime::app::{
        AppState, OutputEvent, WorkbenchTab, WorkspaceFocus,
    };
    use vac_tui_runtime::handlers::mouse::dispatch_click;

    fn make_state() -> (
        AppState,
        mpsc::Sender<OutputEvent>,
        mpsc::Receiver<OutputEvent>,
    ) {
        let state = AppState::default();
        let (tx, rx) = mpsc::channel::<OutputEvent>(64);
        (state, tx, rx)
    }

    #[test]
    fn review_row_click_selects_path_and_switches_tab() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Sessions;
        state
            .review_file_row_regions
            .push(("src/lib.rs".to_string(), Rect::new(2, 5, 40, 1)));
        state
            .review_file_row_regions
            .push(("src/main.rs".to_string(), Rect::new(2, 6, 40, 1)));

        let handled = dispatch_click(&mut state, &tx, 10, 6);
        assert!(handled, "click inside review row region must be consumed");
        assert_eq!(
            state.review.selected_path.as_deref(),
            Some("src/main.rs"),
            "clicked file should become the selected path"
        );
        assert_eq!(state.workbench_tab, WorkbenchTab::Review);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn approvals_row_click_selects_idx_and_switches_tab() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state
            .approvals_row_regions
            .push((0, Rect::new(4, 8, 30, 1)));
        state
            .approvals_row_regions
            .push((2, Rect::new(4, 10, 30, 1)));

        let handled = dispatch_click(&mut state, &tx, 5, 10);
        assert!(handled);
        assert_eq!(state.approval_selected_idx, 2);
        assert_eq!(state.workbench_tab, WorkbenchTab::Approvals);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn vil_issue_row_click_selects_and_switches_tab() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state
            .vil_issue_row_regions
            .push((3, Rect::new(2, 12, 60, 1)));

        let handled = dispatch_click(&mut state, &tx, 5, 12);
        assert!(handled);
        assert_eq!(state.vil.workbench_selected, 3);
        assert_eq!(state.workbench_tab, WorkbenchTab::Vil);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn workbench_body_click_grabs_focus_without_tab_switch() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Plan;
        state.workbench_body_region = Some(Rect::new(0, 5, 80, 20));

        let handled = dispatch_click(&mut state, &tx, 40, 15);
        assert!(handled);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
        // Body fallback must NOT silently change the active tab.
        assert_eq!(state.workbench_tab, WorkbenchTab::Plan);
    }

    #[test]
    fn click_outside_all_regions_is_ignored() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state
            .review_file_row_regions
            .push(("a.rs".to_string(), Rect::new(0, 0, 10, 1)));
        state.workbench_body_region = Some(Rect::new(0, 5, 20, 5));

        let handled = dispatch_click(&mut state, &tx, 80, 40);
        assert!(!handled, "click outside every region must be a no-op");
        assert_eq!(state.focus, WorkspaceFocus::Input);
    }
}
