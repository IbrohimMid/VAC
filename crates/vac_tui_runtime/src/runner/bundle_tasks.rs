//! Bundle task helpers — ExportBundle, ImportBundle output events.

use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::InputEvent;

/// Handle `OutputEvent::ExportBundle` — export the current session bundle to
/// `path` and surface a success toast + assistant message (or an error toast).
/// Spawns internally.
pub(super) fn handle_export_bundle(
    project_root: PathBuf,
    input_tx: mpsc::Sender<InputEvent>,
    path: PathBuf,
) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            vac_core::bundle::export_bundle_to_path(&project_root, None, Some(&path), true)
        })
        .await
        .unwrap_or_else(|e| {
            Err(vac_core::VacError::Task(format!("join error: {e}")))
        });
        match result {
            Ok(out_path) => {
                let _ = input_tx
                    .send(InputEvent::ShowToast(crate::services::Toast::success(
                        format!("Bundle diekspor: {}", out_path.display()),
                    )))
                    .await;
                let _ = input_tx
                    .send(InputEvent::AssistantMessage(format!(
                        "Bundle diekspor ke `{}`",
                        out_path.display()
                    )))
                    .await;
            }
            Err(e) => {
                let _ = input_tx
                    .send(InputEvent::ShowToast(crate::services::Toast::error(
                        format!("Gagal export bundle: {e}"),
                    )))
                    .await;
            }
        }
    });
}

/// Handle `OutputEvent::ImportBundle` — import a session bundle from `path` and
/// surface a success toast + assistant message (or an error toast). Spawns
/// internally.
pub(super) fn handle_import_bundle(
    project_root: PathBuf,
    input_tx: mpsc::Sender<InputEvent>,
    path: PathBuf,
) {
    tokio::spawn(async move {
        let path_for_msg = path.clone();
        let result = tokio::task::spawn_blocking(move || {
            vac_core::bundle::import_bundle_from_path(&project_root, &path)
        })
        .await
        .unwrap_or_else(|e| {
            Err(vac_core::VacError::Task(format!("join error: {e}")))
        });
        match result {
            Ok(session_id) => {
                let _ = input_tx
                    .send(InputEvent::ShowToast(crate::services::Toast::success(
                        format!("Bundle diimpor (session_id={})", session_id),
                    )))
                    .await;
                let _ = input_tx
                    .send(InputEvent::AssistantMessage(format!(
                        "Bundle diimpor dari `{}`",
                        path_for_msg.display()
                    )))
                    .await;
            }
            Err(e) => {
                let _ = input_tx
                    .send(InputEvent::ShowToast(crate::services::Toast::error(
                        format!("Gagal import bundle: {e}"),
                    )))
                    .await;
            }
        }
    });
}
