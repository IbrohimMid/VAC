use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

use crate::executor::TaskExecutor;
use crate::jobs::JobStatus;
use crate::queue::TaskQueue;

pub struct Scheduler {
    queue: Arc<TaskQueue>,
    executor: Arc<TaskExecutor>,
    running: Arc<AtomicBool>,
}

impl Scheduler {
    pub fn new(queue: Arc<TaskQueue>, executor: Arc<TaskExecutor>) -> Self {
        Self {
            queue,
            executor,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the background scheduler loop.
    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return; // already running
        }
        let queue = self.queue.clone();
        let executor = self.executor.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            info!("VAC runtime scheduler started");
            while running.load(Ordering::SeqCst) {
                if let Some(mut job) = queue.dequeue().await {
                    job.status = JobStatus::Running;
                    job.started_at = Some(chrono::Utc::now());
                    info!(job_id = %job.id, kind = ?job.kind, "Executing job");

                    match executor.execute(&job).await {
                        Ok(summary) => {
                            job.status = JobStatus::Completed;
                            job.result_summary = Some(summary);
                            job.completed_at = Some(chrono::Utc::now());
                            info!(job_id = %job.id, "Job completed");
                        }
                        Err(e) => {
                            job.status = JobStatus::Failed(e.to_string());
                            job.completed_at = Some(chrono::Utc::now());
                            warn!(job_id = %job.id, error = %e, "Job failed");
                        }
                    }
                } else {
                    // No jobs — sleep briefly
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
            info!("VAC runtime scheduler stopped");
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
