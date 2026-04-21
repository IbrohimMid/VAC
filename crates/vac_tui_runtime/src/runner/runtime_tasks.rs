//! Runtime task helpers — queue polling, state loading, cancel/retry.

use std::path::Path;
use crate::InputEvent;
use tokio::sync::mpsc;

pub(super) async fn load_runtime_queue_items<Q>(queue: &Q) -> Vec<Q::Item>
where
    Q: vac_runtime::RuntimeQueue,
{
    queue.list().await
}

pub(super) async fn load_runtime_jobs(project_root: &Path) -> Vec<vac_runtime::Job> {
    let queue = vac_runtime::TaskQueue::with_storage(project_root.join(".vac/queue.json"));
    load_runtime_queue_items(&queue).await
}

pub(super) async fn load_runtime_state(
    project_root: &Path,
) -> Option<vac_runtime::AutopilotStateFile> {
    let path = project_root.join(".vac/autopilot.state");
    tokio::task::spawn_blocking(move || {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    })
    .await
    .unwrap_or(None)
}

pub(super) async fn load_agent_tasks(project_root: &Path) -> Vec<vac_runtime::AgentTask> {
    let queue =
        vac_runtime::AgentTaskQueue::with_storage(project_root.join(".vac/agent_queue.json"));
    load_runtime_queue_items(&queue).await
}

pub(super) async fn load_agent_state(
    project_root: &Path,
) -> Option<vac_runtime::AgentSchedulerStateFile> {
    let path = project_root.join(".vac/agent_scheduler.state");
    tokio::task::spawn_blocking(move || {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    })
    .await
    .unwrap_or(None)
}

/// Handle `OutputEvent::CancelRuntimeJob`.
pub(super) async fn handle_cancel_runtime_job(
    project_root: &Path,
    input_tx: &mpsc::Sender<InputEvent>,
    id: uuid::Uuid,
) {
    let queue = vac_runtime::TaskQueue::with_storage(project_root.join(".vac/queue.json"));
    let cancelled = queue.cancel(id).await;
    let toast = if cancelled {
        crate::services::Toast::success(format!("Cancelled job {}", id))
    } else {
        crate::services::Toast::error(format!("Failed to cancel job {}", id))
    };
    let jobs = load_runtime_jobs(project_root).await;
    let snapshot = load_runtime_state(project_root).await;
    let _ = input_tx.send(InputEvent::SetRuntimeJobs(jobs)).await;
    let _ = input_tx.send(InputEvent::SetRuntimeState(snapshot)).await;
    let _ = input_tx.send(InputEvent::ShowToast(toast)).await;
}

/// Handle `OutputEvent::RetryRuntimeJob`.
pub(super) async fn handle_retry_runtime_job(
    project_root: &Path,
    input_tx: &mpsc::Sender<InputEvent>,
    id: uuid::Uuid,
) {
    let queue = vac_runtime::TaskQueue::with_storage(project_root.join(".vac/queue.json"));
    let retried = queue.retry(id).await;
    let toast = if retried {
        crate::services::Toast::success(format!("Retried job {}", id))
    } else {
        crate::services::Toast::error(format!("Failed to retry job {}", id))
    };
    let jobs = load_runtime_jobs(project_root).await;
    let snapshot = load_runtime_state(project_root).await;
    let _ = input_tx.send(InputEvent::SetRuntimeJobs(jobs)).await;
    let _ = input_tx.send(InputEvent::SetRuntimeState(snapshot)).await;
    let _ = input_tx.send(InputEvent::ShowToast(toast)).await;
}
