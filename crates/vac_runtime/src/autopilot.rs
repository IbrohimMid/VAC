use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;

use crate::executor::{OperatingMode, TaskExecutor};
use crate::jobs::{Job, JobKind, JobStatus};
use crate::queue::TaskQueue;
use crate::scheduler::{AutopilotEvent, AutopilotState, AutopilotStateFile};

pub struct AutopilotController {
    project_root: PathBuf,
    queue: Arc<TaskQueue>,
    executor: TaskExecutor,
    config: vac_core::config::AutopilotConfig,
    state_path: PathBuf,
    approvals_dir: PathBuf,
    engine: Option<Arc<Mutex<vac_core::VacEngine>>>,
}

impl AutopilotController {
    pub async fn new(project_root: PathBuf) -> anyhow::Result<Self> {
        let config = vac_core::config::AutopilotConfig::load(&project_root)?;
        let vac_config = vac_core::VacConfig::load_with_fallback(&project_root)?;
        let operating_mode = OperatingMode::from_str(&vac_config.runtime.operating_mode);
        let executor = TaskExecutor::new(project_root.clone(), operating_mode);
        let queue = Arc::new(TaskQueue::with_storage(project_root.join(".vac/queue.json")));
        Ok(Self {
            state_path: project_root.join(".vac/autopilot.state"),
            approvals_dir: project_root.join(".vac/autopilot.approvals"),
            project_root,
            queue,
            executor,
            config,
            engine: None,
        })
    }

    pub async fn run(mut self, mut shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
        let poll_interval = std::time::Duration::from_secs(self.config.poll_interval_secs.max(1));

        loop {
            if *shutdown.borrow() {
                break;
            }

            if self.config.mode == "monitor" {
                let queued = self.queued_len().await;
                self.write_state(AutopilotStateFile {
                    state: AutopilotState::Polling,
                    mode: self.config.mode.clone(),
                    poll_interval_secs: self.config.poll_interval_secs,
                    queue_len: queued,
                    current_job: None,
                    last_event: if queued > 0 { Some(AutopilotEvent::TaskQueued) } else { None },
                    last_error: None,
                    updated_at: Utc::now(),
                });
                tokio::select! {
                    _ = tokio::time::sleep(poll_interval) => {}
                    _ = shutdown.changed() => {}
                }
                continue;
            }

            if let Some(mut job) = self.queue.dequeue().await {
                let queued = self.queued_len().await;
                self.write_state(AutopilotStateFile {
                    state: AutopilotState::Executing { job_id: job.id, kind: job.kind_name() },
                    mode: self.config.mode.clone(),
                    poll_interval_secs: self.config.poll_interval_secs,
                    queue_len: queued,
                    current_job: Some(job.id),
                    last_event: Some(AutopilotEvent::TaskStarted),
                    last_error: None,
                    updated_at: Utc::now(),
                });

                let result = match job.kind.clone() {
                    JobKind::ManualApproval { tool_name } => {
                        self.handle_manual_approval(&mut job, &tool_name, &mut shutdown).await
                    }
                    _ => self.execute_job(&job).await,
                };

                match result {
                    Ok(summary) => {
                        job.status = JobStatus::Completed;
                        job.result_summary = Some(summary);
                        job.completed_at = Some(Utc::now());
                        self.queue.update_job(job).await;
                        self.write_state(AutopilotStateFile {
                            state: AutopilotState::Idle,
                            mode: self.config.mode.clone(),
                            poll_interval_secs: self.config.poll_interval_secs,
                            queue_len: self.queued_len().await,
                            current_job: None,
                            last_event: Some(AutopilotEvent::TaskCompleted),
                            last_error: None,
                            updated_at: Utc::now(),
                        });
                    }
                    Err(e) => {
                        let error = e.to_string();
                        job.status = JobStatus::Failed(error.clone());
                        job.completed_at = Some(Utc::now());
                        self.queue.update_job(job).await;

                        let until = Utc::now()
                            + chrono::Duration::from_std(poll_interval)
                                .unwrap_or_else(|_| chrono::Duration::seconds(30));
                        self.write_state(AutopilotStateFile {
                            state: AutopilotState::Backoff { until },
                            mode: self.config.mode.clone(),
                            poll_interval_secs: self.config.poll_interval_secs,
                            queue_len: self.queued_len().await,
                            current_job: None,
                            last_event: Some(AutopilotEvent::TaskFailed),
                            last_error: Some(error),
                            updated_at: Utc::now(),
                        });

                        tokio::select! {
                            _ = tokio::time::sleep(poll_interval) => {}
                            _ = shutdown.changed() => {}
                        }
                    }
                }
            } else {
                self.write_state(AutopilotStateFile {
                    state: AutopilotState::Polling,
                    mode: self.config.mode.clone(),
                    poll_interval_secs: self.config.poll_interval_secs,
                    queue_len: self.queued_len().await,
                    current_job: None,
                    last_event: None,
                    last_error: None,
                    updated_at: Utc::now(),
                });
                tokio::select! {
                    _ = tokio::time::sleep(poll_interval) => {}
                    _ = shutdown.changed() => {}
                }
            }
        }

        Ok(())
    }

