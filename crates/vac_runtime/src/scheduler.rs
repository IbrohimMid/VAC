use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

use crate::executor::TaskExecutor;
use crate::jobs::JobStatus;
use crate::queue::TaskQueue;
use vac_core::config::ExecutionEnvironment;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AutopilotState {
    Idle,
    Polling,
    Executing { job_id: uuid::Uuid, kind: String },
    WaitingApproval { tool_call_id: String },
    Backoff { until: DateTime<Utc> },
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutopilotEvent {
    TaskQueued,
    TaskStarted,
    ApprovalNeeded,
    TaskCompleted,
    TaskFailed,
    RetryScheduled,
    StateChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotStateFile {
    #[serde(flatten)]
    pub state: AutopilotState,
    pub mode: String,
    #[serde(default = "default_task_intent_mode")]
    pub task_intent_mode: String,
    #[serde(default = "default_environment_mode")]
    pub environment_mode: String,
    #[serde(default)]
    pub execution_environment: ExecutionEnvironment,
    pub poll_interval_secs: u64,
    pub queue_len: usize,
    pub current_job: Option<uuid::Uuid>,
    pub last_event: Option<AutopilotEvent>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

fn default_task_intent_mode() -> String {
    "monitor-only".to_string()
}

fn default_environment_mode() -> String {
    "host".to_string()
}

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub mode: String,
    pub poll_interval_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            mode: "monitor".to_string(),
            poll_interval_secs: 30,
        }
    }
}

pub struct Scheduler {
    queue: Arc<TaskQueue>,
    executor: Arc<TaskExecutor>,
    running: Arc<AtomicBool>,
    state_file: PathBuf,
    config: SchedulerConfig,
}

impl Scheduler {
    pub fn new(
        queue: Arc<TaskQueue>,
        executor: Arc<TaskExecutor>,
        config: SchedulerConfig,
    ) -> Self {
        let state_file = executor.project_root.join(".vac/autopilot.state");
        Self {
            queue,
            executor,
            running: Arc::new(AtomicBool::new(false)),
            state_file,
            config,
        }
    }

