use std::collections::HashSet;
use std::path::PathBuf;
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
        let approvals = engine.lock().await.approval_handle();
        let store = approvals.store().clone();

        let queue = self.queue.clone();
        let state_path = self.state_path.clone();
        let mode = self.config.mode.clone();
        let poll_interval_secs = self.config.poll_interval_secs;
        let job_id = job.id;
        let kind_label = job.kind_name();

        let pending: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        tokio::spawn({
            let pending = pending.clone();
            let approvals = approvals.clone();
            let store = store.clone();
            async move {
                while let Some(update) = update_rx.recv().await {
                    if let vac_core::engine::RuntimeUpdate::ApprovalRequired {
                        tool_call_id,
                        tool_name: _,
                        arguments: _,
                        explanation: _,
                    } = update
                    {
                        let is_new = {
                            let mut p = pending.lock().await;
                            p.insert(tool_call_id.clone())
                        };

                        if !is_new {
                            continue;
                        }

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
                            let state_path = state_path.clone();
                            let mode = mode.clone();
                            let queue = queue.clone();
                            let kind_label = kind_label.clone();
                            let pending = pending.clone();
                            let approvals = approvals.clone();
                            let store = store.clone();
                            let mut shutdown = shutdown.clone();
                            async move {
                                let intent = wait_for_approval_intent(
                                    &store,
                                    &tool_call_id,
                                    &mut shutdown,
                                )
                                .await;

                                if let Ok(intent) = intent {
                                    let res = if intent.approved {
                                        approvals.approve(tool_call_id.clone()).await
                                    } else {
                                        approvals
                                            .reject(tool_call_id.clone(), intent.reason.clone())
                                            .await
                                    };

                                    if let Err(e) = res {
                                        let store = store.clone();
                                        let id = tool_call_id.clone();
                                        let _ = tokio::task::spawn_blocking(move || {
                                            store.clear_intent(&id)
                                        })
                                        .await;

                                        let queue_len = queued_len(&queue).await;
                                        write_state_at(
                                            &state_path,
                                            AutopilotStateFile {
                                                state: AutopilotState::Backoff {
                                                    until: Utc::now()
                                                        + chrono::Duration::seconds(
                                                            poll_interval_secs as i64,
                                                        ),
                                                },
                                                mode: mode.clone(),
                                                poll_interval_secs,
                                                queue_len,
                                                current_job: Some(job_id),
                                                last_event: Some(AutopilotEvent::TaskFailed),
                                                last_error: Some(e.to_string()),
                                                updated_at: Utc::now(),
                                            },
                                        );
                                    }
                                }

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
                .run_task_with_approvals(description, Some(update_tx), None, None)
                .await
                .map_err(|e| anyhow::anyhow!("Engine error: {e}"))?,
            JobKind::PatchProposal { files } => {
                let task = format!("Review and propose patches for: {}", files.join(", "));
                engine
                    .lock()
                    .await
                    .run_task_with_approvals(&task, Some(update_tx), None, None)
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
                let session_id = resolve_or_create_session_id(&self.project_root);
                let task_id = job.id;
                let active = vac_core::approval::ActiveApprovalRegistry::new();
                let approvals =
                    vac_core::ApprovalHandle::new(self.project_root.clone(), active.clone());
                let store = approvals.store().clone();

                let (approval_tx, _approval_rx) =
                    tokio::sync::mpsc::unbounded_channel::<vil_swarm::ApprovalResponse>();
                active.register(task_id, session_id, approval_tx).await;

                store.record_request(
                    tool_call_id.clone(),
                    tool_name.clone(),
                    arguments.clone(),
                    Some(reason),
                    Some(session_id),
                    Some(task_id),
                )?;

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

                let intent = wait_for_approval_intent(&store, &tool_call_id, &mut shutdown).await?;
                let _ = store.clear_intent(&tool_call_id);

                if intent.approved {
                    approvals.approve(tool_call_id.clone()).await?;
                } else {
                    approvals
                        .reject(tool_call_id.clone(), intent.reason.clone())
                        .await?;
                }

                if intent.approved {
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
                        intent.reason.unwrap_or_else(|| "no reason".to_string())
                    ))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Tool error: {e}")),
        }
    }
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

fn resolve_or_create_session_id(project_root: &PathBuf) -> uuid::Uuid {
    match vac_core::Session::load_latest(project_root) {
        Ok(Some(s)) => s.id,
        _ => {
            let session = vac_core::Session::new(project_root.clone());
            let id = session.id;
            let _ = session.save();
            id
        }
    }
}

async fn wait_for_approval_intent(
    store: &vac_core::ApprovalStore,
    tool_call_id: &str,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<vac_core::approval::ApprovalIntent> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);

    loop {
        if *shutdown.borrow() {
            anyhow::bail!("Shutdown while waiting approval");
        }

        let record = {
            let store = store.clone();
            let id = tool_call_id.to_string();
            tokio::task::spawn_blocking(move || store.load(&id))
                .await
                .map_err(|e| anyhow::anyhow!(e))??
        };

        let Some(record) = record else {
            anyhow::bail!("Unknown tool_call_id (no approval record found): {tool_call_id}");
        };

        if record.state != vac_core::ApprovalState::Pending {
            anyhow::bail!(
                "Approval is not pending (tool_call_id={}, state={:?})",
                tool_call_id,
                record.state
            );
        }

        if let Some(intent) = record.intent {
            return Ok(intent);
        }

        if std::time::Instant::now() > deadline {
            anyhow::bail!("approval timeout");
        }

        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
            _ = shutdown.changed() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn toolcall_overlap_routes_to_correct_task() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".vac")).unwrap();

        let session_id = uuid::Uuid::new_v4();
        let task_a = uuid::Uuid::new_v4();
        let task_b = uuid::Uuid::new_v4();
        let tc_a = format!("tc-{task_a}");
        let tc_b = format!("tc-{task_b}");

        let active = vac_core::approval::ActiveApprovalRegistry::new();
        let approvals = vac_core::ApprovalHandle::new(root.clone(), active.clone());
        let store = approvals.store().clone();

        let (tx_a, mut rx_a) = mpsc::unbounded_channel::<vil_swarm::ApprovalResponse>();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel::<vil_swarm::ApprovalResponse>();
        active.register(task_a, session_id, tx_a).await;
        active.register(task_b, session_id, tx_b).await;

        store
            .record_request(
                tc_a.clone(),
                "file_write".to_string(),
                serde_json::json!({"path":"a.txt","content":"a"}),
                Some("needs approval".to_string()),
                Some(session_id),
                Some(task_a),
            )
            .unwrap();
        store
            .record_request(
                tc_b.clone(),
                "file_write".to_string(),
                serde_json::json!({"path":"b.txt","content":"b"}),
                Some("needs approval".to_string()),
                Some(session_id),
                Some(task_b),
            )
            .unwrap();

        approvals.approve(tc_a.clone()).await.unwrap();
        let got_a = tokio::time::timeout(std::time::Duration::from_secs(1), rx_a.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got_a.tool_call_id, tc_a);

        let none_b = tokio::time::timeout(std::time::Duration::from_millis(100), rx_b.recv()).await;
        assert!(none_b.is_err());

        approvals.approve(tc_b.clone()).await.unwrap();
        let got_b = tokio::time::timeout(std::time::Duration::from_secs(1), rx_b.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got_b.tool_call_id, tc_b);
    }
}