    async fn queued_len(&self) -> usize {
        self.queue
            .list()
            .await
            .into_iter()
            .filter(|j| j.status == JobStatus::Queued)
            .count()
    }

    async fn execute_job(&mut self, job: &Job) -> anyhow::Result<String> {
        if matches!(job.kind, JobKind::RunTask { .. } | JobKind::PatchProposal { .. })
            && matches!(self.executor.operating_mode, OperatingMode::PatchProposal | OperatingMode::AutoFixLowRisk)
            && self.engine.is_none()
        {
            let mut engine = vac_core::VacEngine::new(self.project_root.clone()).await?;
            engine.init().await?;
            let engine = Arc::new(Mutex::new(engine));
            self.executor.attach_engine(engine.clone());
            self.engine = Some(engine);
        }

        self.executor.execute(job).await
    }

    async fn handle_manual_approval(
        &mut self,
        job: &mut Job,
        tool_name: &str,
        shutdown: &mut tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<String> {
        let tool_call_id = job.id.to_string();
        let approval_path = self.approvals_dir.join(format!("{tool_call_id}.json"));

        self.write_state(AutopilotStateFile {
            state: AutopilotState::WaitingApproval {
                tool_call_id: tool_call_id.clone(),
            },
            mode: self.config.mode.clone(),
            poll_interval_secs: self.config.poll_interval_secs,
            queue_len: self.queued_len().await,
            current_job: Some(job.id),
            last_event: Some(AutopilotEvent::ApprovalNeeded),
            last_error: None,
            updated_at: Utc::now(),
        });

        if let Some(parent) = approval_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        loop {
            if *shutdown.borrow() {
                anyhow::bail!("Shutdown while waiting approval");
            }

            if approval_path.exists() {
                let content = std::fs::read_to_string(&approval_path)?;
                let v: serde_json::Value = serde_json::from_str(&content)?;
                let approved = v.get("approved").and_then(|b| b.as_bool()).unwrap_or(false);
                let reason = v.get("reason").and_then(|s| s.as_str()).map(|s| s.to_string());
                let _ = std::fs::remove_file(&approval_path);

                if approved {
                    job.status = JobStatus::Completed;
                    return Ok(format!("Manual approval granted for {tool_name} ({})", reason.unwrap_or_default()));
                }

                anyhow::bail!(format!(
                    "Manual approval denied for {tool_name} ({})",
                    reason.unwrap_or_else(|| "no reason".to_string())
                ));
            }

            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
                _ = shutdown.changed() => {}
            }
        }
    }

    fn write_state(&self, state: AutopilotStateFile) {
        if let Some(parent) = self.state_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write(&self.state_path, json);
        }
    }
}

pub fn approval_file_path(project_root: &Path, tool_call_id: &str) -> PathBuf {
    project_root
        .join(".vac/autopilot.approvals")
        .join(format!("{tool_call_id}.json"))
}
