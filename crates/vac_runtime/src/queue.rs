use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::jobs::{Job, JobStatus};

pub struct TaskQueue {
    jobs: Arc<RwLock<VecDeque<Job>>>,
}

impl Default for TaskQueue {
    fn default() -> Self { Self::new() }
}

impl TaskQueue {
    pub fn new() -> Self {
        Self { jobs: Arc::new(RwLock::new(VecDeque::new())) }
    }

    pub async fn enqueue(&self, job: Job) {
        self.jobs.write().await.push_back(job);
    }

    pub async fn dequeue(&self) -> Option<Job> {
        self.jobs.write().await.pop_front()
    }

    pub async fn list(&self) -> Vec<Job> {
        self.jobs.read().await.iter().cloned().collect()
    }

    pub async fn cancel(&self, id: Uuid) -> bool {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            if job.status == JobStatus::Queued {
                job.status = JobStatus::Cancelled;
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
