//! W8.D — plan + memory commands.
//!
//! Operator-facing surfaces for planning scaffolds + memory
//! inspection. Everything read-only except `sandbox-toggle`, which
//! mutates the runtime config.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub async fn thinkback(project_root: PathBuf, limit: usize) -> anyhow::Result<()> {
    println!("── vac thinkback ─────────────────────────────");
    let sessions_dir = project_root.join(".vac").join("sessions");
    let Some(newest) = newest_session(&sessions_dir).await? else {
        println!("No prior session. Start one with `vac session`.");
        return Ok(());
    };
    println!("session: {}", newest.display());
    let body = tokio::fs::read_to_string(&newest).await?;
    let lines: Vec<&str> = body.lines().collect();
    let total = lines.len();
    let window = lines
        .iter()
        .rev()
        .take(limit.max(1))
        .copied()
        .collect::<Vec<&str>>();
    println!("last {} of {} lines:", window.len(), total);
    for line in window.into_iter().rev() {
        println!("  {}", head(line, 240));
    }
    Ok(())
}

pub async fn ultraplan(_project_root: PathBuf, goal: String) -> anyhow::Result<()> {
    // `ultraplan` is deliberately offline: it prints a canonical
    // plan skeleton the operator fills in. The LLM-backed version
    // lives under `vac plan --remote …` (P2); this command is the
    // no-network fallback.
    println!("── vac ultraplan ─────────────────────────────");
    println!("goal: {goal}");
    for (i, stage) in [
        "Context",
        "Constraints",
        "Decisions",
        "Milestones",
        "Risks",
        "Verification",
    ]
    .iter()
    .enumerate()
    {
        println!("  {}. {stage}", i + 1);
        println!("     - …");
    }
    println!("hint: copy into docs/plan-<topic>.md and flesh out.");
    Ok(())
}

pub async fn sandbox_toggle(project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac sandbox-toggle ────────────────────────");
    let config_path = project_root.join(".vac").join("sandbox.toml");
    let current = load_sandbox(&config_path).await;
    let next = match current.environment_mode.as_str() {
        "host" => "isolated",
        "isolated" => "restricted-offline",
        "restricted-offline" => "trusted-networked",
        _ => "host",
    };
    let next_config = SandboxConfig {
        environment_mode: next.to_string(),
    };
    save_sandbox(&config_path, &next_config).await?;
    println!(
        "  environment_mode: {} → {}",
        current.environment_mode, next
    );
    println!("  wrote {}", config_path.display());
    Ok(())
}

pub async fn rewind(project_root: PathBuf, limit: usize) -> anyhow::Result<()> {
    println!("── vac rewind ────────────────────────────────");
    let sessions_dir = project_root.join(".vac").join("sessions");
    if !sessions_dir.exists() {
        println!("No session history yet.");
        return Ok(());
    }
    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut rd = tokio::fs::read_dir(&sessions_dir).await?;
    while let Some(e) = rd.next_entry().await? {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
            continue;
        }
        let meta = e.metadata().await?;
        if let Ok(m) = meta.modified() {
            entries.push((m, p));
        }
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    println!("resume candidates (newest first, top {limit}):");
    for (_, p) in entries.iter().take(limit.max(1)) {
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            println!("  {stem}");
        }
    }
    println!("resume one via: vac resume --file {{id}}");
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SandboxConfig {
    environment_mode: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            environment_mode: "host".into(),
        }
    }
}

async fn load_sandbox(path: &Path) -> SandboxConfig {
    let Ok(raw) = tokio::fs::read_to_string(path).await else {
        return SandboxConfig::default();
    };
    toml::from_str(&raw).unwrap_or_default()
}

async fn save_sandbox(path: &Path, cfg: &SandboxConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let raw = toml::to_string(cfg)?;
    tokio::fs::write(path, raw).await?;
    Ok(())
}

fn head(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

async fn newest_session(dir: &std::path::Path) -> anyhow::Result<Option<PathBuf>> {
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    let mut rd = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let Ok(mtime) = meta.modified() else { continue };
        if newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
            newest = Some((mtime, p));
        }
    }
    Ok(newest.map(|(_, p)| p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ultraplan_renders_template() {
        ultraplan(PathBuf::from("."), "refactor auth".into())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn thinkback_with_no_session_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        thinkback(tmp.path().to_path_buf(), 5).await.unwrap();
    }

    #[tokio::test]
    async fn thinkback_prints_tail_of_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".vac/sessions");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let body = (0..20)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        tokio::fs::write(dir.join("s.jsonl"), body).await.unwrap();
        thinkback(tmp.path().to_path_buf(), 3).await.unwrap();
    }

    #[tokio::test]
    async fn sandbox_toggle_cycles_modes() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = tmp.path().join(".vac/sandbox.toml");
        // First call: host → isolated.
        sandbox_toggle(tmp.path().to_path_buf()).await.unwrap();
        assert_eq!(load_sandbox(&cfg).await.environment_mode, "isolated");
        // Second call: isolated → restricted-offline.
        sandbox_toggle(tmp.path().to_path_buf()).await.unwrap();
        assert_eq!(
            load_sandbox(&cfg).await.environment_mode,
            "restricted-offline",
        );
        // Third call: → trusted-networked.
        sandbox_toggle(tmp.path().to_path_buf()).await.unwrap();
        assert_eq!(
            load_sandbox(&cfg).await.environment_mode,
            "trusted-networked",
        );
        // Fourth call: back to host.
        sandbox_toggle(tmp.path().to_path_buf()).await.unwrap();
        assert_eq!(load_sandbox(&cfg).await.environment_mode, "host");
    }

    #[tokio::test]
    async fn rewind_empty_project_ok() {
        let tmp = tempfile::tempdir().unwrap();
        rewind(tmp.path().to_path_buf(), 5).await.unwrap();
    }

    #[tokio::test]
    async fn rewind_lists_sessions_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".vac/sessions");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("old.jsonl"), "").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        tokio::fs::write(dir.join("new.jsonl"), "").await.unwrap();
        rewind(tmp.path().to_path_buf(), 5).await.unwrap();
    }

    #[tokio::test]
    async fn load_sandbox_missing_returns_default_host() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = load_sandbox(&tmp.path().join("nope.toml")).await;
        assert_eq!(cfg.environment_mode, "host");
    }
}
