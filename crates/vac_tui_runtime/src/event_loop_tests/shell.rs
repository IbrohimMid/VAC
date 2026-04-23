#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::make_state;
use crate::app::InputEvent;

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
    let active = state.execution.shell.session_store.active().unwrap();
    assert!(active.output.len() <= 1024 * 1024);
    assert!(active.output.chars().all(|c| c == 'x'));
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
    let active = state.execution.shell.session_store.active().unwrap();
    assert!(active.command.is_some());
    assert!(state.execution.shell.session_store.popup_visible);

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
    let active = state.execution.shell.session_store.active().unwrap();
    assert!(active.output.contains("hello"));
    assert!(active.waiting_for_input);

    crate::controller::handle_backend_event(
        &mut state,
        &tx,
        InputEvent::ShellCompleted("shell-1".to_string(), 0),
    );
    let active = state.execution.shell.session_store.active().unwrap();
    assert!(active.command.is_none());
    assert_eq!(active.exit_code, Some(0));
    assert!(!active.waiting_for_input);
}
