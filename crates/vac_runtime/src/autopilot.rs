use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::executor::{OperatingMode, TaskExecutor};
use crate::jobs::{Job, JobKind, JobStatus};
use crate::queue::TaskQueue;
use crate::scheduler::{AutopilotEvent, AutopilotState, AutopilotStateFile};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub explanation: Option<String>,
    pub job_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

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
        let operating_mode = OperatingMode::parse(&vac_config.runtime.operating_mode);
        let executor = TaskExecutor::new(project_root.clone(), operating_mode);
        let queue = Arc::new(TaskQueue::with_storage(
            project_root.join(".vac/queue.json"),
        ));
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

    pub async fn run(
        mut self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
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
                    last_event: if queued > 0 {
                        Some(AutopilotEvent::TaskQueued)
                    } else {
                        None
                    },
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
                    state: AutopilotState::Executing {
                        job_id: job.id,
                        kind: job.kind_name(),
                    },
                    mode: self.config.mode.clone(),
                    poll_interval_secs: self.config.poll_interval_secs,
                    queue_len: queued,
                    current_job: Some(job.id),
                    last_event: Some(AutopilotEvent::TaskStarted),
                    last_error: None,
                    updated_at: Utc::now(),
                });

                let result = match job.kind.clone() {
                    JobKind::ToolCall {
                        tool_name,
                        arguments,
                    } => {
                        self.execute_tool_call_job(&job, tool_name, arguments, shutdown.clone())
                            .await
                    }
                    _ => self.execute_job(&job, shutdown.clone()).await,
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

    async fn execute_job(
        &mut self,
        job: &Job,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<String> {
        if matches!(
            job.kind,
            JobKind::RunTask { .. } | JobKind::PatchProposal { .. }
        ) && matches!(
            self.executor.operating_mode,
            OperatingMode::PatchProposal | OperatingMode::AutoFixLowRisk
        ) && self.engine.is_none()
        {
            let mut engine = vac_core::VacEngine::new(self.project_root.clone()).await?;
            engine.init().await?;
            let engine = Arc::new(Mutex::new(engine));
            self.executor.attach_engine(engine.clone());
            self.engine = Some(engine);
        }

        if matches!(
            job.kind,
            JobKind::RunTask { .. } | JobKind::PatchProposal { .. }
        ) && matches!(
            self.executor.operating_mode,
            OperatingMode::PatchProposal | OperatingMode::AutoFixLowRisk
        ) {
            return self
                .execute_engine_job_with_file_approvals(job, shutdown)
                .await;
        }

        self.executor.execute(job).await
    }

    async fn execute_engine_job_with_file_approvals(
        &mut self,
        job: &Job,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<String> {
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Autopilot controller has no engine attached"))?
            .clone();

        let (update_tx, mut update_rx) =
            tokio::sync::mpsc::unbounded_channel::<vac_core::engine::RuntimeUpdate>();
        let (approval_tx, approval_rx) =
            tokio::sync::mpsc::unbounded_channel::<vil_swarm::ApprovalResponse>();

        let queue = self.queue.clone();
        let state_path = self.state_path.clone();
        let approvals_dir = self.approvals_dir.clone();
        let mode = self.config.mode.clone();
        let poll_interval_secs = self.config.poll_interval_secs;
        let job_id = job.id;
        let kind_label = job.kind_name();

        let pending: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        tokio::spawn({
            let pending = pending.clone();
            let approval_tx = approval_tx.clone();
            async move {
                while let Some(update) = update_rx.recv().await {
                    if let vac_core::engine::RuntimeUpdate::ApprovalRequired {
                        tool_call_id,
                        tool_name,
                        arguments,
                        explanation,
                    } = update
                    {
                        let is_new = {
                            let mut p = pending.lock().await;
                            p.insert(tool_call_id.clone())
                        };

                        if !is_new {
                            continue;
                        }

                        let req = AutopilotApprovalRequest {
                            tool_call_id: tool_call_id.clone(),
                            tool_name: tool_name.clone(),
                            arguments: arguments.clone(),
                            explanation: explanation.clone(),
                            job_id,
                            created_at: Utc::now(),
                        };

                        let _ = write_approval_request(&approvals_dir, &req);

                        let queue_len = queued_len(&queue).await;
                        write_state_at(
                            &state_path,
                            AutopilotStateFile {
                                state: AutopilotState::WaitingApproval {
                                    tool_call_id: tool_call_id.clone(),
                                },
                                mode: mode.clone(),
                                poll_interval_secs,
                                queue_len,
                                current_job: Some(job_id),
                                last_event: Some(AutopilotEvent::ApprovalNeeded),
                                last_error: None,
                                updated_at: Utc::now(),
                            },
                        );

                        tokio::spawn({
                            let approvals_dir = approvals_dir.clone();
                            let state_path = state_path.clone();
                            let mode = mode.clone();
                            let queue = queue.clone();
                            let kind_label = kind_label.clone();
                            let pending = pending.clone();
                            let approval_tx = approval_tx.clone();
                            let mut shutdown = shutdown.clone();
                            async move {
                                let response = wait_for_approval_response(
                                    &approvals_dir,
                                    &tool_call_id,
                                    &mut shutdown,
                                )
                                .await
                                .unwrap_or_else(|e| vil_swarm::ApprovalResponse {
                                    tool_call_id: tool_call_id.clone(),
                                    approved: false,
                                    reason: Some(e.to_string()),
                                });

                                let _ = approval_tx.send(response);

                                {
                                    let mut p = pending.lock().await;
                                    p.remove(&tool_call_id);
                                }

                                let queue_len = queued_len(&queue).await;
                                let next_pending = {
                                    let p = pending.lock().await;
                                    p.iter().next().cloned()
                                };

                                if let Some(next_id) = next_pending {
                                    write_state_at(
                                        &state_path,
                                        AutopilotStateFile {
                                            state: AutopilotState::WaitingApproval {
                                                tool_call_id: next_id,
                                            },
                                            mode: mode.clone(),
                                            poll_interval_secs,
                                            queue_len,
                                            current_job: Some(job_id),
                                            last_event: Some(AutopilotEvent::ApprovalNeeded),
                                            last_error: None,
                                            updated_at: Utc::now(),
                                        },
                                    );
                                } else {
                                    write_state_at(
                                        &state_path,
                                        AutopilotStateFile {
                                            state: AutopilotState::Executing {
                                                job_id,
                                                kind: kind_label,
                                            },
                                            mode: mode.clone(),
                                            poll_interval_secs,
                                            queue_len,
                                            current_job: Some(job_id),
                                            last_event: Some(AutopilotEvent::StateChanged),
                                            last_error: None,
                                            updated_at: Utc::now(),
                                        },
                                    );
                                }
                            }
                        });
                    }
                }
            }
        });

        let result = match &job.kind {
            JobKind::RunTask { description } => engine
                .lock()
                .await
                .run_task_with_approvals(description, Some(update_tx), None, Some(approval_rx))
                .await
                .map_err(|e| anyhow::anyhow!("Engine error: {e}"))?,
            JobKind::PatchProposal { files } => {
                let task = format!("Review and propose patches for: {}", files.join(", "));
                engine
                    .lock()
                    .await
                    .run_task_with_approvals(&task, Some(update_tx), None, Some(approval_rx))
                    .await
                    .map_err(|e| anyhow::anyhow!("Engine error: {e}"))?
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "execute_engine_job_with_file_approvals only supports engine-backed jobs"
                ));
            }
        };

        Ok(format!(
            "Task completed: {} | modified: {} | tokens: {}",
            result.summary,
            result.modified_files.len(),
            result.total_tokens_used
        ))
    }

    fn write_state(&self, state: AutopilotStateFile) {
        if let Some(parent) = self.state_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write(&self.state_path, json);
        }
    }

    async fn execute_tool_call_job(
        &mut self,
        job: &Job,
        tool_name: String,
        arguments: serde_json::Value,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<String> {
        let tool_call_id = job.id.to_string();

        let vac_config = vac_core::VacConfig::load_with_fallback(&self.project_root)?;
        let tool_config = vac_tools::router::ToolConfigStub {
            default_policy: vac_config.tools.default_policy.clone(),
            allow: vac_config.tools.allow.clone(),
            deny: vac_config.tools.deny.clone(),
        };

        let registry = Arc::new(vac_tools::ToolRegistry::new());
        vac_tools::builtin::register_builtin_tools(&registry.clone()).await?;
        let policy = Arc::new(
            vac_tools::router::VilTrustPolicyAdapter::with_registry_and_config(
                registry.clone(),
                tool_config,
            ),
        );
        let router = vac_tools::ToolRouter::new(registry, policy);
        let context = vac_tools::registry::ToolContext::new(self.project_root.clone());

        match router.route(&tool_name, arguments.clone(), &context).await {
            Ok(value) => Ok(format!("Tool executed: {tool_name} | result={}", value)),
            Err(vac_tools::error::ToolError::ApprovalRequired(reason)) => {
                let req = AutopilotApprovalRequest {
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.clone(),
                    arguments: arguments.clone(),
                    explanation: Some(reason),
                    job_id: job.id,
                    created_at: Utc::now(),
                };
                write_approval_request(&self.approvals_dir, &req)?;

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

                let response =
                    wait_for_approval_response(&self.approvals_dir, &tool_call_id, &mut shutdown)
                        .await?;

                if response.approved {
                    let value = router
                        .route_approved(&tool_name, arguments, &context)
                        .await?;
                    Ok(format!(
                        "Tool approved+executed: {tool_name} | result={}",
                        value
                    ))
                } else {
                    anyhow::bail!(format!(
                        "Tool rejected: {}",
                        response.reason.unwrap_or_else(|| "no reason".to_string())
                    ))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Tool error: {e}")),
        }
    }
}

pub fn approval_file_path(project_root: &Path, tool_call_id: &str) -> PathBuf {
    project_root
        .join(".vac/autopilot.approvals")
        .join(format!("{tool_call_id}.json"))
}

pub fn approval_request_file_path(project_root: &Path, tool_call_id: &str) -> PathBuf {
    project_root
        .join(".vac/autopilot.approvals")
        .join(format!("{tool_call_id}.request.json"))
}

fn write_state_at(path: &PathBuf, state: AutopilotStateFile) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = std::fs::write(path, json);
    }
}

