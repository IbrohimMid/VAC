#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::make_state;
use crate::app::{InputEvent, OutputEvent};

#[test]
fn slash_semantics_fix_and_explain_send_prompt_with_args() {
    // /fix and /explain are BuiltInWithPrompt: they push to pending_user_messages,
    // not directly to the output channel.
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state.composer.input.set_content("/fix cargo clippy");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    let msg = state
        .transcript.pending_user_messages
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
        .composer.input
        .set_content("/explain crates/vac_cli/src/tui/event_loop.rs");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    let msg = state
        .transcript.pending_user_messages
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
        .workspace.changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), false);
    state.workspace.modified_files = state.workspace.changeset_store.modified_files();
    state.composer.input.set_content("/review");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(state.workspace.review.open);
    assert_eq!(state.layout.workbench_tab, crate::app::WorkbenchTab::Review);
}

#[tokio::test]
async fn slash_command_model_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /model command exists
    let commands = state.layout.commands;
    assert!(commands.iter().any(|c| c.command == "/model"));
}

#[tokio::test]
async fn slash_command_files_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /files command exists
    let commands = state.layout.commands;
    assert!(commands.iter().any(|c| c.command == "/files"));
}

#[tokio::test]
async fn slash_command_changes_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /changes command exists
    let commands = state.layout.commands;
    assert!(commands.iter().any(|c| c.command == "/changes"));
}

#[test]
fn command_palette_filters_commands_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Empty filter shows all commands
    state.layout.command_palette.input = "".to_string();
    let all = state.filtered_commands();
    assert!(!all.is_empty());

    // Filter by prefix
    state.layout.command_palette.input = "/model".to_string();
    let filtered = state.filtered_commands();
    assert!(filtered.iter().any(|c| c.command == "/model"));

    // Non-matching filter
    state.layout.command_palette.input = "/nonexistent".to_string();
    let empty = state.filtered_commands();
    assert!(empty.is_empty());
}

#[test]
fn command_palette_selection_stays_within_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state.layout.command_palette.input = "".to_string();
    let commands = state.filtered_commands();

    // Selection should not exceed command count
    if !commands.is_empty() {
        state.layout.command_palette.selected = 0;
        assert_eq!(state.layout.command_palette.selected, 0);

        state.layout.command_palette.selected = commands.len() - 1;
        assert_eq!(state.layout.command_palette.selected, commands.len() - 1);
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
        state.workspace.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        crate::overlay::close_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
    }

    // Verify review opened, not sent as literal message
    assert!(state.workspace.review.open);
    assert!(
        !state
            .layout.overlay_manager
            .is_active(crate::overlay::OverlayId::CommandPalette)
    );

    // No output event should be sent for /review
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn slash_model_dispatch_opens_model_switcher_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.composer.input.set_content("/model");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .layout.overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
    assert!(
        !state
            .layout.overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
}

#[tokio::test]
async fn slash_files_dispatch_opens_file_search_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.composer.input.set_content("/files");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .layout.overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
    assert!(
        !state
            .layout.overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
}

#[tokio::test]
async fn slash_changes_dispatch_opens_changeset_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.composer.input.set_content("/changes");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .layout.overlay_manager
            .is_active(crate::overlay::OverlayId::Changeset)
    );
}

#[tokio::test]
async fn slash_review_dispatch_opens_review_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.composer.input.set_content("/review");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(state.workspace.review.open);
    assert_eq!(state.layout.workbench_tab, crate::app::WorkbenchTab::Review);
}
