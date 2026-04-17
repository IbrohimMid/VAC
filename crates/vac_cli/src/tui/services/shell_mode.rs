use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use vac_runtime::IsolationLaunchSpec;

static PROCESS_REGISTRY: std::sync::OnceLock<Arc<Mutex<HashMap<String, u32>>>> =
    std::sync::OnceLock::new();

fn get_process_registry() -> Arc<Mutex<HashMap<String, u32>>> {
    PROCESS_REGISTRY
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

// ========== Shell lifecycle & prompt detection (Unit 6, Wave 3.4) ==========

/// Lifecycle state shown in the shell footer strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellLifecycle {
    /// Shell is running, no special state.
    Running,
    /// Shell appears idle at a prompt (prompt-ready detection fired).
    PromptReady,
    /// Process exited normally.
    Exited(i32),
    /// Process was killed (SIGKILL / forced termination).
    Killed,
    /// An error prevented the shell from starting or continuing.
    Error(String),
}

impl ShellLifecycle {
    /// Human-readable label for the footer hint.
    pub fn label(&self) -> String {
        match self {
            Self::Running => "Running".into(),
            Self::PromptReady => "Prompt ready".into(),
            Self::Exited(code) => format!("Exited({})", code),
            Self::Killed => "Killed".into(),
            Self::Error(msg) => format!("Error: {}", msg),
        }
    }
}

/// Detect whether `text` ends with a shell prompt indicator.
///
/// Matches common prompt suffixes: `$ `, `# `, `> `, `% `,
/// as well as Windows-style `>` at the very end.
pub fn detect_prompt_ready(text: &str) -> bool {
    let trimmed = text.trim_end_matches('\n');
    if trimmed.is_empty() {
        return false;
    }
    let last_line = trimmed.lines().next_back().unwrap_or("");
    last_line.ends_with("$ ")
        || last_line.ends_with("# ")
        || last_line.ends_with("> ")
        || last_line.ends_with("% ")
        || last_line.ends_with(">")
}

/// Detect whether the last line of output is a password/passphrase prompt.
///
/// When true, the TUI should suppress input echo (show dots or nothing).
pub fn detect_password_prompt(text: &str) -> bool {
    let trimmed = text.trim_end_matches('\n');
    if trimmed.is_empty() {
        return false;
    }
    let last_line = trimmed
        .lines()
        .next_back()
        .unwrap_or("")
        .to_ascii_lowercase();
    last_line.contains("password")
        || last_line.contains("passphrase")
        || last_line.contains("pin:")
        || last_line.contains("secret:")
        || (last_line.contains("enter") && last_line.contains("key"))
}

#[derive(Debug, Clone)]
pub enum ShellEvent {
    Output(String, String),
    Error(String, String),
    Completed(String, i32),
    WaitingForInput(String),
}

#[derive(Debug, Clone)]
pub struct ShellCommand {
    pub id: String,
    pub command: String,
    pub stdin_tx: mpsc::Sender<String>,
}

impl ShellCommand {
    pub fn send_input(&self, input: String) {
        let tx = self.stdin_tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(input).await;
        });
    }

    pub fn kill(&self) -> Result<(), Box<dyn std::error::Error>> {
        for _ in 0..3 {
            let _ = self.stdin_tx.try_send("\u{3}".to_string());
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let registry = get_process_registry();
        if let Ok(registry) = registry.lock()
            && let Some(&pid) = registry.get(&self.id)
        {
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args([&pid.to_string()])
                    .status();
                std::thread::sleep(std::time::Duration::from_millis(100));
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .status();
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .status();
            }
        }
        Ok(())
    }
}

