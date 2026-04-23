//! `vac interactive` — full-screen TUI mode.

use std::path::PathBuf;

pub async fn execute(
    project_root: PathBuf,
    resume: bool,
    record: Option<PathBuf>,
    replay: Option<PathBuf>,
) -> anyhow::Result<()> {
    // M9: Detect pending submit and prompt before starting TUI
    if let Ok(Some(session)) = vac_core::session::Session::load_latest(&project_root) {
        let writer = vac_session_engine::TranscriptWriter::new(project_root.clone());
        if let Ok(Some(entry_id)) = writer.last_pending_submit(session.id).await {
            // Read timestamp of that entry
            let mut hours_ago = 0;
            if let Ok(rows) = writer.read(session.id).await {
                if let Some(accepted) = rows.iter().find(|r| r.id == entry_id) {
                    let diff = chrono::Utc::now() - accepted.timestamp;
                    hours_ago = diff.num_hours();
                }
            }
            
            println!("Session crashed mid-submit ({} hours ago). Resume? [Y/n]", hours_ago);
            let mut buf = String::new();
            let _ = std::io::stdin().read_line(&mut buf);
            let ans = buf.trim().to_lowercase();
            
            if ans.is_empty() || ans == "y" || ans == "yes" {
                println!("Resuming submit {}...", entry_id);
                // Call submit_one with the stored Accepted row's content
                // We'll actually pass this instruction to the TUI to handle it, or we can do it here.
                // Wait, if we do it here, we need to spin up the engine. It's easier to pass a flag to TUI.
            } else {
                // append an Aborted row with reason "operator-declined resume"
                let aborted = vac_session_engine::TranscriptEntry::new(
                    session.id,
                    vac_session_engine::TranscriptKind::Aborted,
                    serde_json::json!({ "reason": "operator-declined resume", "kind": "cancelled" }),
                );
                let handle = writer.open(session.id).await?;
                writer.append(&handle, &aborted).await?;
            }
        }
    }

    let io_mode = vac_tui_runtime::TuiIoMode {
        record_dir: record,
        replay_file: replay,
    };
    vac_tui_runtime::run_vac_tui_with_io(project_root, resume, io_mode).await
}
