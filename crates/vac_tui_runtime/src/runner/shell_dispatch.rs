//! Shell command dispatch — ExecuteCommand handler extracted from the main loop.

use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::InputEvent;

/// Outcome of shell spec resolution — `Skip` means the caller should
/// `continue` the event loop (an error was already reported to the user).
pub(super) enum ShellSpecOutcome {
    Spec(Option<vac_runtime::IsolationLaunchSpec>),
    Skip,
}

/// Resolve the isolation launch spec for a command. Returns `Skip` if an error
/// was sent to the user and the caller should `continue` the outer loop.
pub(super) async fn resolve_shell_spec(
    cmd: &str,
    active_isolation_mode: &str,
    project_root: &PathBuf,
    input_tx: &mpsc::Sender<InputEvent>,
) -> ShellSpecOutcome {
    let runtime_project_root_for_shell = project_root.clone();
    let config_result = tokio::task::spawn_blocking(move || {
        vac_core::VacConfig::load_with_fallback(&runtime_project_root_for_shell)
    })
    .await
    .unwrap_or_else(|_| {
        Err(vac_core::error::VacError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "spawn_blocking failed",
        )))
    });

    let mut config = match config_result {
        Ok(c) => c,
        Err(err) => {
            let _ = input_tx
                .send(InputEvent::ShellError(
                    "system".to_string(),
                    format!("Failed to load runtime config for shell: {err}"),
                ))
                .await;
            return ShellSpecOutcome::Skip;
        }
    };

    let is_interactive = cmd.is_empty();

    let mut env_mode_str = active_isolation_mode;
    let owned_mode;
    if active_isolation_mode.starts_with("isolated") {
        owned_mode = if is_interactive {
            "isolated_interactive".to_string()
        } else {
            "isolated_batch".to_string()
        };
        env_mode_str = &owned_mode;
    }

    if let Ok(env_mode) =
        serde_json::from_str::<vac_core::ExecutionEnvironment>(&format!("\"{}\"", env_mode_str))
    {
        config.runtime.execution_environment = env_mode;
    }

    if active_isolation_mode.contains("(Rust)") {
        config
            .runtime
            .mount_presets
            .push(vac_core::config::MountPreset::Rust);
    } else if active_isolation_mode.contains("(Node)") {
        config
            .runtime
            .mount_presets
            .push(vac_core::config::MountPreset::Node);
    } else if active_isolation_mode.contains("(Python)") {
        config
            .runtime
            .mount_presets
            .push(vac_core::config::MountPreset::Python);
    }

    if config.runtime.execution_environment == vac_core::ExecutionEnvironment::IsolatedInteractive
        || config.runtime.execution_environment == vac_core::ExecutionEnvironment::IsolatedBatch
    {
        let isolation =
            vac_runtime::IsolationManager::new(project_root.clone(), config.runtime.clone());

        if is_interactive {
            match isolation.build_interactive_shell_spec() {
                Ok(spec) => ShellSpecOutcome::Spec(Some(spec)),
                Err(err) => {
                    let _ = input_tx
                        .send(InputEvent::ShellError(
                            "system".to_string(),
                            format!("Failed to prepare isolated shell: {err}"),
                        ))
                        .await;
                    ShellSpecOutcome::Skip
                }
            }
        } else {
            let env = std::collections::HashMap::new();
            match isolation.build_container_command(
                std::path::Path::new("sh"),
                &["-c".to_string(), cmd.to_string()],
                true,
                &env,
            ) {
                Ok(command) => {
                    let program = command.get_program().to_string_lossy().to_string();
                    let args = command
                        .get_args()
                        .map(|a| a.to_string_lossy().to_string())
                        .collect();
                    ShellSpecOutcome::Spec(Some(vac_runtime::IsolationLaunchSpec {
                        program,
                        args,
                        cwd: project_root.clone(),
                        env,
                    }))
                }
                Err(err) => {
                    let _ = input_tx
                        .send(InputEvent::ShellError(
                            "system".to_string(),
                            format!("Failed to build batch command: {err}"),
                        ))
                        .await;
                    ShellSpecOutcome::Skip
                }
            }
        }
    } else {
        ShellSpecOutcome::Spec(None)
    }
}

/// Launch a PTY command with the resolved shell spec.
pub(super) async fn launch_pty(
    cmd: String,
    shell_spec: Option<vac_runtime::IsolationLaunchSpec>,
    input_tx: &mpsc::Sender<InputEvent>,
    rows: u16,
    cols: u16,
) {
    let (shell_tx, mut shell_rx) = tokio::sync::mpsc::channel(100);
    let input_tx_inner = input_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = shell_rx.recv().await {
            match event {
                vac_shell::ShellEvent::Output(id, text) => {
                    let _ = input_tx_inner.send(InputEvent::ShellOutput(id, text)).await;
                }
                vac_shell::ShellEvent::Error(id, text) => {
                    let _ = input_tx_inner.send(InputEvent::ShellError(id, text)).await;
                }
                vac_shell::ShellEvent::Completed(id, code) => {
                    let _ = input_tx_inner
                        .send(InputEvent::ShellCompleted(id, code))
                        .await;
                }
                vac_shell::ShellEvent::WaitingForInput(id) => {
                    let _ = input_tx_inner
                        .send(InputEvent::ShellWaitingForInput(id))
                        .await;
                }
            }
        }
    });

    let shell_result = vac_shell::run_pty_command(cmd, shell_spec, shell_tx, rows, cols)
        .map_err(|err: Box<dyn std::error::Error>| err.to_string());

    match shell_result {
        Ok(shell) => {
            let _ = input_tx.send(InputEvent::ShellStarted(shell)).await;
        }
        Err(err_msg) => {
            let _ = input_tx
                .send(InputEvent::ShellError(
                    "system".to_string(),
                    format!("Failed to start shell: {err_msg}"),
                ))
                .await;
        }
    }
}
