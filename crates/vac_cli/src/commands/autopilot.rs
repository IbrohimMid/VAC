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

pub async fn execute_up(project_root: PathBuf, execute: bool) -> anyhow::Result<()> {
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
    let vac_config = vac_core::VacConfig::load_with_fallback(&project_root)?;

    // M1 — Dry-run by default (Stakpak-discipline). Require explicit
    // --execute to spawn the background daemon. Plan-only output is
    // scannable (≤7 lines) so operators can verify intent before
    // committing to 24/7 autonomous execution.
    if !execute {
        println!("Autopilot plan (dry-run; re-run with --execute to spawn):");
        println!("  Mode:    {}", config.mode);
        println!("  Env:     {}", vac_config.runtime.environment_mode);
        println!("  Exec:    {:?}", vac_config.runtime.execution_environment);
        println!("  Poll:    {}s", config.poll_interval_secs);
        println!("  PID:     (not written)");
        println!("  Log:     {}", project_root.join(LOG_FILE).display());
        return Ok(());
    }

    let exe = std::env::current_exe()?;
    std::fs::create_dir_all(project_root.join(".vac"))?;
    let log_path = project_root.join(LOG_FILE);
    let pid = if should_wrap_in_isolation(&vac_config.runtime) {
        let isolation =
            vac_runtime::IsolationManager::new(project_root.clone(), vac_config.runtime.clone());
        let args = vec![
            "--project".to_string(),
            project_root.display().to_string(),
            "autopilot".to_string(),
            "run".to_string(),
        ];
        isolation.spawn_background(&exe, &args, &log_path, std::collections::HashMap::new())?
    } else {
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
        child.id()
    };
    std::fs::write(&pid_path, pid.to_string())?;

    // Lifecycle event: started
    append_event(
        &project_root,
        &format!(
            "STARTED pid={pid} mode={} execution={:?}",
            config.mode, vac_config.runtime.execution_environment
        ),
    );

    println!("✓ Autopilot started (PID {pid})");
    println!("  Mode:     {}", config.mode);
    println!("  Interval: {}s", config.poll_interval_secs);
    println!("  Env:      {}", vac_config.runtime.environment_mode);
    println!("  Exec:     {:?}", vac_config.runtime.execution_environment);
    println!("  Log:      {}", log_path.display());
    Ok(())
}

pub async fn execute_run(project_root: PathBuf) -> anyhow::Result<()> {
    let vac_config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    if should_wrap_in_isolation(&vac_config.runtime) {
        let exe = std::env::current_exe()?;
        let isolation =
            vac_runtime::IsolationManager::new(project_root.clone(), vac_config.runtime.clone());
        let args = vec![
            "--project".to_string(),
            project_root.display().to_string(),
            "autopilot".to_string(),
            "run".to_string(),
        ];
        let code =
            isolation.run_foreground(&exe, &args, false, std::collections::HashMap::new())?;
        if code != 0 {
            anyhow::bail!("Isolated autopilot exited with code {code}");
        }
        return Ok(());
    }

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
            if let Ok(mut sigterm) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
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
                    vac_runtime::AutopilotState::Backoff { until } => {
                        format!("Backoff until {until}")
                    }
                    vac_runtime::AutopilotState::Failed { error } => format!("Failed: {error}"),
                };
            } else if let Ok(state) = serde_json::from_str::<vac_runtime::AutopilotState>(&content)
            {
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
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({ "status": "stopped", "internal_state": state_json })
                )?
            );
            return Ok(());
        }
        println!("Autopilot: stopped");
        return Ok(());
    }

    let pid: u32 = std::fs::read_to_string(&pid_path)?.trim().parse()?;
    if is_running(pid) {
        let config = AutopilotConfig::load(&project_root)?;
        let vac_config = vac_core::VacConfig::load_with_fallback(&project_root)?;
        if format == "json" {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "running",
                    "pid": pid,
                    "mode": config.mode,
                    "environment_mode": vac_config.runtime.environment_mode,
                    "execution_environment": vac_config.runtime.execution_environment,
                    "poll_interval_secs": config.poll_interval_secs,
                    "internal_state": state_json,
                    "log": project_root.join(LOG_FILE).display().to_string()
                }))?
            );
            return Ok(());
        }
        println!("Autopilot: running");
        println!("  PID:    {pid}");
        println!("  Mode:   {}", config.mode);
        println!("  Env:    {}", vac_config.runtime.environment_mode);
        println!("  Exec:   {:?}", vac_config.runtime.execution_environment);
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
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({ "status": "stopped", "stale_pid_removed": true })
                )?
            );
            return Ok(());
        }
        println!("Autopilot: stopped (stale PID removed)");
    }
    Ok(())
}

