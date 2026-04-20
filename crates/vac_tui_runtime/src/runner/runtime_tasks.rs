//! Runtime task helpers — queue polling, state loading.

use std::path::Path;

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