pub fn run_pty_command(
    command: String,
    launch_spec: Option<IsolationLaunchSpec>,
    output_tx: mpsc::Sender<ShellEvent>,
    rows: u16,
    cols: u16,
) -> Result<ShellCommand, Box<dyn std::error::Error>> {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use std::io::Read;

    let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(100);
    let command_id = uuid::Uuid::new_v4().to_string();

    let shell_cmd = ShellCommand {
        id: command_id.clone(),
        command: command.clone(),
        stdin_tx: stdin_tx.clone(),
    };

    std::thread::spawn(move || {
        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(pair) => pair,
            Err(err) => {
                let _ = output_tx.blocking_send(ShellEvent::Error(
                    command_id.clone(),
                    format!("Failed to open PTY: {err}"),
                ));
                return;
            }
        };

        let cmd = if let Some(spec) = launch_spec {
            let mut cmd = CommandBuilder::new(&spec.program);
            cmd.cwd(spec.cwd);
            cmd.args(spec.args);
            for (key, value) in spec.env {
                cmd.env(key, value);
            }
            cmd
        } else {
            let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let shell = if cfg!(windows) {
                std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
            } else {
                std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
            };

            let mut cmd = CommandBuilder::new(&shell);
            cmd.cwd(current_dir);
            if !cfg!(windows) {
                cmd.args(["-il"]);
            }
            cmd
        };

        let mut child = match pair.slave.spawn_command(cmd) {
            Ok(child) => child,
            Err(err) => {
                let _ = output_tx.blocking_send(ShellEvent::Error(
                    command_id.clone(),
                    format!("Failed to spawn shell: {err}"),
                ));
                return;
            }
        };

        if let Some(pid) = child.process_id() {
            let registry = get_process_registry();
            if let Ok(mut registry) = registry.lock() {
                registry.insert(command_id.clone(), pid);
            }
        }

        let mut writer = match pair.master.take_writer() {
            Ok(writer) => writer,
            Err(err) => {
                let _ = output_tx.blocking_send(ShellEvent::Error(
                    command_id.clone(),
                    format!("Failed to open PTY writer: {err}"),
                ));
                return;
            }
        };

        let mut reader = match pair.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(err) => {
                let _ = output_tx.blocking_send(ShellEvent::Error(
                    command_id.clone(),
                    format!("Failed to open PTY reader: {err}"),
                ));
                return;
            }
        };

        let initial_command = if command.trim().is_empty() {
            None
        } else {
            Some(command.clone())
        };
        let (prompt_ready_tx, prompt_ready_rx) = std::sync::mpsc::channel::<()>();

        let output_tx_clone = output_tx.clone();
        let command_id_for_reader = command_id.clone();
        std::thread::spawn(move || {
            let mut first_output = true;
            let mut buf = vec![0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = output_tx_clone.blocking_send(ShellEvent::Output(
                            command_id_for_reader.clone(),
                            text.clone(),
                        ));

                        let has_prompt = detect_prompt_ready(&text) || text.contains("Press Enter");
                        if first_output || has_prompt {
                            let _ = prompt_ready_tx.send(());
                            let _ = output_tx_clone.blocking_send(ShellEvent::WaitingForInput(
                                command_id_for_reader.clone(),
                            ));
                            first_output = false;
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(err) => {
                        let _ = output_tx_clone.blocking_send(ShellEvent::Error(
                            command_id_for_reader.clone(),
                            format!("Read error: {err}"),
                        ));
                        break;
                    }
                }
            }
        });

        std::thread::spawn(move || {
            if let Some(cmd) = initial_command {
                let _ = prompt_ready_rx.recv_timeout(std::time::Duration::from_secs(5));
                std::thread::sleep(std::time::Duration::from_millis(250));
                #[cfg(windows)]
                let line_ending = "\r";
                #[cfg(not(windows))]
                let line_ending = "\n";
                let _ = write!(writer, "{}{}", cmd, line_ending);
                let _ = writer.flush();
            }

            while let Some(input) = stdin_rx.blocking_recv() {
                let _ = write!(writer, "{}", input);
                let _ = writer.flush();
            }
        });

        match child.wait() {
            Ok(status) => {
                let code = status.exit_code() as i32;
                let registry = get_process_registry();
                if let Ok(mut registry) = registry.lock() {
                    registry.remove(&command_id);
                }
                let _ = output_tx.blocking_send(ShellEvent::Completed(command_id.clone(), code));
            }
            Err(err) => {
                let registry = get_process_registry();
                if let Ok(mut registry) = registry.lock() {
                    registry.remove(&command_id);
                }
                let _ = output_tx.blocking_send(ShellEvent::Error(
                    command_id.clone(),
                    format!("Wait error: {err}"),
                ));
                let _ = output_tx.blocking_send(ShellEvent::Completed(command_id.clone(), -1));
            }
        }
    });

    Ok(shell_cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_prompt_ready_matches_common_suffixes() {
        assert!(detect_prompt_ready("user@host:~$ "));
        assert!(detect_prompt_ready("root@host:/# "));
        assert!(detect_prompt_ready("mysql> "));
        assert!(detect_prompt_ready("(venv) user@host:~% "));
        assert!(detect_prompt_ready("C:\\Users\\me>"));
    }

    #[test]
    fn detect_prompt_ready_rejects_non_prompts() {
        assert!(!detect_prompt_ready("compiling crate..."));
        assert!(!detect_prompt_ready(""));
        assert!(!detect_prompt_ready("hello world\n"));
    }

    #[test]
    fn detect_password_prompt_matches_common_patterns() {
        assert!(detect_password_prompt("Password: "));
        assert!(detect_password_prompt(
            "Enter passphrase for key '/home/user/.ssh/id_rsa': "
        ));
        assert!(detect_password_prompt("[sudo] password for user: "));
        assert!(detect_password_prompt("PIN: "));
        assert!(detect_password_prompt("Enter your secret: "));
        assert!(detect_password_prompt("Enter decryption key: "));
    }

    #[test]
    fn detect_password_prompt_rejects_normal_output() {
        assert!(!detect_password_prompt("user@host:~$ "));
        assert!(!detect_password_prompt("Downloading file..."));
        assert!(!detect_password_prompt(""));
    }

    #[test]
    fn lifecycle_labels_are_human_readable() {
        assert_eq!(ShellLifecycle::Running.label(), "Running");
        assert_eq!(ShellLifecycle::PromptReady.label(), "Prompt ready");
        assert_eq!(ShellLifecycle::Exited(0).label(), "Exited(0)");
        assert_eq!(ShellLifecycle::Exited(1).label(), "Exited(1)");
        assert_eq!(ShellLifecycle::Killed.label(), "Killed");
        assert_eq!(ShellLifecycle::Error("boom".into()).label(), "Error: boom");
    }
}
