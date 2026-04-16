//! `vac isolation` — execution boundary inspection and containerized command runner.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub async fn execute_status(project_root: PathBuf, format: &str) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    let isolation = vac_runtime::IsolationManager::new(project_root, config.runtime);
    let json = isolation.status_json();

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    println!("VAC Isolation Status");
    println!(
        "  execution_environment: {:?}",
        json.get("execution_environment")
            .unwrap_or(&serde_json::Value::Null)
    );
    println!(
        "  environment_mode:      {}",
        json.get("environment_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("-")
    );
    println!(
        "  container_runtime:     {}",
        json.get("container_runtime")
            .and_then(|v| v.as_str())
            .unwrap_or("<default: docker>")
    );
    println!(
        "  container_image:       {}",
        json.get("container_image")
            .and_then(|v| v.as_str())
            .unwrap_or("<unset>")
    );
    println!(
        "  network_policy:        {}",
        json.get("network_policy")
            .and_then(|v| v.as_str())
            .unwrap_or("-")
    );
    println!(
        "  log:                   {}",
        isolation.log_path().display()
    );
    Ok(())
}

pub async fn execute_logs(project_root: PathBuf) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    let isolation = vac_runtime::IsolationManager::new(project_root, config.runtime);
    let path = isolation.log_path();
    if !path.exists() {
        println!("No isolation log found at {}", path.display());
        return Ok(());
    }
    print!("{}", std::fs::read_to_string(path)?);
    Ok(())
}

pub async fn execute_run(
    project_root: PathBuf,
    command: Vec<String>,
    tty: Option<bool>,
) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    let isolation =
        vac_runtime::IsolationManager::new(project_root.clone(), config.runtime.clone());
    if !isolation.is_isolated() {
        anyhow::bail!(
            "runtime.execution_environment is host; set it to isolated_interactive or isolated_batch"
        );
    }
    if command.is_empty() {
        anyhow::bail!("No command supplied. Usage: vac isolation run -- <command> [args...]");
    }

    let binary = Path::new(&command[0]);
    let args = command[1..].to_vec();
    let tty = tty.unwrap_or(matches!(
        config.runtime.execution_environment,
        vac_core::ExecutionEnvironment::IsolatedInteractive
    ));
    let code = isolation.run_foreground(binary, &args, tty, HashMap::new())?;
    if code != 0 {
        anyhow::bail!("Isolated command exited with code {code}");
    }
    Ok(())
}