    fn write_state(&self, state: &AutopilotStateFile) {
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
        let config = self.config.clone();

        self.write_state(&AutopilotStateFile {
            state: AutopilotState::Idle,
            mode: config.mode.clone(),
            task_intent_mode: executor.task_intent_mode.as_str().to_string(),
            environment_mode: executor.environment_mode.as_str().to_string(),
            execution_environment: executor.execution_environment,
            poll_interval_secs: config.poll_interval_secs,
            queue_len: 0,
            current_job: None,
            last_event: Some(AutopilotEvent::StateChanged),
            last_error: None,
            updated_at: Utc::now(),
        });

        tokio::spawn(async move {
            info!("VAC runtime scheduler started");
            let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs.max(1));
            let update_state = |state: &AutopilotStateFile| {
                if let Ok(json) = serde_json::to_string_pretty(state) {
                    let _ = std::fs::write(&state_file, json);
                }
            };

            while running.load(Ordering::SeqCst) {
                let queue_len = queue.len().await;

                if config.mode == "monitor" {
                    update_state(&AutopilotStateFile {
                        state: AutopilotState::Polling,
                        mode: config.mode.clone(),
                        task_intent_mode: executor.task_intent_mode.as_str().to_string(),
                        environment_mode: executor.environment_mode.as_str().to_string(),
                        execution_environment: executor.execution_environment,
                        poll_interval_secs: config.poll_interval_secs,
                        queue_len,
                        current_job: None,
                        last_event: if queue_len > 0 {
                            Some(AutopilotEvent::TaskQueued)
                        } else {
                            None
                        },
                        last_error: None,
                        updated_at: Utc::now(),
                    });
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }

                if let Some(mut job) = queue.dequeue().await {
                    info!(job_id = %job.id, kind = ?job.kind, "Executing job");
                    update_state(&AutopilotStateFile {
                        state: AutopilotState::Executing {
                            job_id: job.id,
                            kind: job.kind_name(),
                        },
                        mode: config.mode.clone(),
                        task_intent_mode: executor.task_intent_mode.as_str().to_string(),
                        environment_mode: executor.environment_mode.as_str().to_string(),
                        execution_environment: executor.execution_environment,
                        poll_interval_secs: config.poll_interval_secs,
                        queue_len,
                        current_job: Some(job.id),
                        last_event: Some(AutopilotEvent::TaskStarted),
                        last_error: None,
                        updated_at: Utc::now(),
                    });

                    match executor.execute(&job).await {
                        Ok(summary) => {
                            job.status = JobStatus::Completed;
                            job.result_summary = Some(summary);
                            job.completed_at = Some(chrono::Utc::now());
                            info!(job_id = %job.id, "Job completed");
                            queue.update_job(job).await;
                            update_state(&AutopilotStateFile {
                                state: AutopilotState::Idle,
                                mode: config.mode.clone(),
                                task_intent_mode: executor.task_intent_mode.as_str().to_string(),
                                environment_mode: executor.environment_mode.as_str().to_string(),
                                execution_environment: executor.execution_environment,
                                poll_interval_secs: config.poll_interval_secs,
                                queue_len: queue.len().await,
                                current_job: None,
                                last_event: Some(AutopilotEvent::TaskCompleted),
                                last_error: None,
                                updated_at: Utc::now(),
                            });
                        }
                        Err(e) => {
                            let error = e.to_string();
                            if job.retry_count < job.max_retries {
                                job.retry_count += 1;
                                job.status = JobStatus::Queued;
                                queue.update_job(job.clone()).await;

                                let until = Utc::now()
                                    + chrono::Duration::from_std(poll_interval)
                                        .unwrap_or_else(|_| chrono::Duration::seconds(5));
                                update_state(&AutopilotStateFile {
                                    state: AutopilotState::Backoff { until },
                                    mode: config.mode.clone(),
                                    task_intent_mode: executor
                                        .task_intent_mode
                                        .as_str()
                                        .to_string(),
                                    environment_mode: executor
                                        .environment_mode
                                        .as_str()
                                        .to_string(),
                                    execution_environment: executor.execution_environment,
                                    poll_interval_secs: config.poll_interval_secs,
                                    queue_len: queue.len().await,
                                    current_job: None,
                                    last_event: Some(AutopilotEvent::RetryScheduled),
                                    last_error: Some(format!(
                                        "Retry {}/{}: {}",
                                        job.retry_count, job.max_retries, error
                                    )),
                                    updated_at: Utc::now(),
                                });
                                tokio::time::sleep(poll_interval).await;
                            } else {
                                job.status = JobStatus::Failed(error.clone());
                                job.completed_at = Some(chrono::Utc::now());
                                warn!(job_id = %job.id, error = %e, "Job failed");
                                queue.update_job(job).await;

                                update_state(&AutopilotStateFile {
                                    state: AutopilotState::Failed {
                                        error: error.clone(),
                                    },
                                    mode: config.mode.clone(),
                                    task_intent_mode: executor
                                        .task_intent_mode
                                        .as_str()
                                        .to_string(),
                                    environment_mode: executor
                                        .environment_mode
                                        .as_str()
                                        .to_string(),
                                    execution_environment: executor.execution_environment,
                                    poll_interval_secs: config.poll_interval_secs,
                                    queue_len: queue.len().await,
                                    current_job: None,
                                    last_event: Some(AutopilotEvent::TaskFailed),
                                    last_error: Some(error),
                                    updated_at: Utc::now(),
                                });
                                tokio::time::sleep(poll_interval).await;
                            }
                        }
                    }
                } else {
                    update_state(&AutopilotStateFile {
                        state: AutopilotState::Polling,
                        mode: config.mode.clone(),
                        task_intent_mode: executor.task_intent_mode.as_str().to_string(),
                        environment_mode: executor.environment_mode.as_str().to_string(),
                        execution_environment: executor.execution_environment,
                        poll_interval_secs: config.poll_interval_secs,
                        queue_len,
                        current_job: None,
                        last_event: None,
                        last_error: None,
                        updated_at: Utc::now(),
                    });
                    tokio::time::sleep(poll_interval).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::OperatingMode;
    use crate::jobs::{Job, JobKind, JobStatus};

    #[test]
    fn state_file_serializes_required_fields() {
        let sf = AutopilotStateFile {
            state: AutopilotState::Polling,
            mode: "monitor".to_string(),
            task_intent_mode: "monitor-only".to_string(),
            environment_mode: "host".to_string(),
            execution_environment: ExecutionEnvironment::Host,
            poll_interval_secs: 5,
            queue_len: 2,
            current_job: None,
            last_event: Some(AutopilotEvent::TaskQueued),
            last_error: None,
            updated_at: Utc::now(),
        };
        let v = serde_json::to_value(&sf).unwrap();
        assert_eq!(v.get("state").and_then(|s| s.as_str()), Some("polling"));
        assert_eq!(v.get("mode").and_then(|s| s.as_str()), Some("monitor"));
        assert_eq!(
            v.get("task_intent_mode").and_then(|s| s.as_str()),
            Some("monitor-only")
        );
        assert_eq!(
            v.get("environment_mode").and_then(|s| s.as_str()),
            Some("host")
        );
        assert_eq!(
            v.get("poll_interval_secs").and_then(|s| s.as_u64()),
            Some(5)
        );
        assert_eq!(v.get("queue_len").and_then(|s| s.as_u64()), Some(2));
        assert!(v.get("updated_at").is_some());
        assert!(v.get("last_event").is_some());
    }

    #[tokio::test]
    async fn monitor_mode_does_not_dequeue_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let queue = Arc::new(TaskQueue::new());
        let job = Job::new(JobKind::DiagnosticSweep);
        let id = job.id;
        queue.enqueue(job).await;

        let executor = Arc::new(TaskExecutor::new(root.clone(), OperatingMode::MonitorOnly));
        let scheduler = Scheduler::new(
            queue.clone(),
            executor,
            SchedulerConfig {
                mode: "monitor".to_string(),
                poll_interval_secs: 1,
            },
        );
        scheduler.start();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        scheduler.stop();

        let jobs = queue.list().await;
        let j = jobs.iter().find(|j| j.id == id).unwrap();
        assert_eq!(j.status, JobStatus::Queued);

        let content = std::fs::read_to_string(root.join(".vac/autopilot.state")).unwrap();
        let sf: AutopilotStateFile = serde_json::from_str(&content).unwrap();
        assert_eq!(sf.mode, "monitor");
        assert_eq!(sf.poll_interval_secs, 1);
        assert!(matches!(
            sf.state,
            AutopilotState::Polling | AutopilotState::Idle
        ));
    }
}