async fn queued_len(queue: &TaskQueue) -> usize {
    queue
        .list()
        .await
        .into_iter()
        .filter(|j| j.status == JobStatus::Queued)
        .count()
}

fn write_approval_request(
    dir: &std::path::Path,
    req: &AutopilotApprovalRequest,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.request.json", req.tool_call_id));
    std::fs::write(path, serde_json::to_string_pretty(req)?)?;
    Ok(())
}

async fn wait_for_approval_response(
    dir: &std::path::Path,
    tool_call_id: &str,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<vil_swarm::ApprovalResponse> {
    let approval_path = dir.join(format!("{tool_call_id}.json"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);

    loop {
        if *shutdown.borrow() {
            anyhow::bail!("Shutdown while waiting approval");
        }

        if approval_path.exists() {
            let content = std::fs::read_to_string(&approval_path)?;
            let v: serde_json::Value = serde_json::from_str(&content)?;
            let approved = v.get("approved").and_then(|b| b.as_bool()).unwrap_or(false);
            let reason = v
                .get("reason")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            let _ = std::fs::remove_file(&approval_path);
            let _ = std::fs::remove_file(dir.join(format!("{tool_call_id}.request.json")));

            return Ok(vil_swarm::ApprovalResponse {
                tool_call_id: tool_call_id.to_string(),
                approved,
                reason,
            });
        }

        if std::time::Instant::now() > deadline {
            return Ok(vil_swarm::ApprovalResponse {
                tool_call_id: tool_call_id.to_string(),
                approved: false,
                reason: Some("approval timeout".to_string()),
            });
        }

        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
            _ = shutdown.changed() => {}
        }
    }
}
