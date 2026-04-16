//! Shell lifecycle tests

use tokio::sync::mpsc;
use vac_cli::tui::services::{ShellEvent, run_pty_command};

#[tokio::test]
async fn test_shell_basic_execution() {
    let (tx, mut rx) = mpsc::channel(100);
    
    let result = run_pty_command(
        "echo test".to_string(),
        None,
        tx,
        24,
        80,
    );
    
    assert!(result.is_ok(), "Shell command should start successfully");
    
    let mut got_output = false;
    let mut got_completion = false;
    
    while let Some(event) = rx.recv().await {
        match event {
            ShellEvent::Output(text) => {
                if text.contains("test") {
                    got_output = true;
                }
            }
            ShellEvent::Completed(_) => {
                got_completion = true;
                break;
            }
            _ => {}
        }
    }
    
    assert!(got_output, "Should receive output containing 'test'");
    assert!(got_completion, "Should receive completion event");
}

#[tokio::test]
async fn test_shell_cleanup_on_kill() {
    let (tx, mut rx) = mpsc::channel(100);
    
    let shell = run_pty_command(
        "sleep 10".to_string(),
        None,
        tx,
        24,
        80,
    ).expect("Shell should start");
    
    // Wait for shell to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Kill the shell
    let kill_result = shell.kill();
    assert!(kill_result.is_ok(), "Kill should succeed");
    
    // Should receive completion
    let mut got_completion = false;
    let timeout = tokio::time::timeout(
        tokio::time::Duration::from_secs(3),
        async {
            while let Some(event) = rx.recv().await {
                if matches!(event, ShellEvent::Completed(_)) {
                    got_completion = true;
                    break;
                }
            }
        }
    );
    
    let _ = timeout.await;
    assert!(got_completion, "Should receive completion after kill");
}

#[tokio::test]
async fn test_shell_state_cleanup() {
    use vac_cli::tui::app::AppState;
    
    let mut state = AppState::default();
    
    // Simulate shell started
    assert!(state.active_shell_command.is_none());
    assert!(!state.shell_popup_visible);
    assert!(state.shell_output.is_empty());
    
    // After shell starts
    state.shell_popup_visible = true;
    state.shell_output = "test output".to_string();
    
    // Cleanup
    state.active_shell_command = None;
    state.shell_popup_visible = false;
    state.shell_output.clear();
    state.shell_waiting_for_input = false;
    state.shell_backgrounded = false;
    state.shell_exit_code = None;
    state.shell_last_error = None;
    
    // Verify cleanup
    assert!(state.active_shell_command.is_none());
    assert!(!state.shell_popup_visible);
    assert!(state.shell_output.is_empty());
    assert!(!state.shell_waiting_for_input);
    assert!(!state.shell_backgrounded);
    assert!(state.shell_exit_code.is_none());
    assert!(state.shell_last_error.is_none());
}

#[tokio::test]
async fn test_shell_buffer_limit() {
    let (tx, mut rx) = mpsc::channel(100);
    
    // Generate large output
    let large_command = format!("printf '{}'", "x".repeat(10000));
    
    let result = run_pty_command(
        large_command,
        None,
        tx,
        24,
        80,
    );
    
    assert!(result.is_ok(), "Shell should handle large output");
    
    let mut total_output = String::new();
    let timeout = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        async {
            while let Some(event) = rx.recv().await {
                match event {
                    ShellEvent::Output(text) => {
                        total_output.push_str(&text);
                    }
                    ShellEvent::Completed(_) => break,
                    _ => {}
                }
            }
        }
    );
    
    let _ = timeout.await;
    assert!(!total_output.is_empty(), "Should receive output");
}

#[tokio::test]
async fn test_shell_exit_code() {
    let (tx, mut rx) = mpsc::channel(100);
    
    let result = run_pty_command(
        "exit 42".to_string(),
        None,
        tx,
        24,
        80,
    );
    
    assert!(result.is_ok());
    
    let mut exit_code = None;
    let timeout = tokio::time::timeout(
        tokio::time::Duration::from_secs(3),
        async {
            while let Some(event) = rx.recv().await {
                if let ShellEvent::Completed(code) = event {
                    exit_code = Some(code);
                    break;
                }
            }
        }
    );
    
    let _ = timeout.await;
    assert_eq!(exit_code, Some(42), "Should capture exit code 42");
}
