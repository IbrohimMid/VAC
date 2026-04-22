//! Session task helpers — resume, switch, snapshot, cleanup.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use vac_core::RuntimeUpdate;
use vac_core::engine::VacEngine;

use super::{ActiveUpdateTx, backend::handle_runtime_update};
use crate::{InputEvent, ToolCall};

/// Handle `OutputEvent::ListSessions` — load all sessions with checkpoint/snapshot metadata.
pub(super) async fn handle_list_sessions(
    engine: &Arc<Mutex<VacEngine>>,
    project_root: &Path,
    input_tx: &mpsc::Sender<InputEvent>,
) {
    let eng = engine.lock().await;
    if let Ok(sessions) = eng.list_sessions().await {
        let project_root = project_root.to_path_buf();
        let input_tx = input_tx.clone();
        tokio::spawn(async move {
            let snapshots = vac_session_control::list_snapshots_async(project_root.clone())
                .await
                .unwrap_or_default();
            let snapshot_by_id: std::collections::HashMap<_, _> = snapshots
                .into_iter()
                .map(|snapshot| (snapshot.session_id, snapshot))
                .collect();
            let mut session_infos = Vec::with_capacity(sessions.len());
            for s in sessions {
                let id_str = s.id.to_string();
                let checkpoint_dir = project_root.join(".vac/checkpoints");
                let has_checkpoint =
                    vac_session_control::has_checkpoint_async(project_root.clone(), s.id).await;
                let checkpoints: Vec<String> = if checkpoint_dir.exists() {
                    let mut files: Vec<_> = std::fs::read_dir(checkpoint_dir)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|e| e.file_name().to_string_lossy().starts_with(&id_str))
                        .filter_map(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            let modified = e.metadata().ok()?.modified().ok()?;
                            Some((modified, name))
                        })
                        .collect();
                    files.sort_by(|a, b| b.0.cmp(&a.0));
                    files.into_iter().map(|(_, name)| name).take(5).collect()
                } else {
                    vec![]
                };
                let last_activity = s.updated_at.format("%Y-%m-%d %H:%M").to_string();
                let snapshot_present = snapshot_by_id.contains_key(&s.id);
                let snapshot_stale = snapshot_by_id.get(&s.id).is_some_and(|snapshot| {
                    vac_session_control::is_stale(snapshot, chrono::Duration::days(30))
                });
                session_infos.push(crate::app::SessionInfo {
                    id: id_str.clone(),
                    title: format!("Session {}", &id_str[..8]),
                    updated_at: s.updated_at.to_rfc3339(),
                    checkpoints,
                    task_count: s.tasks.len(),
                    last_activity,
                    has_checkpoint,
                    snapshot_present,
                    snapshot_stale,
                });
            }
            let _ = input_tx.send(InputEvent::SetSessions(session_infos)).await;
        });
    } else {
        let _ = input_tx.send(InputEvent::SetSessions(vec![])).await;
    }
}

/// Handle `OutputEvent::LoadSessionResumeList` — load snapshots for the resume picker.
pub(super) async fn handle_load_session_resume_list(
    project_root: &Path,
    input_tx: &mpsc::Sender<InputEvent>,
) {
    let root = project_root.to_path_buf();
    let tx = input_tx.clone();
    tokio::spawn(async move {
        let snapshots = vac_session_control::list_snapshots_async(root.clone())
            .await
            .unwrap_or_default();
        let project_name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();
        let entries: Vec<crate::app::types::SessionResumeEntry> = snapshots
            .into_iter()
            .map(|snap| {
                let id_str = snap.session_id.to_string();
                crate::app::types::SessionResumeEntry {
                    session_id: snap.session_id,
                    title: format!("Session {}", &id_str[..8]),
                    project: project_name.clone(),
                    last_message_preview: format!(
                        "{} tasks, {} tokens",
                        snap.task_count, snap.total_tokens
                    ),
                    last_active: snap.updated_at,
                    model: snap.active_model.clone(),
                    token_count: Some(snap.total_tokens as u32),
                }
            })
            .collect();
        let _ = tx.send(InputEvent::SetSessionResumeList(entries)).await;
    });
}

