use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Dev,
    Qa,
    Review,
}

impl AgentRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dev => "Dev",
            Self::Qa => "QA",
            Self::Review => "Review",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentTaskStatus {
    Queued,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: Uuid,
    pub role: AgentRole,
    pub description: String,
    pub status: AgentTaskStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub output_summary: Option<String>,
}

impl AgentTask {
    pub fn new(role: AgentRole, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            description: description.into(),
            status: AgentTaskStatus::Queued,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            output_summary: None,
        }
    }
}

pub struct AgentTaskQueue {
    tasks: Arc<RwLock<VecDeque<AgentTask>>>,
    notify: Arc<Notify>,
    storage_path: Option<PathBuf>,
}

impl Default for AgentTaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentTaskQueue {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(VecDeque::new())),
            notify: Arc::new(Notify::new()),
            storage_path: None,
        }
    }

    pub fn with_storage(path: PathBuf) -> Self {
        let mut loaded = VecDeque::new();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(tasks) = serde_json::from_str::<Vec<AgentTask>>(&content) {
                    for t in tasks {
                        loaded.push_back(t);
                    }
                }
            }
        }
        Self {
            tasks: Arc::new(RwLock::new(loaded)),
            notify: Arc::new(Notify::new()),
            storage_path: Some(path),
        }
    }

    async fn persist(&self) {
        let Some(path) = &self.storage_path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tasks = self.tasks.read().await;
        let vec: Vec<AgentTask> = tasks.iter().cloned().collect();
        if let Ok(json) = serde_json::to_string_pretty(&vec) {
            let _ = std::fs::write(path, json);
        }
    }

    pub async fn enqueue(&self, task: AgentTask) {
        self.tasks.write().await.push_back(task);
        self.persist().await;
        self.notify.notify_waiters();
    }

    pub async fn dequeue_for_role(&self, role: AgentRole) -> Option<AgentTask> {
        let mut tasks = self.tasks.write().await;
        for t in tasks.iter_mut() {
            if t.role == role && t.status == AgentTaskStatus::Queued {
                t.status = AgentTaskStatus::Running;
                t.started_at = Some(Utc::now());
                let cloned = t.clone();
                drop(tasks);
                self.persist().await;
                return Some(cloned);
            }
        }
        None
    }

    pub async fn update_task(&self, updated: AgentTask) {
        let mut tasks = self.tasks.write().await;
        if let Some(t) = tasks.iter_mut().find(|t| t.id == updated.id) {
            *t = updated;
        }
        drop(tasks);
        self.persist().await;
    }

    pub async fn list(&self) -> Vec<AgentTask> {
        self.tasks.read().await.iter().cloned().collect()
    }

    pub async fn counts(&self) -> AgentQueueCounts {
        let tasks = self.tasks.read().await;
        let mut c = AgentQueueCounts::default();
        for t in tasks.iter() {
            match &t.status {
                AgentTaskStatus::Queued => c.queued += 1,
                AgentTaskStatus::Running => c.running += 1,
                AgentTaskStatus::Completed => c.completed += 1,
                AgentTaskStatus::Failed(_) => c.failed += 1,
                AgentTaskStatus::Cancelled => c.cancelled += 1,
            }
        }
        c
    }

    fn notifier(&self) -> Arc<Notify> {
        self.notify.clone()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentQueueCounts {
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentWorkerStatus {
    Idle,
    Running { task_id: Uuid, description: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWorkerSnapshot {
    pub worker_id: String,
    pub role: AgentRole,
    pub status: AgentWorkerStatus,
    pub last_output: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSchedulerStateFile {
    pub counts: AgentQueueCounts,
    pub workers: Vec<AgentWorkerSnapshot>,
    pub updated_at: DateTime<Utc>,
}

pub type AgentTaskHandler = Arc<
    dyn Fn(AgentRole, AgentTask) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone)]
pub struct AgentSchedulerConfig {
    pub dev_workers: usize,
    pub qa_workers: usize,
    pub review_workers: usize,
}

impl Default for AgentSchedulerConfig {
    fn default() -> Self {
        Self {
            dev_workers: 1,
            qa_workers: 1,
            review_workers: 1,
        }
    }
}

pub struct AgentScheduler {
    queue: Arc<AgentTaskQueue>,
    handler: AgentTaskHandler,
    cancel: CancellationToken,
    handles: Arc<RwLock<Vec<tokio::task::JoinHandle<()>>>>,
    state_file: Option<PathBuf>,
    state: Arc<RwLock<AgentSchedulerStateFile>>,
}

impl AgentScheduler {
    pub fn new(
        queue: Arc<AgentTaskQueue>,
        config: AgentSchedulerConfig,
        handler: AgentTaskHandler,
    ) -> Self {
        let mut workers = Vec::new();
        for i in 0..config.dev_workers {
            workers.push(AgentWorkerSnapshot {
                worker_id: format!("dev-{}", i + 1),
                role: AgentRole::Dev,
                status: AgentWorkerStatus::Idle,
                last_output: None,
                updated_at: Utc::now(),
            });
        }
        for i in 0..config.qa_workers {
            workers.push(AgentWorkerSnapshot {
                worker_id: format!("qa-{}", i + 1),
                role: AgentRole::Qa,
                status: AgentWorkerStatus::Idle,
                last_output: None,
                updated_at: Utc::now(),
            });
        }
        for i in 0..config.review_workers {
            workers.push(AgentWorkerSnapshot {
                worker_id: format!("review-{}", i + 1),
                role: AgentRole::Review,
                status: AgentWorkerStatus::Idle,
                last_output: None,
                updated_at: Utc::now(),
            });
        }

        Self {
            queue,
            handler,
            cancel: CancellationToken::new(),
            handles: Arc::new(RwLock::new(Vec::new())),
            state_file: None,
            state: Arc::new(RwLock::new(AgentSchedulerStateFile {
                counts: AgentQueueCounts::default(),
                workers,
                updated_at: Utc::now(),
            })),
        }
    }

    pub fn with_state_file(mut self, path: PathBuf) -> Self {
        self.state_file = Some(path);
        self
    }

    pub async fn start(&self) {
        let notify = self.queue.notifier();
        let state = self.state.read().await;
        let mut joins = Vec::new();
        for w in &state.workers {
            let q = self.queue.clone();
            let h = self.handler.clone();
            let c = self.cancel.clone();
            let n = notify.clone();
            let st = self.state.clone();
            let sf = self.state_file.clone();
            let worker_id = w.worker_id.clone();
            let role = w.role;
            joins.push(tokio::spawn(async move {
                run_worker(worker_id, role, q, h, c, n, st, sf).await;
            }));
        }
        drop(state);
        *self.handles.write().await = joins;
        self.write_state().await;
    }

    pub async fn shutdown(&self) {
        self.cancel.cancel();
        let joins = {
            let mut h = self.handles.write().await;
            std::mem::take(&mut *h)
        };
        for j in joins {
            let _ = j.await;
        }
        self.write_state().await;
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn queue(&self) -> Arc<AgentTaskQueue> {
        self.queue.clone()
    }

    pub async fn snapshot(&self) -> AgentSchedulerStateFile {
        self.state.read().await.clone()
    }

    async fn write_state(&self) {
        let Some(path) = &self.state_file else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut s = self.state.write().await;
        s.counts = self.queue.counts().await;
        s.updated_at = Utc::now();
        if let Ok(json) = serde_json::to_string_pretty(&*s) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_worker(
    worker_id: String,
    role: AgentRole,
    queue: Arc<AgentTaskQueue>,
    handler: AgentTaskHandler,
    cancel: CancellationToken,
    notify: Arc<Notify>,
    state: Arc<RwLock<AgentSchedulerStateFile>>,
    state_file: Option<PathBuf>,
) {
    loop {
        if cancel.is_cancelled() {
            set_worker_status(
                &state,
                &state_file,
                &queue,
                &worker_id,
                AgentWorkerStatus::Idle,
                None,
            )
            .await;
            break;
        }

        let Some(task) = queue.dequeue_for_role(role).await else {
            tokio::select! {
                _ = cancel.cancelled() => {}
                _ = notify.notified() => {}
            }
            continue;
        };

        set_worker_status(
            &state,
            &state_file,
            &queue,
            &worker_id,
            AgentWorkerStatus::Running {
                task_id: task.id,
                description: task.description.clone(),
            },
            None,
        )
        .await;

        let fut = (handler)(role, task.clone());
        let res = tokio::select! {
            _ = cancel.cancelled() => Err(anyhow::anyhow!("cancelled")),
            r = fut => r,
        };

        let mut updated = task.clone();
        match res {
            Ok(out) => {
                updated.status = AgentTaskStatus::Completed;
                updated.output_summary = Some(trim_summary(out));
                updated.completed_at = Some(Utc::now());
                queue.update_task(updated.clone()).await;
                set_worker_status(
                    &state,
                    &state_file,
                    &queue,
                    &worker_id,
                    AgentWorkerStatus::Idle,
                    updated.output_summary.clone(),
                )
                .await;
            }
            Err(e) => {
                let msg = e.to_string();
                if msg == "cancelled" || cancel.is_cancelled() {
                    updated.status = AgentTaskStatus::Cancelled;
                } else {
                    updated.status = AgentTaskStatus::Failed(msg);
                }
                updated.completed_at = Some(Utc::now());
                queue.update_task(updated.clone()).await;
                set_worker_status(
                    &state,
                    &state_file,
                    &queue,
                    &worker_id,
                    AgentWorkerStatus::Idle,
                    updated.output_summary.clone(),
                )
                .await;
            }
        }
    }
}

fn trim_summary(s: String) -> String {
    let trimmed = s.trim().to_string();
    if trimmed.chars().count() <= 140 {
        return trimmed;
    }
    let mut out = trimmed.chars().take(137).collect::<String>();
    out.push_str("...");
    out
}

async fn set_worker_status(
    state: &Arc<RwLock<AgentSchedulerStateFile>>,
    state_file: &Option<PathBuf>,
    queue: &Arc<AgentTaskQueue>,
    worker_id: &str,
    status: AgentWorkerStatus,
    last_output: Option<String>,
) {
    {
        let mut s = state.write().await;
        if let Some(w) = s.workers.iter_mut().find(|w| w.worker_id == worker_id) {
            w.status = status;
            if last_output.is_some() {
                w.last_output = last_output;
            }
            w.updated_at = Utc::now();
        }
        s.counts = queue.counts().await;
        s.updated_at = Utc::now();
    }

    let Some(path) = state_file else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let s = state.read().await;
    if let Ok(json) = serde_json::to_string_pretty(&*s) {
        let _ = std::fs::write(path, json);
    }
}
