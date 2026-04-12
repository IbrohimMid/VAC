//! VacEngine — the main entry point for all VAC operations.

use crate::{
    auth,
    config::VacConfig,
    error::{VacError, VacResult},
    session::Session,
    spawn_subtask_tool::SpawnSubtaskTool,
    task::{Task, TaskResult, TaskStatus},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{error, info, instrument, warn};
use vil_llm::LlmRouter;
use vil_llm::providers::anthropic::AnthropicProvider;

/// The main VAC engine that orchestrates all subsystems.
pub struct VacEngine {
    config: VacConfig,
    project_root: PathBuf,
    session: Arc<RwLock<Session>>,
    ir_pipeline: Option<vil_ir::IrPipeline>,
    context_engine: Option<vil_context::ContextEngine>,
    memory_store: Option<vil_memory::MemoryStore>,
    swarm: Option<Arc<RwLock<vil_swarm::SwarmOrchestrator>>>,
    tool_router: Option<vac_tools::ToolRouter>,
    llm_router: Option<std::sync::Arc<vil_llm::LlmRouter>>,
    trace_recorder: Option<std::sync::Arc<std::sync::Mutex<vac_trace::TraceRecorder>>>,
}

impl VacEngine {
    /// Create a new VacEngine for the given project root.
    pub async fn new(project_root: PathBuf) -> VacResult<Self> {
        let config = VacConfig::load_with_fallback(&project_root)?;
        let session = Session::load_latest(&project_root)?
            .unwrap_or_else(|| Session::new(project_root.clone()));

        info!(project = %project_root.display(), "VAC Engine initialized");

        Ok(Self {
            config,
            project_root,
            session: Arc::new(RwLock::new(session)),
            ir_pipeline: None,
            context_engine: None,
            memory_store: None,
            swarm: None,
            tool_router: None,
            llm_router: None,
            trace_recorder: None,
        })
    }

    /// Initialize all subsystems. Called by `vac init`.
    #[instrument(skip(self))]
    pub async fn init(&mut self) -> VacResult<()> {
        self.init_with_policy(None).await
    }

    #[instrument(skip(self, policy_override))]
    pub async fn init_with_policy(
        &mut self,
        policy_override: Option<Arc<dyn vac_tools::router::PolicyEngine>>,
    ) -> VacResult<()> {
        info!("Initializing VAC subsystems...");

        info!("Initializing IR pipeline...");
        let ir = vil_ir::IrPipeline::new(&self.project_root)?;
        self.ir_pipeline = Some(ir);

        info!("Initializing context engine...");
        let shm_path = self.project_root.join(".vac/cache/vil_context.shm");
        if let Some(parent) = shm_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| VacError::Other(anyhow::anyhow!("Context cache init error: {}", e)))?;
        }
        let ctx_config = vil_context::ContextConfig {
            max_context_tokens: 8192,
            chunk_size: 512,
            chunk_overlap: 50,
            shm_path,
        };
        let ctx = vil_context::ContextEngine::new(ctx_config).await?;
        self.context_engine = Some(ctx);

        info!("Initializing memory store...");
        let mem_config = vil_memory::MemoryConfig {
            working_capacity: 100,
            db_path: self.config.memory.persist_path.clone(),
        };
        let mem = vil_memory::MemoryStore::new(mem_config).await?;
        self.memory_store = Some(mem);

        info!("Initializing tool router...");
        let registry = Arc::new(vac_tools::ToolRegistry::new());
        vac_tools::builtin::register_builtin_tools(&registry.clone())
            .await
            .map_err(|e| VacError::Other(anyhow::anyhow!("Tool registration error: {}", e)))?;
        let tool_config = vac_tools::router::ToolConfigStub {
            default_policy: self.config.tools.default_policy.clone(),
            allow: self.config.tools.allow.clone(),
            deny: self.config.tools.deny.clone(),
        };
        let policy: Arc<dyn vac_tools::router::PolicyEngine> =
            if let Some(p) = policy_override.clone() {
                p
            } else {
                Arc::new(
                    vac_tools::router::VilTrustPolicyAdapter::with_registry_and_config(
                        registry.clone(),
                        tool_config.clone(),
                    ),
                )
            };
        let tools = vac_tools::ToolRouter::new(registry.clone(), policy.clone());
        self.tool_router = Some(tools);

        if let Some(ref mcp_servers) = self.config.mcp_servers {
            info!(count = mcp_servers.len(), "Initializing MCP servers...");
            for server_config in mcp_servers {
                let client = vac_tools::mcp::McpClient::new(server_config.clone(), registry.clone());
                match client.connect().await {
                    Ok(()) => {
                        match client.register_proxy_tools().await {
                            Ok(count) => info!(name = %server_config.name, tools = count, "MCP server connected"),
                            Err(e) => warn!(name = %server_config.name, error = %e, "Failed to register MCP tools"),
                        }
                    }
                    Err(e) => warn!(name = %server_config.name, error = %e, "Failed to connect MCP server"),
                }
            }
        }

        info!("Initializing LLM router...");
        let budget_limit = self.config.llm.max_tokens_per_task;
        let mut llm_router = LlmRouter::new(&self.config.llm.default_provider, budget_limit);
        match self.config.llm.default_provider.as_str() {
            "anthropic" => {
                llm_router.add_provider(Arc::new(self.build_kilo_provider("anthropic")));
            }
            "kilo_gateway" => {
                llm_router.add_provider(Arc::new(self.build_kilo_provider("kilo_gateway")));
            }
            _ => {
                llm_router.add_provider(Arc::new(self.build_kilo_provider("anthropic")));
            }
        }
        let llm_router = Arc::new(llm_router);
        self.llm_router = Some(llm_router.clone());

        if self.config.trace.enable {
            info!("Initializing trace recorder...");
            let trace = vac_trace::TraceRecorder::new(
                self.config.trace.output_path.clone(),
                self.config.trace.enable_signing,
            )
            .map_err(|e| VacError::Other(anyhow::anyhow!("Trace error: {}", e)))?;
            self.trace_recorder = Some(std::sync::Arc::new(std::sync::Mutex::new(trace)));
        }

        info!("Initializing swarm orchestrator...");
        let swarm_policy = policy_override.unwrap_or_else(|| {
            Arc::new(
                vac_tools::router::VilTrustPolicyAdapter::with_registry_and_config(
                    registry.clone(),
                    tool_config,
                ),
            )
        });
        let tool_router = vac_tools::ToolRouter::new(registry.clone(), swarm_policy);
        let swarm = vil_swarm::SwarmOrchestrator::new(
            self.config.swarm.max_concurrent_agents,
            self.config.swarm.enable_parallel,
            Some(llm_router),
            Some(Arc::new(tool_router)),
        )
        .await?;
        self.swarm = Some(Arc::new(RwLock::new(swarm)));

        if let Some(ref swarm_arc) = self.swarm {
            let spawn_tool = SpawnSubtaskTool::new(swarm_arc.clone());
            registry.register(spawn_tool).await
                .map_err(|e| VacError::Other(anyhow::anyhow!("Spawn tool registration error: {}", e)))?;
        }

        info!("All VAC subsystems initialized successfully.");
        Ok(())
    }

    fn build_kilo_provider(&self, provider_name: &str) -> AnthropicProvider {
        let mut provider = AnthropicProvider::new();
        if let Some(config) = self.config.llm.providers.get(provider_name) {
            if let Some(api_key) = resolve_provider_api_key(config.api_key_env.as_str()) {
                provider = provider.with_api_key(&api_key);
            }
            if let Some(base_url) = &config.base_url {
                if !base_url.trim().is_empty() && std::env::var("KILO_GATEWAY_URL").is_err() {
                    provider = provider.with_base_url(base_url);
                }
            }
            if !config.model.trim().is_empty() && std::env::var("KILO_MODEL").is_err() {
                provider = provider.with_model(&config.model);
            }
        } else if let Some(api_key) = resolve_provider_api_key("KILO_API_KEY") {
            provider = provider.with_api_key(&api_key);
        }
        provider
    }

    /// Execute a task end-to-end.
    #[instrument(skip(self), fields(task_id))]
    pub async fn run_task(&mut self, description: &str) -> VacResult<TaskResult> {
        self.run_task_with_updates(description, None).await
    }

    #[instrument(skip(self, updates), fields(task_id))]
    pub async fn run_task_with_updates(
        &mut self,
        description: &str,
        updates: Option<mpsc::UnboundedSender<RuntimeUpdate>>,
    ) -> VacResult<TaskResult> {
        let task = Task::new(description);
        let task_id = task.id;
        tracing::Span::current().record("task_id", tracing::field::display(task_id.0));

        info!(task = %description, "Starting task execution");

        {
            let mut session = self.session.write().await;
            session.tasks.push(task.clone());
        }

        let result = match self.execute_task_pipeline(task, updates.clone()).await {
            Ok(result) => result,
            Err(e) => {
                error!(error = %e, "Task execution failed");
                if let Some(ref recorder) = self.trace_recorder {
                    if let Ok(mut rec) = recorder.lock() {
                        rec.record_task_failed(&task_id.0.to_string(), &e.to_string());
                        let _ = rec.flush();
                    }
                }
                TaskResult {
                    task_id,
                    status: TaskStatus::Failed(e.to_string()),
                    summary: format!("Task failed: {e}"),
                    modified_files: vec![],
                    created_files: vec![],
                    validation_score: None,
                    elapsed_ms: 0,
                    total_tokens_used: 0,
                    agent_contributions: vec![],
                }
            }
        };

        {
            let mut session = self.session.write().await;
            session.record_result(result.clone());
            if let Err(e) = session.save() {
                warn!(error = %e, "Failed to auto-save session");
            }
        }

        Ok(result)
    }

    async fn execute_task_pipeline(
        &mut self,
        task: Task,
        updates: Option<mpsc::UnboundedSender<RuntimeUpdate>>,
    ) -> VacResult<TaskResult> {
        let start = std::time::Instant::now();
        let task_id = task.id;

        if let Some(ref recorder) = self.trace_recorder {
            if let Ok(mut rec) = recorder.lock() {
                let _ = rec.record_task(&task_id.0.to_string(), &task.description);
                let _ = rec.flush();
            }
        }

        let mut swarm = self
            .swarm
            .as_ref()
            .ok_or_else(|| VacError::Task("Swarm not initialized. Run `vac init` first.".into()))?
            .write()
            .await;
        let _context = self
            .context_engine
            .as_ref()
            .ok_or_else(|| VacError::Task("Context engine not initialized.".into()))?;
        let _tools = self
            .tool_router
            .as_ref()
            .ok_or_else(|| VacError::Task("Tool router not initialized.".into()))?;

        info!("Starting agent loop execution...");
        let trace_handle = self.trace_recorder.clone();
        let update_tx = updates.clone();
        let (swarm_tx, mut swarm_rx) = mpsc::unbounded_channel::<vil_swarm::AgentLoopEvent>();
        tokio::spawn(async move {
            while let Some(event) = swarm_rx.recv().await {
                if let Some(ref recorder) = trace_handle {
                    match recorder.lock() {
                        Ok(mut rec) => {
                            match &event {
                                vil_swarm::AgentLoopEvent::LlmRequest {
                                    provider,
                                    model,
                                    message_count,
                                } => {
                                    rec.record_llm_request(provider, model, *message_count);
                                }
                                vil_swarm::AgentLoopEvent::ToolCall {
                                    id: _,
                                    name,
                                    arguments,
                                } => {
                                    rec.record_tool_call(name, arguments);
                                }
                                vil_swarm::AgentLoopEvent::ToolResult {
                                    id: _,
                                    name,
                                    content,
                                    success,
                                } => {
                                    rec.record_tool_result(name, content, *success);
                                }
                                vil_swarm::AgentLoopEvent::ModelResponse { provider, model } => {
                                    rec.record_llm_response(provider, model);
                                }
                                _ => {}
                            }
                            if let Err(e) = rec.flush() {
                                tracing::warn!("Failed to write trace to disk: {}", e);
                            }
                        }
                        Err(e) => tracing::warn!("Trace recorder lock is poisoned: {}", e),
                    }
                }
                if let Some(ref tx) = update_tx {
                    let update = match event {
                        vil_swarm::AgentLoopEvent::Status(message) => {
                            Some(RuntimeUpdate::Status(message))
                        }
                        vil_swarm::AgentLoopEvent::ModelResponse { provider, model } => {
                            Some(RuntimeUpdate::ModelInfo { provider, model })
                        }
                        vil_swarm::AgentLoopEvent::AssistantChunk(chunk) => {
                            Some(RuntimeUpdate::AssistantChunk(chunk))
                        }
                        vil_swarm::AgentLoopEvent::ToolCall {
                            id,
                            name,
                            arguments,
                        } => Some(RuntimeUpdate::ToolCall {
                            id,
                            name,
                            arguments,
                        }),
                        vil_swarm::AgentLoopEvent::ToolResult {
                            id,
                            name,
                            content,
                            success,
                        } => Some(RuntimeUpdate::ToolResult {
                            id,
                            name,
                            content,
                            success,
                        }),
                        vil_swarm::AgentLoopEvent::LlmRequest { .. } => None,
                    };
                    if let Some(up) = update {
                        let _ = tx.send(up);
                    }
                }
            }
        });

        let execution = swarm
            .agent_loop_with_events(&task.description, Some(swarm_tx))
            .await?;

        info!("Phase 3: Validating...");
        let validation_score = if let Some(ir) = &self.ir_pipeline {
            let report = vil_validate::validate_changes(ir, &execution.modified_files)?;
            Some(report.score)
        } else {
            None
        };

        if let Some(ref recorder) = self.trace_recorder {
            if let Ok(mut rec) = recorder.lock() {
                let task_id_str = task_id.0.to_string();
                rec.record_task_complete(&task_id_str, &execution.summary);
                let _ = rec.flush();
            }
        }

        let elapsed = start.elapsed();

        let result = TaskResult {
            task_id,
            status: TaskStatus::Completed,
            summary: execution.summary,
            modified_files: execution.modified_files,
            created_files: execution.created_files,
            validation_score,
            elapsed_ms: elapsed.as_millis() as u64,
            total_tokens_used: execution.total_tokens_used,
            agent_contributions: execution
                .agent_contributions
                .into_iter()
                .map(|c| crate::task::AgentContribution {
                    agent_id: c.agent_id,
                    agent_role: c.agent_role,
                    actions_taken: c.actions,
                    tokens_used: c.tokens_used,
                })
                .collect(),
        };

        if let Some(tx) = &updates {
            let _ = tx.send(RuntimeUpdate::Completed(result.clone()));
        }

        Ok(result)
    }

    /// Get engine status information.
    pub async fn status(&self) -> VacResult<EngineStatus> {
        let session = self.session.read().await;
        Ok(EngineStatus {
            project_root: self.project_root.clone(),
            session_id: session.id,
            total_tasks: session.tasks.len(),
            completed_tasks: session.metadata.total_tasks_completed,
            failed_tasks: session.metadata.total_tasks_failed,
            total_tokens_used: session.metadata.total_tokens_used,
            subsystems_initialized: self.swarm.is_some(),
        })
    }

    pub async fn history(&self) -> VacResult<Vec<TaskHistoryEntry>> {
        let session = self.session.read().await;
        let mut history = session
            .tasks
            .iter()
            .rev()
            .map(|task| {
                let result = session.results.get(&task.id);
                TaskHistoryEntry {
                    task_id: task.id.0,
                    description: task.description.clone(),
                    status: result
                        .map(|r| r.status.clone())
                        .unwrap_or_else(|| task.status.clone()),
                    updated_at: task.updated_at,
                    total_tokens_used: result.map(|r| r.total_tokens_used).unwrap_or_default(),
                    summary: result.map(|r| r.summary.clone()),
                }
            })
            .collect::<Vec<_>>();
        history.truncate(20);
        Ok(history)
    }
}

fn resolve_provider_api_key(env_name: &str) -> Option<String> {
    std::env::var(env_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| auth::resolve_kilo_api_key().ok().flatten())
}

/// Snapshot of engine status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub project_root: PathBuf,
    pub session_id: uuid::Uuid,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub total_tokens_used: u64,
    pub subsystems_initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHistoryEntry {
    pub task_id: uuid::Uuid,
    pub description: String,
    pub status: TaskStatus,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub total_tokens_used: u64,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RuntimeUpdate {
    Status(String),
    ModelInfo {
        provider: String,
        model: String,
    },
    AssistantChunk(String),
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: String,
        name: String,
        content: String,
        success: bool,
    },
    Completed(TaskResult),
}
