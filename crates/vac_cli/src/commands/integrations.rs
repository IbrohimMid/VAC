//! W8.B — integration commands.
//!
//! These commands bootstrap VAC's integration with external systems.
//! Actual OAuth / webhook wiring lives in follow-up milestones; the
//! commands here **print the exact steps** so a new operator can
//! stand up the integration without digging through docs.

use std::path::PathBuf;

const GITHUB_APP_MANIFEST_URL: &str = "https://github.com/settings/apps/new";
const SLACK_APP_MANIFEST_URL: &str = "https://api.slack.com/apps?new_app=1";

pub async fn install_github_app(_project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac install-github-app ────────────────────");
    println!("Open {GITHUB_APP_MANIFEST_URL}");
    println!("Register a new GitHub App with the following scope:");
    println!("  - Repository permissions: contents:read, pull_requests:write");
    println!("  - Subscribe to events: pull_request, issue_comment, push");
    println!("After install, run `vac auth login github` (coming in W7) to store the token.");
    Ok(())
}

pub async fn install_slack_app(_project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac install-slack-app ─────────────────────");
    println!("Open {SLACK_APP_MANIFEST_URL}");
    println!("Paste the manifest from docs/integrations/slack-manifest.yml");
    println!("Scopes required: chat:write, channels:read, channels:history");
    println!("After install, set SLACK_BOT_TOKEN in ~/.vac/env.toml");
    Ok(())
}

pub async fn reload_plugins(project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac reload-plugins ────────────────────────");
    let plugin_dir = project_root.join(".vac").join("plugins");
    if !plugin_dir.exists() {
        println!(
            "No plugin directory at {} — nothing to reload.",
            plugin_dir.display()
        );
        return Ok(());
    }
    let mut count = 0usize;
    let mut rd = tokio::fs::read_dir(&plugin_dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        if let Some(name) = entry.file_name().to_str() {
            if !name.starts_with('.') {
                count += 1;
                println!("  plugin: {name}");
            }
        }
    }
    println!("{count} plugin(s) scanned. Full reload wiring is W8.E scope.");
    Ok(())
}

pub async fn teleport(project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac teleport ──────────────────────────────");
    let sessions_dir = project_root.join(".vac").join("sessions");
    if !sessions_dir.exists() {
        println!("No prior sessions at {}.", sessions_dir.display());
        return Ok(());
    }
    let entries = collect_session_summaries(&sessions_dir).await?;
    if entries.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }
    println!("Recent sessions (tail-5):");
    for summary in entries.iter().take(5) {
        println!("  {summary}");
    }
    println!("Tip: `vac resume <session-id>` to re-enter one.");
    Ok(())
}

async fn collect_session_summaries(dir: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut rd = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let Ok(mtime) = meta.modified() else { continue };
        files.push((mtime, path));
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = Vec::new();
    for (_, p) in files {
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            out.push(stem.to_string());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn github_command_runs_to_completion() {
        install_github_app(PathBuf::from(".")).await.unwrap();
    }

    #[tokio::test]
    async fn slack_command_runs_to_completion() {
        install_slack_app(PathBuf::from(".")).await.unwrap();
    }

    #[tokio::test]
    async fn reload_plugins_empty_project_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        reload_plugins(tmp.path().to_path_buf()).await.unwrap();
    }

    #[tokio::test]
    async fn reload_plugins_scans_directory_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin_dir = tmp.path().join(".vac/plugins");
        tokio::fs::create_dir_all(&plugin_dir).await.unwrap();
        tokio::fs::create_dir(plugin_dir.join("sample-plugin"))
            .await
            .unwrap();
        reload_plugins(tmp.path().to_path_buf()).await.unwrap();
    }

    #[tokio::test]
    async fn teleport_with_no_sessions_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        teleport(tmp.path().to_path_buf()).await.unwrap();
    }

    #[tokio::test]
    async fn teleport_lists_jsonl_session_files() {
        let tmp = tempfile::tempdir().unwrap();
        let sdir = tmp.path().join(".vac/sessions");
        tokio::fs::create_dir_all(&sdir).await.unwrap();
        for name in &["aaaa.jsonl", "bbbb.jsonl", "ignoreme.txt"] {
            tokio::fs::write(sdir.join(name), "").await.unwrap();
        }
        let out = collect_session_summaries(&sdir).await.unwrap();
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|s| s == "aaaa" || s == "bbbb"));
    }
}
