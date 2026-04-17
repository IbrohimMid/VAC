use async_trait::async_trait;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::jobs::{Job, JobStatus};
use crate::runtime_queue::RuntimeQueue;

pub struct TaskQueue {
    jobs: Arc<RwLock<VecDeque<Job>>>,
    storage_path: Option<PathBuf>,
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskQueue {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(VecDeque::new())),
            storage_path: None,
        }
    }

    pub fn with_storage(path: PathBuf) -> Self {
        let mut loaded = VecDeque::new();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(jobs) = serde_json::from_str::<Vec<Job>>(&content) {
                    for job in jobs {
                        loaded.push_back(job);
                    }
                }
            }
        }

        Self {
            jobs: Arc::new(RwLock::new(loaded)),
            storage_path: Some(path),
        }
    }

    async fn persist(&self) {
        if let Some(path) = &self.storage_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let jobs = self.jobs.read().await;
            let vec: Vec<Job> = jobs.iter().cloned().collect();
            if let Ok(json) = serde_json::to_string_pretty(&vec) {
                let tmp_path = path.with_file_name(format!(
                    "{}.tmp",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("queue.json")
                ));
                if std::fs::write(&tmp_path, json).is_ok() {
                    let _ = std::fs::rename(&tmp_path, path);
                }
            }
        }
    }

    pub async fn enqueue(&self, job: Job) {
        self.jobs.write().await.push_back(job);
        self.persist().await;
    }

    /// Dequeue the next Queued job, marking it as Running
    pub async fn dequeue(&self) -> Option<Job> {
        let mut jobs = self.jobs.write().await;
        for job in jobs.iter_mut() {
            if job.status == JobStatus::Queued {
                job.status = JobStatus::Running;
                job.started_at = Some(chrono::Utc::now());
                let cloned = job.clone();
                // We drop the lock here because we don't want to hold it while persisting
                drop(jobs);
                self.persist().await;
                return Some(cloned);
            }
        }
        None
    }

    /// Update an existing job (used by scheduler to report completion/failure)
    pub async fn update_job(&self, updated: Job) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == updated.id) {
            *job = updated;
        }
        drop(jobs);
        self.persist().await;
    }

    pub async fn list(&self) -> Vec<Job> {
        self.jobs.read().await.iter().cloned().collect()
    }

    pub async fn get(&self, id: Uuid) -> Option<Job> {
        self.jobs.read().await.iter().find(|j| j.id == id).cloned()
    }

    pub async fn cancel(&self, id: Uuid) -> bool {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            if job.status == JobStatus::Queued || job.status == JobStatus::Running {
                job.status = JobStatus::Cancelled;
                drop(jobs);
                self.persist().await;
                return true;
            }
        }
        false
    }

    pub async fn retry(&self, id: Uuid) -> bool {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            if matches!(job.status, JobStatus::Failed(_) | JobStatus::Cancelled) {
                job.status = JobStatus::Queued;
                job.retry_count += 1;
                job.started_at = None;
                job.completed_at = None;
                job.result_summary = None;
                drop(jobs);
                self.persist().await;
                return true;
            }
        }
        false
    }

    pub async fn len(&self) -> usize {
        self.jobs.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.jobs.read().await.is_empty()
    }
}

#[async_trait]
impl RuntimeQueue for TaskQueue {
    type Item = Job;

    async fn list(&self) -> Vec<Self::Item> {
        TaskQueue::list(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{Job, JobKind};

    #[tokio::test]
    async fn with_storage_persists_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("queue.json");

        let queue = TaskQueue::with_storage(path.clone());
        let job = Job::new(JobKind::DiagnosticSweep);
        let id = job.id;
        queue.enqueue(job).await;

        let queue2 = TaskQueue::with_storage(path);
        let jobs = queue2.list().await;
        assert!(jobs.iter().any(|j| j.id == id));
    }
}
