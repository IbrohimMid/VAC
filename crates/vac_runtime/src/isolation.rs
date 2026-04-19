use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, anyhow};
use vac_core::config::{ExecutionEnvironment, NetworkPolicy, RuntimeConfig};

pub const ISOLATION_LOG_FILE: &str = ".vac/isolation.log";

#[derive(Debug, Clone)]
pub struct IsolationLaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct IsolationManager {
    project_root: PathBuf,
    runtime: RuntimeConfig,
}

impl IsolationManager {
    pub fn new(project_root: PathBuf, runtime: RuntimeConfig) -> Self {
        Self {
            project_root,
            runtime,
        }
    }

    pub fn execution_environment(&self) -> ExecutionEnvironment {
        self.runtime.execution_environment
    }

    pub fn is_isolated(&self) -> bool {
        self.execution_environment() != ExecutionEnvironment::Host
    }

    pub fn container_runtime(&self) -> &str {
        self.runtime
            .container_runtime
            .as_deref()
            .unwrap_or("docker")
    }

    pub fn container_image(&self) -> anyhow::Result<&str> {
        self.runtime
            .container_image
            .as_deref()
            .ok_or_else(|| anyhow!("runtime.container_image must be set for isolated execution"))
    }

    pub fn log_path(&self) -> PathBuf {
        self.project_root.join(ISOLATION_LOG_FILE)
    }

