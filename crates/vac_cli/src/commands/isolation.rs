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

pub async fn execute_wrap(project_root: PathBuf, command: Vec<String>) -> anyhow::Result<()> {
    let mut config = vac_core::VacConfig::load_with_fallback(&project_root)?;

    let (binary, args, mut env) = if command.is_empty() {
        let current_exe = std::env::current_exe()?;
        let mut e = HashMap::new();
        e.insert("VAC_INSIDE_ISOLATION".to_string(), "1".to_string());

        // Add the VAC binary directory to allowed_mounts
        if let Some(parent) = current_exe.parent() {
            config
                .runtime
                .allowed_mounts
                .push(parent.to_string_lossy().to_string());
        }

        (current_exe, vec!["interactive".to_string()], e)
    } else {
        (
            PathBuf::from(&command[0]),
            command[1..].to_vec(),
            HashMap::new(),
        )
    };

    let isolation =
        vac_runtime::IsolationManager::new(project_root.clone(), config.runtime.clone());

    if !isolation.is_isolated() {
        anyhow::bail!("Cannot wrap command: execution_environment is not isolated");
    }

    let code = isolation.run_foreground(&binary, &args, true, env)?;
    if code != 0 {
        anyhow::bail!("Wrapped command exited with code {code}");
    }
    Ok(())
}

pub async fn execute_clear_logs(project_root: PathBuf) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    let isolation = vac_runtime::IsolationManager::new(project_root, config.runtime);
    let path = isolation.log_path();
    if path.exists() {
        std::fs::File::create(&path)?;
        println!("Cleared isolation log at {}", path.display());
    } else {
        println!("No isolation log found at {}", path.display());
    }
    Ok(())
}

pub async fn execute_doctor(project_root: PathBuf, format: &str) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    let isolation = vac_runtime::IsolationManager::new(project_root, config.runtime);

    let is_json = format == "json";

    if !is_json {
        println!("VAC Isolation Doctor\n");
    }

    let mut checks = Vec::new();

    // 1. Check container runtime
    let runtime = isolation.container_runtime();
    let runtime_status = match std::process::Command::new(runtime)
        .arg("--version")
        .output()
    {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            ("OK", version)
        }
        _ => ("ERROR", format!("Could not execute `{runtime} --version`")),
    };
    checks.push(("Container Runtime", runtime_status));

    // 2. Check container image
    let image_status = match isolation.container_image() {
        Ok(img) => ("OK", img.to_string()),
        Err(_) => ("WARNING", "No container image configured".to_string()),
    };
    checks.push(("Container Image", image_status));

    // 3. Check mounts
    let mounts_status = match isolation.resolve_mounts() {
        Ok(mounts) => ("OK", format!("{} valid mounts", mounts.len())),
        Err(e) => ("ERROR", e.to_string()),
    };
    checks.push(("Mounts", mounts_status));

    if is_json {
        let mut json_obj = serde_json::Map::new();
        for (name, (status, msg)) in checks {
            json_obj.insert(
                name.to_string(),
                serde_json::json!({
                    "status": status,
                    "message": msg
                }),
            );
        }
        println!("{}", serde_json::to_string_pretty(&json_obj)?);
    } else {
        for (name, (status, msg)) in checks {
            let color_status = match status {
                "OK" => "\x1b[32m[OK]\x1b[0m",
                "WARNING" => "\x1b[33m[WARNING]\x1b[0m",
                "ERROR" => "\x1b[31m[ERROR]\x1b[0m",
                _ => status,
            };
            println!("{:<30} {} - {}", name, color_status, msg);
        }
    }

    Ok(())
}
