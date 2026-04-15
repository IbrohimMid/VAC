//! `vac autopilot` — VIL-native autonomous runtime daemon management.
//!
//! Lifecycle: up → running → (task loop) → down
//! Policy modes: "monitor" (observe only) | "auto" (execute autonomously)
//! PID file: .vac/autopilot.pid
//! Event log: .vac/autopilot.log

use std::path::PathBuf;
use vac_core::config::AutopilotConfig;

const PID_FILE: &str = ".vac/autopilot.pid";
const LOG_FILE: &str = ".vac/autopilot.log";

pub async fn execute_up(project_root: PathBuf) -> anyhow::Result<()> {
    let pid_path = project_root.join(PID_FILE);
    if pid_path.exists() {
        let pid: u32 = std::fs::read_to_string(&pid_path)?.trim().parse()?;
        if is_running(pid) {
            println!("Autopilot already running (PID {pid})");
            return Ok(());
        }
        // Stale PID — clean up
        std::fs::remove_file(&pid_path)?;
    }

    let config = AutopilotConfig::load(&project_root)?;

    let exe = std::env::current_exe()?;
    std::fs::create_dir_all(project_root.join(".vac"))?;
    let log_path = project_root.join(LOG_FILE);
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let child = std::process::Command::new(exe)
        .args([
            "--project",
            project_root.to_str().unwrap_or("."),
            "autopilot",
            "run",
        ])
        .current_dir(&project_root)
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .spawn()?;

    let pid = child.id();
    std::fs::write(&pid_path, pid.to_string())?;

    // Lifecycle event: started
    append_event(&project_root, &format!("STARTED pid={pid} mode={}", config.mode));

    println!("✓ Autopilot started (PID {pid})");
    println!("  Mode:     {}", config.mode);
    println!("  Interval: {}s", config.poll_interval_secs);
    println!("  Log:      {}", log_path.display());
    Ok(())
}

pub async fn execute_run(project_root: PathBuf) -> anyhow::Result<()> {
    let controller = vac_runtime::AutopilotController::new(project_root).await?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let shutdown_tx_ctrlc = shutdown_tx.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = shutdown_tx_ctrlc.send(true);
    });

    #[cfg(unix)]
    {
        let shutdown_tx = shutdown_tx.clone();
        tokio::spawn(async move {
            if let Ok(mut sigterm) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                sigterm.recv().await;
                let _ = shutdown_tx.send(true);
            }
        });
    }

    controller.run(shutdown_rx).await?;
    Ok(())
}

pub async fn execute_down(project_root: PathBuf) -> anyhow::Result<()> {
    let pid_path = project_root.join(PID_FILE);
    if !pid_path.exists() {
        println!("Autopilot is not running");
        return Ok(());
    }

    let pid: u32 = std::fs::read_to_string(&pid_path)?.trim().parse()?;
    kill_process(pid)?;
    std::fs::remove_file(&pid_path)?;

    // Lifecycle event: stopped
    append_event(&project_root, &format!("STOPPED pid={pid}"));

    println!("✓ Autopilot stopped (PID {pid})");
    Ok(())
}

pub async fn execute_status(project_root: PathBuf, format: &str) -> anyhow::Result<()> {
    let pid_path = project_root.join(PID_FILE);
    let state_path = project_root.join(".vac/autopilot.state");

    let mut state_json = serde_json::json!({ "state": "unknown" });
    let mut state_str = "Unknown".to_string();
    let mut state_mode: Option<String> = None;
    let mut state_poll_interval: Option<u64> = None;
    let mut state_queue_len: Option<usize> = None;
    let mut state_last_error: Option<String> = None;

    if state_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&state_path) {
            if let Ok(sf) = serde_json::from_str::<vac_runtime::AutopilotStateFile>(&content) {
                state_mode = Some(sf.mode.clone());
                state_poll_interval = Some(sf.poll_interval_secs);
                state_queue_len = Some(sf.queue_len);
                state_last_error = sf.last_error.clone();
                state_json = serde_json::to_value(&sf)?;

                state_str = match &sf.state {
                    vac_runtime::AutopilotState::Idle => "Idle".to_string(),
                    vac_runtime::AutopilotState::Polling => "Polling".to_string(),
                    vac_runtime::AutopilotState::Executing { job_id, kind } => {
                        format!("Executing job {} ({})", job_id, kind)
                    }
                    vac_runtime::AutopilotState::WaitingApproval { tool_call_id } => {
                        format!("Waiting approval ({tool_call_id})")
                    }
                    vac_runtime::AutopilotState::Backoff { until } => format!("Backoff until {until}"),
                    vac_runtime::AutopilotState::Failed { error } => format!("Failed: {error}"),
                };
            } else if let Ok(state) = serde_json::from_str::<vac_runtime::AutopilotState>(&content) {
                match state {
                    vac_runtime::AutopilotState::Idle => {
                        state_str = "Idle".to_string();
                        state_json = serde_json::json!({ "state": "idle" });
                    }
                    vac_runtime::AutopilotState::Executing { job_id, kind } => {
                        state_str = format!("Executing job {} ({})", job_id, kind);
                        state_json = serde_json::json!({
                            "state": "executing",
                            "job_id": job_id,
                            "kind": kind
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    if !pid_path.exists() {
        if format == "json" {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "stopped", "internal_state": state_json }))?);
            return Ok(());
        }
        println!("Autopilot: stopped");
        return Ok(());
    }

    let pid: u32 = std::fs::read_to_string(&pid_path)?.trim().parse()?;
    if is_running(pid) {
        let config = AutopilotConfig::load(&project_root)?;
        if format == "json" {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "status": "running",
                "pid": pid,
                "mode": config.mode,
                "poll_interval_secs": config.poll_interval_secs,
                "internal_state": state_json,
                "log": project_root.join(LOG_FILE).display().to_string()
            }))?);
            return Ok(());
        }
        println!("Autopilot: running");
        println!("  PID:    {pid}");
        println!("  Mode:   {}", config.mode);
        if let Some(mode) = state_mode {
            println!("  State Mode: {mode}");
        }
        if let Some(interval) = state_poll_interval {
            println!("  Poll:   {interval}s");
        } else {
            println!("  Poll:   {}s", config.poll_interval_secs);
        }
        if let Some(len) = state_queue_len {
            println!("  Queue:  {len}");
        }
        println!("  State:  {}", state_str);
        if let Some(err) = state_last_error {
            println!("  Error:  {err}");
        }
        println!("  Log:    {}", project_root.join(LOG_FILE).display());
    } else {
        std::fs::remove_file(&pid_path)?;
        if format == "json" {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "stopped", "stale_pid_removed": true }))?);
            return Ok(());
        }
        println!("Autopilot: stopped (stale PID removed)");
    }
    Ok(())
}

fn append_event(project_root: &PathBuf, event: &str) {
    use std::io::Write;
    let log_path = project_root.join(LOG_FILE);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path) {
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let _ = writeln!(f, "[{ts}] AUTOPILOT {event}");
    }
}

#[cfg(unix)]
fn is_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn is_running(_pid: u32) -> bool { false }

#[cfg(unix)]
fn kill_process(pid: u32) -> anyhow::Result<()> {
    unsafe { libc::kill(pid as i32, libc::SIGTERM); }
    Ok(())
}

#[cfg(not(unix))]
fn kill_process(_pid: u32) -> anyhow::Result<()> {
    anyhow::bail!("Process management not supported on this platform")
}