fn should_wrap_in_isolation(runtime: &vac_core::RuntimeConfig) -> bool {
    runtime.execution_environment != vac_core::ExecutionEnvironment::Host
        && std::env::var("VAC_SKIP_ISOLATION_WRAPPER").ok().as_deref() != Some("1")
}

fn append_event(project_root: &std::path::Path, event: &str) {
    use std::io::Write;
    let log_path = project_root.join(LOG_FILE);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let _ = writeln!(f, "[{ts}] AUTOPILOT {event}");
    }
}

#[cfg(unix)]
fn is_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn is_running(_pid: u32) -> bool {
    false
}

pub async fn execute_schedule(
    project_root: PathBuf,
    action: crate::ScheduleAction,
) -> anyhow::Result<()> {
    let schedules_file = project_root.join(".vac/autopilot.schedules.toml");

    #[derive(serde::Deserialize, serde::Serialize, Default)]
    struct ScheduleDoc {
        #[serde(default)]
        schedules: Vec<vac_core::config::ScheduleEntry>,
    }

    let mut doc: ScheduleDoc = match std::fs::read_to_string(&schedules_file) {
        Ok(content) => toml::from_str(&content)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ScheduleDoc::default(),
        Err(e) => return Err(e.into()),
    };

    match action {
        crate::ScheduleAction::List => {
            if doc.schedules.is_empty() {
                println!("No schedules configured.");
            } else {
                for s in &doc.schedules {
                    println!("ID: {}", s.id);
                    println!("  Cron: {}", s.cron);
                    println!("  Task: {}", s.task);
                    if let Some(r) = &s.profile {
                        println!("  Profile: {}", r);
                    }
                    if s.disabled {
                        println!("  [DISABLED]");
                    }
                    println!();
                }
            }
        }
        crate::ScheduleAction::Add { id, cron, task, rulebook } => {
            if doc.schedules.iter().any(|s| s.id == id) {
                anyhow::bail!("Schedule ID '{}' already exists", id);
            }
            // validate cron
            let fields: Vec<&str> = cron.split_whitespace().collect();
            if fields.len() != 5 && !cron.starts_with('@') {
                anyhow::bail!("Invalid cron expression");
            }
            doc.schedules.push(vac_core::config::ScheduleEntry {
                id: id.clone(),
                cron: cron.clone(),
                task: task.clone(),
                profile: rulebook,
                disabled: false,
            });
            let content = toml::to_string_pretty(&doc)?;
            if let Some(parent) = schedules_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&schedules_file, content)?;
            println!("Added schedule '{}'", id);
        }
        crate::ScheduleAction::Remove { id } => {
            let len_before = doc.schedules.len();
            doc.schedules.retain(|s| s.id != id);
            if doc.schedules.len() == len_before {
                anyhow::bail!("Schedule ID '{}' not found", id);
            }
            let content = toml::to_string_pretty(&doc)?;
            std::fs::write(&schedules_file, content)?;
            println!("Removed schedule '{}'", id);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn kill_process(pid: u32) -> anyhow::Result<()> {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    Ok(())
}

#[cfg(not(unix))]
fn kill_process(_pid: u32) -> anyhow::Result<()> {
    anyhow::bail!("Process management not supported on this platform")
}