    pub fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "environment_mode": self.runtime.environment_mode,
            "task_intent_mode": self.runtime.task_intent_mode,
            "execution_environment": self.runtime.execution_environment,
            "container_runtime": self.runtime.container_runtime,
            "container_image": self.runtime.container_image,
            "allowed_mounts": self.runtime.allowed_mounts,
            "allowed_env": self.runtime.allowed_env,
            "network_policy": self.runtime.network_policy,
            "log": self.log_path().display().to_string(),
        })
    }

    pub fn append_log(&self, line: &str) {
        use std::io::Write;
        let path = self.log_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
            let _ = writeln!(file, "[{ts}] {line}");
        }
    }

    pub fn resolve_mounts(&self) -> anyhow::Result<Vec<PathBuf>> {
        let mut mounts = vec![self.project_root.clone()];

        let mut explicit_mounts = self.runtime.allowed_mounts.clone();

        let mut presets = self.runtime.mount_presets.clone();
        if self.project_root.join("Cargo.toml").exists()
            && !presets.contains(&vac_core::config::MountPreset::Rust)
        {
            presets.push(vac_core::config::MountPreset::Rust);
        }

        for preset in presets {
            match preset {
                vac_core::config::MountPreset::Rust => {
                    if let Ok(home) = std::env::var("HOME") {
                        explicit_mounts.push(format!("{}/.cargo/registry", home));
                        explicit_mounts.push(format!("{}/.rustup", home));
                    }
                }
                vac_core::config::MountPreset::Node => {
                    // Placeholder for future node presets
                }
                vac_core::config::MountPreset::Python => {
                    // Placeholder for future python presets
                }
            }
        }

        for mount in &explicit_mounts {
            let resolved = if PathBuf::from(mount).is_absolute() {
                PathBuf::from(mount)
            } else {
                self.project_root.join(mount)
            };
            if !resolved.exists() {
                self.append_log(&format!("DENY mount={} reason=missing", resolved.display()));

                // If it was explicitly allowed by the user, fail.
                // We assume presets might be auto-included and thus okay to skip if missing.
                if self.runtime.allowed_mounts.contains(mount) {
                    return Err(anyhow!(
                        "Allowed mount '{}' does not exist",
                        resolved.display()
                    ));
                }
                continue;
            }
            mounts.push(resolved);
        }
        mounts.sort();
        mounts.dedup();
        Ok(mounts)
    }

    pub fn filter_allowed_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        for key in &self.runtime.allowed_env {
            match std::env::var(key) {
                Ok(value) => {
                    env.insert(key.clone(), value);
                }
                Err(_) => {
                    self.append_log(&format!("DENY env={} reason=missing", key));
                }
            }
        }
        env
    }

    pub fn build_container_command(
        &self,
        binary: &Path,
        args: &[String],
        tty: bool,
        extra_env: &HashMap<String, String>,
    ) -> anyhow::Result<Command> {
        if !self.is_isolated() {
            return Err(anyhow!("build_container_command called for host execution"));
        }

        let image = self.container_image()?;
        let mounts = self.resolve_mounts()?;
        let mut cmd = Command::new(self.container_runtime());
        cmd.arg("run").arg("--rm");

        if tty {
            cmd.arg("-it");
        }

        cmd.arg("-w").arg(&self.project_root);

        for mount in mounts {
            cmd.arg("-v")
                .arg(format!("{}:{}", mount.display(), mount.display()));
        }

        match self.runtime.network_policy {
            NetworkPolicy::RestrictedOffline => {
                cmd.args(["--network", "none"]);
            }
            NetworkPolicy::TrustedNetworked | NetworkPolicy::Inherit => {}
        }

        let mut env = self.filter_allowed_env();
        env.extend(extra_env.clone());
        env.insert("VAC_SKIP_ISOLATION_WRAPPER".to_string(), "1".to_string());
        env.insert(
            "VAC_ENVIRONMENT_MODE".to_string(),
            self.runtime.environment_mode.clone(),
        );

        for (key, value) in env {
            cmd.arg("-e").arg(format!("{key}={value}"));
        }

        cmd.arg(image);
        cmd.arg(binary);
        cmd.args(args);
        Ok(cmd)
    }

    pub fn build_interactive_shell_spec(&self) -> anyhow::Result<IsolationLaunchSpec> {
        if !self.is_isolated() {
            return Err(anyhow!(
                "interactive shell spec requested for host execution"
            ));
        }

        let image = self.container_image()?.to_string();
        let mounts = self.resolve_mounts()?;
        let mut args = vec!["run".to_string(), "--rm".to_string(), "-it".to_string()];
        args.push("-w".to_string());
        args.push(self.project_root.display().to_string());

        for mount in mounts {
            args.push("-v".to_string());
            args.push(format!("{}:{}", mount.display(), mount.display()));
        }

        match self.runtime.network_policy {
            NetworkPolicy::RestrictedOffline => {
                args.push("--network".to_string());
                args.push("none".to_string());
            }
            NetworkPolicy::TrustedNetworked | NetworkPolicy::Inherit => {}
        }

        let mut passthrough_env = self.filter_allowed_env();
        passthrough_env.insert("VAC_SKIP_ISOLATION_WRAPPER".to_string(), "1".to_string());
        passthrough_env.insert(
            "VAC_ENVIRONMENT_MODE".to_string(),
            self.runtime.environment_mode.clone(),
        );

        for (key, value) in &passthrough_env {
            args.push("-e".to_string());
            args.push(format!("{key}={value}"));
        }

        args.push(image);
        args.push("sh".to_string());
        args.push("-il".to_string());

        Ok(IsolationLaunchSpec {
            program: self.container_runtime().to_string(),
            args,
            cwd: self.project_root.clone(),
            env: HashMap::new(),
        })
    }

    pub fn run_foreground(
        &self,
        binary: &Path,
        args: &[String],
        tty: bool,
        extra_env: HashMap<String, String>,
    ) -> anyhow::Result<i32> {
        let mut cmd = self.build_container_command(binary, args, tty, &extra_env)?;
        self.append_log(&format!(
            "START foreground runtime={} image={:?} tty={tty}",
            self.container_runtime(),
            self.runtime.container_image
        ));
        let status = cmd
            .status()
            .context("failed to execute container runtime")?;
        let code = status.code().unwrap_or(-1);
        self.append_log(&format!("EXIT code={code}"));
        Ok(code)
    }

    pub fn spawn_background(
        &self,
        binary: &Path,
        args: &[String],
        log_file: &Path,
        extra_env: HashMap<String, String>,
    ) -> anyhow::Result<u32> {
        let mut cmd = self.build_container_command(binary, args, false, &extra_env)?;
        if let Some(parent) = log_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;
        let child = cmd
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()
            .context("failed to spawn isolated background process")?;
        self.append_log(&format!(
            "START background pid={} runtime={} image={:?}",
            child.id(),
            self.container_runtime(),
            self.runtime.container_image
        ));
        Ok(child.id())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mounts_rejects_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = RuntimeConfig {
            execution_environment: ExecutionEnvironment::IsolatedBatch,
            container_image: Some("ghcr.io/example/vac:latest".to_string()),
            allowed_mounts: vec!["missing".to_string()],
            ..RuntimeConfig::default()
        };
        let manager = IsolationManager::new(dir.path().to_path_buf(), runtime);
        assert!(manager.resolve_mounts().is_err());
    }

    #[test]
    fn restricted_offline_adds_network_none() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = RuntimeConfig {
            execution_environment: ExecutionEnvironment::IsolatedBatch,
            container_runtime: Some("docker".to_string()),
            container_image: Some("ghcr.io/example/vac:latest".to_string()),
            network_policy: NetworkPolicy::RestrictedOffline,
            ..RuntimeConfig::default()
        };
        let manager = IsolationManager::new(dir.path().to_path_buf(), runtime);
        let cmd = manager
            .build_container_command(
                Path::new("/usr/bin/vac"),
                &["runtime".to_string(), "start".to_string()],
                false,
                &HashMap::new(),
            )
            .unwrap();
        let rendered = format!("{cmd:?}");
        assert!(rendered.contains("--network"));
        assert!(rendered.contains("none"));
    }

    #[test]
    fn interactive_shell_spec_uses_container_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = RuntimeConfig {
            execution_environment: ExecutionEnvironment::IsolatedInteractive,
            container_runtime: Some("podman".to_string()),
            container_image: Some("ghcr.io/example/vac:latest".to_string()),
            ..RuntimeConfig::default()
        };
        let manager = IsolationManager::new(dir.path().to_path_buf(), runtime);
        let spec = manager.build_interactive_shell_spec().unwrap();
        assert_eq!(spec.program, "podman");
        assert!(spec.args.iter().any(|arg| arg == "sh"));
        assert!(spec.args.iter().any(|arg| arg == "-il"));
    }
}
