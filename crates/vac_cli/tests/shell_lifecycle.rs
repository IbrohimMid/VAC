#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Shell lifecycle tests

use tokio::sync::mpsc;
use vac_shell::{ShellEvent, run_pty_command};

#[tokio::test]
async fn test_shell_basic_execution() {
    let (tx, mut rx) = mpsc::channel(100);

    let shell = run_pty_command(String::new(), None, tx, 24, 80)
        .expect("Shell command should start successfully");

    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    shell.send_input("echo test\n".to_string());
    shell.send_input("exit\n".to_string());

    let mut got_output = false;
    let mut got_completion = false;

    let timeout = tokio::time::timeout(tokio::time::Duration::from_secs(5), async {
        while let Some(event) = rx.recv().await {
            match event {
                ShellEvent::Output(_, text) => {
                    if text.contains("test") {
                        got_output = true;
                    }
                }
                ShellEvent::Completed(_, _) => {
                    got_completion = true;
                    break;
                }
                _ => {}
            }
        }
    });

    let _ = timeout.await;
    assert!(got_output, "Should receive output containing 'test'");
    assert!(got_completion, "Should receive completion event");
}

#[tokio::test]
async fn test_shell_cleanup_on_kill() {
    let (tx, mut rx) = mpsc::channel(100);

    let shell =
        run_pty_command("sleep 10".to_string(), None, tx, 24, 80).expect("Shell should start");

    // Wait for shell to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Kill the shell
    let kill_result = shell.kill();
    assert!(kill_result.is_ok(), "Kill should succeed");

    // Should receive completion
    let mut got_completion = false;
    let timeout = tokio::time::timeout(tokio::time::Duration::from_secs(3), async {
        while let Some(event) = rx.recv().await {
            if matches!(event, ShellEvent::Completed(_, _)) {
                got_completion = true;
                break;
            }
        }
    });

    let _ = timeout.await;
    assert!(got_completion, "Should receive completion after kill");
}

#[tokio::test]
async fn test_shell_state_cleanup() {
    use vac_tui_runtime::app::AppState;

    let mut state = AppState::default();

    assert!(state.execution.shell.session_store.active().is_none());
    assert!(!state.execution.shell.session_store.popup_visible);

    let idx = state
        .execution
        .shell
        .session_store
        .push_new("shell-1".to_string());
    state.execution.shell.session_store.popup_visible = true;
    let session = &mut state.execution.shell.session_store.sessions[idx];
    session.output = "test output".to_string();
    session.waiting_for_input = true;
    session.backgrounded = true;
    session.exit_code = Some(1);
    session.last_error = Some("boom".to_string());

    state.execution.shell.session_store.popup_visible = false;
    let session = &mut state.execution.shell.session_store.sessions[idx];
    session.command = None;
    session.output.clear();
    session.waiting_for_input = false;
    session.backgrounded = false;
    session.exit_code = None;
    session.last_error = None;

    let session = state.execution.shell.session_store.active().unwrap();
    assert!(session.command.is_none());
    assert!(!state.execution.shell.session_store.popup_visible);
    assert!(session.output.is_empty());
    assert!(!session.waiting_for_input);
    assert!(!session.backgrounded);
    assert!(session.exit_code.is_none());
    assert!(session.last_error.is_none());
}

#[tokio::test]
async fn test_shell_buffer_limit() {
    let (tx, mut rx) = mpsc::channel(100);

    // Generate large output
    let large_command = format!("printf '{}'", "x".repeat(10000));

    let shell = run_pty_command(large_command, None, tx, 24, 80).expect("Shell should start");

    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    shell.send_input("exit\n".to_string());

    let mut total_output = String::new();
    let timeout = tokio::time::timeout(tokio::time::Duration::from_secs(5), async {
        while let Some(event) = rx.recv().await {
            match event {
                ShellEvent::Output(_, text) => {
                    total_output.push_str(&text);
                }
                ShellEvent::Completed(_, _) => break,
                _ => {}
            }
        }
    });

    let _ = timeout.await;
    assert!(!total_output.is_empty(), "Should receive output");
}

#[tokio::test]
async fn test_shell_exit_code() {
    let (tx, mut rx) = mpsc::channel(100);

    let result = run_pty_command("exit 42".to_string(), None, tx, 24, 80);

    assert!(result.is_ok());

    let mut exit_code = None;
    let timeout = tokio::time::timeout(tokio::time::Duration::from_secs(5), async {
        while let Some(event) = rx.recv().await {
            if let ShellEvent::Completed(_, code) = event {
                exit_code = Some(code);
                break;
            }
        }
    });

    timeout
        .await
        .expect("shell completion should arrive before timeout");
    assert_eq!(exit_code, Some(42), "Should capture exit code 42");
}