/// Handle `OutputEvent::NewSession`.
pub(super) async fn handle_new_session(
    engine: &Arc<Mutex<VacEngine>>,
    input_tx: &mpsc::Sender<InputEvent>,
) {
    let eng = engine.lock().await;
    if let Ok(status) = eng.status().await {
        let mut current_session = eng.session().write().await;
        *current_session = vac_core::session::Session::new(status.project_root);
        let _ = input_tx
            .send(InputEvent::SessionRestored {
                id: current_session.id.to_string(),
                title: "New Session".to_string(),
                messages: vec![],
            })
            .await;
    }
}

/// Handle `OutputEvent::CleanupSession` — async cleanup.
pub(super) async fn handle_cleanup_session(
    project_root: &Path,
    input_tx: &mpsc::Sender<InputEvent>,
    session_id_str: String,
) {
    let session_id = match uuid::Uuid::parse_str(&session_id_str) {
        Ok(u) => u,
        Err(e) => {
            let _ = input_tx
                .send(InputEvent::ShowToast(crate::services::Toast::error(
                    format!("Invalid session id: {e}"),
                )))
                .await;
            return;
        }
    };

    let report = vac_session_control::cleanup_session_async(project_root.to_path_buf(), session_id)
        .await
        .unwrap_or(vac_session_control::CleanupReport {
            snapshot_removed: false,
            checkpoint_removed: false,
            approvals_removed: 0,
            errors: vec![],
        });

    if report.snapshot_removed || report.checkpoint_removed || report.approvals_removed > 0 {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::success(
                format!(
                    "Cleaned session artifacts: snapshot {}, checkpoint {}, approvals {}",
                    report.snapshot_removed, report.checkpoint_removed, report.approvals_removed
                ),
            )))
            .await;
    } else {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::info(
                "No session artifacts found to clean".to_string(),
            )))
            .await;
    }
    if !report.errors.is_empty() {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::error(
                format!(
                    "Session cleanup completed with {} error(s)",
                    report.errors.len()
                ),
            )))
            .await;
    }
    // Refresh session list after cleanup
    let _ = input_tx
        .send(InputEvent::ShowToast(crate::services::Toast::info(
            "Refreshing session list...".to_string(),
        )))
        .await;
}

pub(super) async fn resume_session_into_tui(
    engine: Arc<Mutex<VacEngine>>,
    active_update_tx: ActiveUpdateTx,
    input_tx: mpsc::Sender<InputEvent>,
    id: String,
) {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(e) => {
            let _ = input_tx
                .send(InputEvent::Error(format!("Invalid session id: {e}")))
                .await;
            return;
        }
    };

    let project_root = {
        let eng = engine.lock().await;
        match eng.status().await {
            Ok(status) => status.project_root,
            Err(e) => {
                let _ = input_tx
                    .send(InputEvent::Error(format!(
                        "Failed to read engine status: {e}"
                    )))
                    .await;
                return;
            }
        }
    };

    let title = match vac_core::session::Session::load(&project_root, uuid) {
        Ok(Some(s)) => format!("Session {}", &s.id.to_string()[..8]),
        _ => format!("Session {}", &id[..id.len().min(8)]),
    };

    let _ = input_tx
        .send(InputEvent::SessionRestored {
            id: id.clone(),
            title,
            messages: vec![],
        })
        .await;

    tokio::spawn(async move {
        let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel::<RuntimeUpdate>();
        let input_tx_inner = input_tx.clone();
        let stream_uuid = uuid::Uuid::new_v4();

        *active_update_tx.lock().await = Some(update_tx.clone());

        tokio::spawn(async move {
            let mut active_tools: HashMap<String, ToolCall> = HashMap::new();
            while let Some(update) = update_rx.recv().await {
                handle_runtime_update(update, &input_tx_inner, stream_uuid, &mut active_tools)
                    .await;
            }
        });

        let result = {
            let mut eng = engine.lock().await;
            eng.resume_run_state(uuid, Some(update_tx), None, None)
                .await
        };

        *active_update_tx.lock().await = None;

        if let Err(e) = result {
            let _ = input_tx
                .send(InputEvent::Error(format!("Failed to resume session: {e}")))
                .await;
        }
    });
}
