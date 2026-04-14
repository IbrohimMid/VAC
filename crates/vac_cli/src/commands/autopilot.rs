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
    let log_path = project_root.join(LOG_FILE);
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let child = std::process::Command::new(exe)
        .args(["runtime", "start"])
        .current_dir(&project_root)
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .spawn()?;

    let pid = child.id();
    std::fs::create_dir_all(project_root.join(".vac"))?;
    std::fs::write(&pid_path, pid.to_string())?;

    // Lifecycle event: started
    append_event(&project_root, &format!("STARTED pid={pid} mode={}", config.mode));

    println!("✓ Autopilot started (PID {pid})");
    println!("  Mode:     {}", config.mode);
    println!("  Interval: {}s", config.poll_interval_secs);
    println!("  Log:      {}", log_path.display());
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

pub async fn execute_status(project_root: PathBuf) -> anyhow::Result<()> {
    let pid_path = project_root.join(PID_FILE);
    if !pid_path.exists() {
        println!("Autopilot: stopped");
        return Ok(());
    }

    let pid: u32 = std::fs::read_to_string(&pid_path)?.trim().parse()?;
    if is_running(pid) {
        let config = AutopilotConfig::load(&project_root)?;
        println!("Autopilot: running");
        println!("  PID:  {pid}");
        println!("  Mode: {}", config.mode);
        println!("  Log:  {}", project_root.join(LOG_FILE).display());
    } else {
        std::fs::remove_file(&pid_path)?;
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
