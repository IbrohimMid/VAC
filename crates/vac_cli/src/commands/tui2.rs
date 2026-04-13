//! TUI2 Command

use anyhow::Result;
use tokio::sync::mpsc;
use vac_cli::tui2::{run_tui, InputEvent, Model, OutputEvent, RulebookConfig};

pub async fn execute() -> Result<()> {
    let (input_tx, input_rx) = mpsc::channel::<InputEvent>(100);
    let (output_tx, mut output_rx) = mpsc::channel::<OutputEvent>(100);
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Backend handler (adapter to VacEngine)
    tokio::spawn(async move {
        while let Some(event) = output_rx.recv().await {
            match event {
                OutputEvent::UserMessage(msg, _, _, _) => {
                    let _ = input_tx.send(InputEvent::StartLoadingOperation(
                        vac_cli::tui2::LoadingOperation::LlmRequest,
                    )).await;
                    
                    // Simulate streaming response
                    let response = format!("You said: {}\n\nThis is a stub response. Connect to VacEngine for real responses.", msg);
                    let msg_id = uuid::Uuid::new_v4();
                    
                    for chunk in response.split_inclusive(|c| c == ' ' || c == '\n') {
                        let _ = input_tx.send(InputEvent::StreamAssistantMessage(msg_id, chunk.to_string())).await;
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                    
                    let _ = input_tx.send(InputEvent::EndLoadingOperation(
                        vac_cli::tui2::LoadingOperation::LlmRequest,
                    )).await;
                }
                OutputEvent::CancelStream => {
                    let _ = input_tx.send(InputEvent::EndLoadingOperation(
                        vac_cli::tui2::LoadingOperation::LlmRequest,
                    )).await;
                }
                OutputEvent::ExecuteCommand(cmd) => {
                    match cmd.as_str() {
                        // VAC-specific commands
                        "/vil" => {
                            let _ = input_tx.send(InputEvent::AddUserMessage(
                                "VIL Engine Status:\n\
                                • Version: 1.0.0\n\
                                • Mode: VIL-native\n\
                                • Restore: restore-first\n\
                                • Checkpoint: enabled".to_string()
                            )).await;
                        }
                        "/swarm" => {
                            let _ = input_tx.send(InputEvent::AddUserMessage(
                                "Swarm Status:\n\
                                • Agents: 0 active\n\
                                • Tasks: 0 pending\n\
                                • Mode: idle".to_string()
                            )).await;
                        }
                        "/rulebook" => {
                            let _ = input_tx.send(InputEvent::AddUserMessage(
                                "Rulebook Status:\n\
                                • Loaded: 0 rulebooks\n\
                                • Use 'vac rulebook list' to see available rulebooks".to_string()
                            )).await;
                        }
                        "/context" => {
                            let _ = input_tx.send(InputEvent::AddUserMessage(
                                "Context Budget:\n\
                                • Used: 0 tokens\n\
                                • Remaining: unlimited\n\
                                • Trim boundary: 0".to_string()
                            )).await;
                        }
                        "/runtime" => {
                            let _ = input_tx.send(InputEvent::AddUserMessage(
                                "Runtime Status:\n\
                                • Jobs: 0 queued\n\
                                • Mode: interactive\n\
                                • Scheduler: inactive".to_string()
                            )).await;
                        }
                        // Session commands
                        "/clear" => {
                            let _ = input_tx.send(InputEvent::AddUserMessage("Conversation cleared.".to_string())).await;
                        }
                        "/new" => {
                            let _ = input_tx.send(InputEvent::AddUserMessage("New session started.".to_string())).await;
                        }
                        "/sessions" => {
                            let _ = input_tx.send(InputEvent::SetSessions(vec![])).await;
                            let _ = input_tx.send(InputEvent::AddUserMessage("No saved sessions.".to_string())).await;
                        }
                        "/resume" => {
                            let _ = input_tx.send(InputEvent::AddUserMessage(
                                "Resume: No checkpoint specified.\n\
                                Use: vac tui2 --resume <checkpoint_path>".to_string()
                            )).await;
                        }
                        "/help" => {
                            let help = "VAC Commands:\n\
                            \n\
                            VIL Engine:\n\
                            /vil      - Show VIL engine status\n\
                            /swarm    - Show swarm status\n\
                            /rulebook - Manage rulebooks\n\
                            /context  - Show context budget\n\
                            /runtime  - Show runtime status\n\
                            \n\
                            Session:\n\
                            /clear    - Clear conversation\n\
                            /new      - Start new session\n\
                            /sessions - List sessions\n\
                            /resume   - Resume from checkpoint\n\
                            \n\
                            Shortcuts:\n\
                            Ctrl+P    - Command palette\n\
                            Ctrl+C    - Quit\n\
                            Esc       - Cancel/Close";
                            let _ = input_tx.send(InputEvent::AddUserMessage(help.to_string())).await;
                        }
                        _ => {
                            let _ = input_tx.send(InputEvent::AddUserMessage(format!("Unknown command: {}. Type /help for available commands.", cmd))).await;
                        }
                    }
                }
                OutputEvent::AcceptTool(tc) => {
                    // Tool approved - in real implementation, this would be sent to VacEngine
                    let _ = input_tx.send(InputEvent::AddUserMessage(format!("✓ Tool approved: {}", tc.function.name))).await;
                    // Simulate tool execution
                    let _ = input_tx.send(InputEvent::ToolResult(vac_cli::tui2::ToolCallResult {
                        call: tc.clone(),
                        result: format!("Tool {} executed successfully", tc.function.name),
                        status: vac_cli::tui2::ToolCallResultStatus::Success,
                    })).await;
                }
                OutputEvent::RejectTool(tc, _) => {
                    let _ = input_tx.send(InputEvent::AddUserMessage(format!("✗ Tool rejected: {}", tc.function.name))).await;
                }
                OutputEvent::ResumeSession(session_id) => {
                    let _ = input_tx.send(InputEvent::SessionRestored {
                        id: session_id,
                        title: "Restored Session".to_string(),
                        messages: vec![],
                    }).await;
                }
                _ => {}
            }
        }
    });

    run_tui(
        input_rx,
        output_tx,
        None,
        shutdown_tx,
        Some(env!("CARGO_PKG_VERSION").to_string()),
        false,
        false,
        false,
        None,
        None,
        "default".to_string(),
        None,
        Some(Model::default()),
        None,
        (None, None, None),
        None,
        false,
        vec![],
        None,
    )
    .await?;

    Ok(())
}