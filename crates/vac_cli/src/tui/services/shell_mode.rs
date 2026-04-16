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

#[derive(Debug, Clone)]
pub enum ShellEvent {
    Output(String),
    Error(String),
    Completed(i32),
    WaitingForInput,
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
                let _ = output_tx
                    .blocking_send(ShellEvent::Error(format!("Failed to open PTY: {err}")));
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
                let _ = output_tx
                    .blocking_send(ShellEvent::Error(format!("Failed to spawn shell: {err}")));
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
                let _ = output_tx.blocking_send(ShellEvent::Error(format!(
                    "Failed to open PTY writer: {err}"
                )));
                return;
            }
        };

        let mut reader = match pair.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(err) => {
                let _ = output_tx.blocking_send(ShellEvent::Error(format!(
                    "Failed to open PTY reader: {err}"
                )));
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
        std::thread::spawn(move || {
            let mut first_output = true;
            let mut buf = vec![0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if first_output {
                            let _ = prompt_ready_tx.send(());
                            let _ = output_tx_clone.blocking_send(ShellEvent::WaitingForInput);
                            first_output = false;
                        }
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = output_tx_clone.blocking_send(ShellEvent::Output(text));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(err) => {
                        let _ = output_tx_clone
                            .blocking_send(ShellEvent::Error(format!("Read error: {err}")));
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
                let _ = output_tx.blocking_send(ShellEvent::Completed(code));
            }
            Err(err) => {
                let registry = get_process_registry();
                if let Ok(mut registry) = registry.lock() {
                    registry.remove(&command_id);
                }
                let _ = output_tx.blocking_send(ShellEvent::Error(format!("Wait error: {err}")));
                let _ = output_tx.blocking_send(ShellEvent::Completed(-1));
            }
        }
    });

    Ok(shell_cmd)
}
