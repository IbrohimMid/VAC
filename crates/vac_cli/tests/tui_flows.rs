#![allow(clippy::unwrap_used, clippy::expect_used, clippy::field_reassign_with_default)]

//! Integration tests for TUI state flow.

use std::collections::HashSet;
use std::fs;

use tempfile::tempdir;
use tokio::sync::mpsc;
use uuid::Uuid;

use vac_tui_runtime::{
    app::ShortcutsPopupMode,
    app::{AppState, AppStateOptions, InputEvent, OutputEvent, PendingUserMessage, SessionInfo},
    handlers::{input_core, profile_switcher, rulebook_switcher, HandlerContext},
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
    state.show_ask_user_popup = true;

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::ShowProfileSwitcher);

    assert!(state.show_ask_user_popup);
    assert!(!state.show_profile_switcher);
}

#[test]
fn shortcuts_popup_swallows_input_without_touching_editor_state() {
    let mut state = AppState::default();
    state.show_shortcuts = true;
    state.input.set_content("seed");

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::InputChanged('x'));

    assert!(state.show_shortcuts);
    assert_eq!(state.input.get_content(), "seed");
}

#[tokio::test]
async fn shortcuts_popup_executes_slash_commands_directly() {
    let mut state = AppState::default();
    state.show_shortcuts = true;
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

    assert!(state.show_model_switcher);
    assert!(!state.show_shortcuts);
    assert_eq!(state.input.get_content(), "seed");
}

#[tokio::test]
async fn shortcuts_popup_can_switch_tabs_without_closing() {
    let mut state = AppState::default();
    state.show_shortcuts = true;
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

    assert!(state.show_shortcuts);
    assert_eq!(state.shortcuts_mode, ShortcutsPopupMode::Sessions);
    assert_eq!(state.shortcuts_scroll, 0);
    assert!(state.command_palette_input.is_empty());
}

#[tokio::test]
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

#[tokio::test]
async fn profile_switcher_request_on_open_and_submit_is_deterministic() {
    let mut state = AppState::default();
    state.active_profile = "migration".to_string();

    let (output_tx, mut output_rx) = mpsc::channel(8);

    input_core::handle_input_event(&mut state, &output_tx, InputEvent::ShowProfileSwitcher);
    {
        let mut ctx = HandlerContext::new(&mut state, &output_tx);
        assert!(ctx.state.show_profile_switcher);
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
    assert!(!state.show_profile_switcher);
}

#[tokio::test]
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
        assert!(ctx.state.show_rulebook_switcher);
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
    assert!(!state.show_rulebook_switcher);
}

#[tokio::test]
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

    assert!(!root
        .join(".vac/sessions")
        .join(format!("{session_id_str}.snapshot.json"))
        .exists());
    assert!(!root
        .join(".vac/checkpoints")
        .join(format!("{session_id_str}_state.json"))
        .exists());
    assert!(!root.join(".vac/approvals").join("approval.json").exists());

    match rx.try_recv().unwrap() {
        OutputEvent::ListSessions => {}
        other => panic!("unexpected output event: {other:?}"),
    }
    assert!(state
        .toasts
        .iter()
        .any(|toast| toast.message.contains("Cleaned session artifacts")));
}
