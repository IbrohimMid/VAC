use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::PathBuf;
use tracing::{info, warn};
use serde::{Serialize, Deserialize};

use crate::executor::TaskExecutor;
use crate::jobs::JobStatus;
use crate::queue::TaskQueue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutopilotState {
    Idle,
    Executing { job_id: uuid::Uuid, kind: String },
}

pub struct Scheduler {
    queue: Arc<TaskQueue>,
    executor: Arc<TaskExecutor>,
    running: Arc<AtomicBool>,
    state_file: PathBuf,
}

impl Scheduler {
    pub fn new(queue: Arc<TaskQueue>, executor: Arc<TaskExecutor>) -> Self {
        let state_file = executor.project_root.join(".vac/autopilot.state");
        Self {
            queue,
            executor,
            running: Arc::new(AtomicBool::new(false)),
            state_file,
        }
    }

    fn write_state(&self, state: &AutopilotState) {
        if let Some(parent) = self.state_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(state) {
            let _ = std::fs::write(&self.state_file, json);
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
        let state_file = self.state_file.clone();

        self.write_state(&AutopilotState::Idle);

        tokio::spawn(async move {
            info!("VAC runtime scheduler started");
            let update_state = |state: &AutopilotState| {
                if let Ok(json) = serde_json::to_string_pretty(state) {
                    let _ = std::fs::write(&state_file, json);
                }
            };

            while running.load(Ordering::SeqCst) {
                if let Some(mut job) = queue.dequeue().await {
                    info!(job_id = %job.id, kind = ?job.kind, "Executing job");
                    update_state(&AutopilotState::Executing { 
                        job_id: job.id, 
                        kind: job.kind_name() 
                    });

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
                    queue.update_job(job).await;
                    update_state(&AutopilotState::Idle);
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
