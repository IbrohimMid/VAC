//! W8.C — diagnostic commands.
//!
//! Operator-facing probes that inspect VAC's own runtime state.
//! Everything in this module is read-only + headless.

use std::path::PathBuf;

pub async fn debug_tool_call(project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac debug-tool-call ───────────────────────");
    let sessions_dir = project_root.join(".vac").join("sessions");
    let Some(newest) = newest_session(&sessions_dir).await? else {
        println!("No session transcripts under {}.", sessions_dir.display());
        return Ok(());
    };
    println!("newest session: {}", newest.display());
    let body = tokio::fs::read_to_string(&newest).await?;
    let mut last_tool: Option<String> = None;
    for line in body.lines() {
        if line.contains("\"tool\"") || line.contains("ToolRequest") {
            last_tool = Some(line.to_string());
        }
    }
    match last_tool {
        Some(line) => {
            println!("last tool line:");
            println!("  {}", head(&line, 240));
        }
        None => println!("no tool entries in most-recent transcript."),
    }
    Ok(())
}

pub async fn heapdump(_project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac heapdump ──────────────────────────────");
    // Linux-only probe: read /proc/self/status for VmRSS / VmPeak.
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = tokio::fs::read_to_string("/proc/self/status").await {
            for line in text.lines() {
                if line.starts_with("VmRSS")
                    || line.starts_with("VmPeak")
                    || line.starts_with("VmSize")
                    || line.starts_with("VmData")
                    || line.starts_with("Threads")
                {
                    println!("  {line}");
                }
            }
            return Ok(());
        }
    }
    println!("  (not on linux or /proc unreadable — full heap profiling is W8.E)");
    Ok(())
}

pub async fn statusline(project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac statusline ────────────────────────────");
    let branch = git_branch(&project_root).await;
    let sessions_dir = project_root.join(".vac").join("sessions");
    let session_count = count_sessions(&sessions_dir).await.unwrap_or(0);
    println!(
        "  [{}] {} session(s)  cwd={}",
        branch.as_deref().unwrap_or("detached"),
        session_count,
        project_root.display(),
    );
    Ok(())
}

pub async fn good_claude(_project_root: PathBuf) -> anyhow::Result<()> {
    // Intentionally small easter-egg surface. Matches Claude Code's
    // `/good-claude`. Zero externalities — no network, no file I/O.
    println!("Good bot. ✦");
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────

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

async fn count_sessions(dir: &std::path::Path) -> anyhow::Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut n = 0usize;
    let mut rd = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
            n += 1;
        }
    }
    Ok(n)
}

async fn git_branch(root: &std::path::Path) -> Option<String> {
    // Async spawn so the CLI's tokio thread isn't pinned on git's
    // fork+exec wait.
    let out = tokio::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(root)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn good_claude_never_errors() {
        good_claude(PathBuf::from(".")).await.unwrap();
    }

    #[tokio::test]
    async fn heapdump_runs_without_error() {
        heapdump(PathBuf::from(".")).await.unwrap();
    }

    #[tokio::test]
    async fn statusline_clean_tempdir_shows_zero_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        statusline(tmp.path().to_path_buf()).await.unwrap();
    }

    #[tokio::test]
    async fn debug_tool_call_empty_project_ok() {
        let tmp = tempfile::tempdir().unwrap();
        debug_tool_call(tmp.path().to_path_buf()).await.unwrap();
    }

    #[tokio::test]
    async fn newest_session_picks_latest_mtime() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sessions");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let old = dir.join("a.jsonl");
        tokio::fs::write(&old, "x").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let new = dir.join("b.jsonl");
        tokio::fs::write(&new, "y").await.unwrap();
        let got = newest_session(&dir).await.unwrap().unwrap();
        assert_eq!(got, new);
    }

    #[tokio::test]
    async fn count_sessions_only_counts_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sessions");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("a.jsonl"), "").await.unwrap();
        tokio::fs::write(dir.join("b.jsonl"), "").await.unwrap();
        tokio::fs::write(dir.join("c.md"), "").await.unwrap();
        assert_eq!(count_sessions(&dir).await.unwrap(), 2);
    }
}
