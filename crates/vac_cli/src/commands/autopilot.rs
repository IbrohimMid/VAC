//! `vac autopilot` — manage background daemon.

use std::path::PathBuf;
use vac_core::config::AutopilotConfig;

const PID_FILE: &str = ".vac/autopilot.pid";

pub async fn execute_up(project_root: PathBuf) -> anyhow::Result<()> {
    let pid_path = project_root.join(PID_FILE);
    if pid_path.exists() {
        let pid: u32 = std::fs::read_to_string(&pid_path)?.trim().parse()?;
        if is_running(pid) {
            println!("Autopilot already running (PID {pid})");
            return Ok(());
        }
    }

    let _config = AutopilotConfig::load(&project_root)?;

    // Spawn detached process
    let exe = std::env::current_exe()?;
    let child = std::process::Command::new(exe)
        .args(["runtime", "start"])
        .current_dir(&project_root)
        .spawn()?;

    let pid = child.id();
    std::fs::create_dir_all(project_root.join(".vac"))?;
    std::fs::write(&pid_path, pid.to_string())?;
    println!("✓ Autopilot started (PID {pid})");
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
        println!("Autopilot: running (PID {pid})");
    } else {
        std::fs::remove_file(&pid_path)?;
        println!("Autopilot: stopped (stale PID file removed)");
    }
    Ok(())
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
