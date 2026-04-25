//! VacEngine — the main entry point for all VAC operations.

use crate::{
    ApprovalState,
    config::VacConfig,
    error::{VacError, VacResult},
    session::Session,
    spawn_subtask_tool::SpawnSubtaskTool,
    task::{Task, TaskResult, TaskStatus},
};
use serde::{Deserialize, Serialize};
use vac_approvals::{ActiveApprovalRegistry, ApprovalHandle, ApprovalStore};
use vac_ingest::ProjectContext;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskNodeStatus {
    Pending,
    Running,
    Blocked,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskNodeProjection {
    pub id: String,
    pub label: String,
    pub status: TaskNodeStatus,
    pub retry_count: u32,
    pub dependencies: Vec<String>,
    pub blockers: Vec<String>,
    pub tools_used: Vec<String>,
    pub shell_sessions: Vec<String>,
    pub artifacts: Vec<String>,
    pub approval_required: bool,
    pub approval_state: ApprovalState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskGraphProjection {
    pub nodes: Vec<TaskNodeProjection>,
    pub root_ids: Vec<String>,
    pub snapshot_at: chrono::DateTime<chrono::Utc>,
}
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{error, info, instrument, warn};
use vil_llm::LlmRouter;

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
    vil_lsp: Option<Arc<crate::lsp::service::VilLspService>>,
    pub privacy_vault: Arc<RwLock<vac_tools::PrivacyVault>>,
    pub active_approvals: ActiveApprovalRegistry,
    pub inspector_ui: Arc<RwLock<InspectorUI>>,
    project_context: Option<ProjectContext>,
}

impl VacEngine {
    /// Create a new VacEngine for the given project root.
    pub async fn new(project_root: PathBuf) -> VacResult<Self> {
        let config = VacConfig::load_with_fallback(&project_root)?;
        // Default to a fresh session per construction. Resume flows go
        // through `resume_run_state` which loads the named session
        // explicitly. Auto-loading here surprised dogfood users by
        // silently reattaching old transcripts on every TUI launch.
        let session = Session::new(project_root.clone());
        let project_context = match vac_ingest::bootstrap(project_root.clone()).await {
            Ok(context) => Some(context),
            Err(e) => {
                warn!(error = %e, "project context bootstrap failed");
                None
            }
        };

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
            vil_lsp: None,
            privacy_vault: Arc::new(RwLock::new(vac_tools::PrivacyVault::new())),
            active_approvals: ActiveApprovalRegistry::new(),
            inspector_ui: Arc::new(RwLock::new(InspectorUI::default())),
            project_context,
        })
    }

    pub fn project_context(&self) -> Option<&ProjectContext> {
        self.project_context.as_ref()
    }

    pub fn approval_handle(&self) -> ApprovalHandle {
        ApprovalHandle::new(self.project_root.clone(), self.active_approvals.clone())
    }

    pub async fn set_model_override(&mut self, model: Option<String>) -> VacResult<()> {
        let Some(ref swarm) = self.swarm else {
            return Err(VacError::Task(
                "Swarm not initialized. Run `vac init` first.".into(),
            ));
        };
        swarm.write().await.set_model_override(model);
        Ok(())
    }

    /// Access the swarm orchestrator (for direct operations like set_rulebook).
    pub fn swarm_mut(&self) -> Option<&Arc<RwLock<vil_swarm::SwarmOrchestrator>>> {
        self.swarm.as_ref()
    }

    /// Execute a tool directly by name (bypasses LLM agent loop).
    /// Used for operator-grade direct actions from TUI popups.
    pub async fn execute_tool_direct(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, VacError> {
        let router = self
            .tool_router
            .as_ref()
            .ok_or_else(|| VacError::Task("Tool router not initialized".into()))?;
        let context = vac_tools::registry::ToolContext::new(self.project_root.clone())
            .with_session_id(self.session.read().await.id);
        router
            .route_approved(tool_name, args, &context)
            .await
            .map_err(|e| VacError::Task(format!("Tool execution failed: {e}")))
    }

    pub fn available_models(&self) -> Vec<(String, String)> {
        let mut models: Vec<(String, String)> = self
            .config
            .llm
            .providers
            .iter()
            .filter_map(|(provider, cfg)| {
                cfg.model
                    .as_ref()
                    .map(|model| (provider.clone(), model.clone()))
            })
            .collect();
        models.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        models
    }

    pub fn task_graph_projection(&self) -> Option<TaskGraphProjection> {
        let session = self.session.try_read().ok()?;
        let inspector = self.inspector_ui.try_read().ok()?;
        if session.tasks.is_empty() {
            return None;
        }
        let mut nodes: Vec<TaskNodeProjection> = session
            .tasks
            .iter()
            .map(|task| {
                let artifacts = session
                    .results
                    .get(&task.id)
                    .map(|r| {
                        let mut a = r.modified_files.clone();
                        a.extend(r.created_files.clone());
                        a
                    })
                    .unwrap_or_default();
                let approval_required =
                    task.constraints.require_approval || task.constraints.approval_policy.is_some();
                TaskNodeProjection {
                    id: task.id.0.to_string(),
                    label: task.description.clone(),
                    status: match &task.status {
                        crate::task::TaskStatus::Pending => TaskNodeStatus::Pending,
                        crate::task::TaskStatus::Planning
                        | crate::task::TaskStatus::Executing
                        | crate::task::TaskStatus::Validating => TaskNodeStatus::Running,
                        crate::task::TaskStatus::Completed => TaskNodeStatus::Completed,
                        crate::task::TaskStatus::Failed(msg) => TaskNodeStatus::Failed(msg.clone()),
                        crate::task::TaskStatus::Cancelled => {
                            TaskNodeStatus::Failed("cancelled".to_string())
                        }
                    },
                    retry_count: 0,
                    dependencies: task.parent_task.iter().map(|p| p.0.to_string()).collect(),
                    blockers: inspector
                        .blockers
                        .get(&task.id.0)
                        .cloned()
                        .unwrap_or_default(),
                    tools_used: inspector
                        .active_tools
                        .get(&task.id.0)
                        .cloned()
                        .unwrap_or_default(),
                    shell_sessions: inspector
                        .shell_sessions
                        .get(&task.id.0)
                        .cloned()
                        .unwrap_or_default(),
                    artifacts,
                    approval_required,
                    approval_state: if approval_required {
                        ApprovalState::Pending
                    } else {
                        ApprovalState::Approved
                    },
                }
            })
            .collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut root_ids: Vec<String> = session
            .tasks
            .iter()
            .filter(|t| t.parent_task.is_none())
            .map(|t| t.id.0.to_string())
            .collect();
        root_ids.sort();
        Some(TaskGraphProjection {
            nodes,
            root_ids,
            snapshot_at: chrono::Utc::now(),
        })
    }

    fn build_llm_router(&self) -> LlmRouter {
        LlmRouter::from_config(&self.config.llm)
            .with_rate_limit(self.config.llm.requests_per_minute)
    }

    /// Reload configuration dynamically and rebuild subsystems without a full restart.
    pub async fn reload_config(&mut self) -> VacResult<Vec<String>> {
        let warnings = Vec::new();
        info!("Reloading VAC configuration dynamically...");

        let new_config = VacConfig::load_with_fallback(&self.project_root)?;
        self.config = new_config;

        info!("Rebuilding LLM router with new config...");
        let llm_router = Arc::new(self.build_llm_router());
        self.llm_router = Some(llm_router.clone());

        if let Some(ref mut swarm_arc) = self.swarm {
            info!("Injecting new LLM router into swarm...");
            let mut swarm = swarm_arc.write().await;
            swarm.set_llm_router(Some(llm_router));
            // We could also re-initialize tool_router here if tool config changed,
            // but for now we focus on the core LLM config swap parity.
        }

        info!("Config reload complete.");
        Ok(warnings)
    }

    /// Initialize all subsystems. Called by `vac init`.
    #[instrument(skip(self))]
    pub async fn init(&mut self) -> VacResult<Vec<String>> {
        self.init_with_policy(None).await
    }

    #[instrument(skip(self, policy_override))]
    pub async fn init_with_policy(
        &mut self,
        policy_override: Option<Arc<dyn vac_tools::router::PolicyEngine>>,
    ) -> VacResult<Vec<String>> {
        let mut warnings = Vec::new();
        info!("Initializing VAC subsystems...");

        // Apply resource governance (P4.1)
        if let Some(cap) = self.config.memory_cap_bytes {
            if let Err(e) = vac_tools::resource_limits::apply_rlimit_as(cap) {
                warn!("Could not set memory cap ({cap} bytes): {e}");
            } else {
                info!(memory_cap_bytes = cap, "Memory cap applied");
            }
        }
        if let Some(quota) = self.config.disk_quota_bytes {
            if let Err(e) = vac_tools::resource_limits::apply_rlimit_fsize(quota) {
                warn!("Could not set disk quota ({quota} bytes): {e}");
            } else {
                info!(disk_quota_bytes = quota, "Disk quota applied");
            }
        }

        info!("Initializing IR pipeline...");
        let ir = vil_ir::IrPipeline::new_async(&self.project_root).await?;
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
        if let Some(parent) = self.config.memory.persist_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    VacError::Other(anyhow::anyhow!("Memory store init error: {}", e))
                })?;
            }
        }
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

        let mut mcp_servers = self.config.mcp_servers.clone().unwrap_or_default();
        if !self.config.mcp_presets.is_empty() {
            let (preset_servers, preset_warnings) =
                vac_tools::mcp::resolve_mcp_presets(&self.config.mcp_presets);
            warnings.extend(preset_warnings);
            mcp_servers.extend(preset_servers);
        }

        if !mcp_servers.is_empty() {
            info!(count = mcp_servers.len(), "Initializing MCP servers...");
            for server_config in &mcp_servers {
                if !server_config.is_allowed_in_mode(&self.config.runtime.environment_mode) {
                    info!(
                        name = %server_config.name,
                        environment_mode = %self.config.runtime.environment_mode,
                        "Skipping MCP server outside allowed_in_modes"
                    );
                    continue;
                }

                let client =
                    vac_tools::mcp::McpClient::new(server_config.clone(), registry.clone());
                match client.connect().await {
                    Ok(()) => match client.register_proxy_tools().await {
                        Ok(count) => {
                            info!(
                                name = %server_config.name,
                                trust = server_config.effective_trust_class().as_label(),
                                tools = count,
                                "MCP server connected"
                            )
                        }
                        Err(e) => {
                            warn!(name = %server_config.name, error = %e, "Failed to register MCP tools")
                        }
                    },
                    Err(e) => {
                        let err_msg = format!(
                            "Failed to connect MCP server '{}': {}",
                            server_config.name, e
                        );
                        warn!("{}", err_msg);
                        warnings.push(err_msg);
                    }
                }
            }
        }

        info!("Initializing LLM router...");
        let llm_router = Arc::new(self.build_llm_router());
        self.llm_router = Some(llm_router.clone());

        if self.config.trace.enable {
            info!("Initializing trace recorder...");
            let trace = vac_trace::TraceRecorder::new(
                self.config.trace.output_path.clone(),
                self.config.trace.enable_signing,
            )
            .map_err(|e| VacError::Other(anyhow::anyhow!("Trace error: {}", e)))?
            .with_redaction(
                self.config.trace.redaction.strip_paths,
                &self.config.trace.redaction.custom_patterns,
            );
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
        let mut swarm = vil_swarm::SwarmOrchestrator::new(
            self.config.swarm.max_concurrent_agents,
            self.config.swarm.enable_parallel,
            Some(llm_router),
            Some(Arc::new(tool_router)),
            self.privacy_vault.clone(),
        )
        .await?;
        // Wire agent strategy from config (SwarmConfig.strategy). Emit
        // an AgentDecision trace record so the selection appears in
        // `vac decisions` / `vac eval` alongside runtime choices.
        swarm.set_strategy_by_name(&self.config.swarm.strategy);
        if let Some(ref recorder) = self.trace_recorder {
            if let Ok(mut rec) = recorder.lock() {
                rec.record_agent_decision(
                    None,
                    swarm.strategy_name(),
                    &["default", "conservative"],
                    Some("strategy resolved from SwarmConfig.strategy"),
                );
                let _ = rec.flush();
            }
        }

        // Phase 5: inject VIL project profile + knowledge into swarm
        let profile = crate::detector::VilProjectProfile::detect(&self.project_root);
        if profile.is_vil_project {
            info!(archetype = %profile.archetype, "VIL project detected");
            let kb = vil_knowledge::KnowledgeBase::load(&self.project_root);
            let swarm_profile = vil_swarm::VilProjectProfile {
                archetype: convert_archetype(&profile.archetype),
                vil_deps: profile.vil_deps.clone(),
                detected_constructs: profile.detected_constructs.clone(),
                is_vil_project: true,
            };
            swarm.set_project_profile(swarm_profile);
            swarm.set_knowledge(kb);
        }

        // P2.3/Phase 7: load rulebooks via multi-rulebook engine
        if self.config.rulebook.enable {
            let books = crate::rulebook::RulebookLoader::load_all(
                &self.project_root,
                &self.config.rulebook.paths,
            );
            let validation = crate::rulebook::validate_rulebooks(&books);
            if !validation.is_valid() {
                for err in &validation.errors {
                    tracing::error!(error = %err, "Rulebook validation error");
                }
                if self.config.rulebook.fail_on_invalid {
                    return Err(VacError::Other(anyhow::anyhow!(
                        "Rulebook validation failed: {}",
                        validation.errors.join("; ")
                    )));
                }
            }
            for warn in &validation.warnings {
                tracing::warn!(warning = %warn, "Rulebook warning");
            }
            let archetype_str = profile.archetype.to_string();
            let ctx = crate::rulebook::ResolvedRuleContext::build(
                books,
                if profile.is_vil_project {
                    Some(&archetype_str)
                } else {
                    None
                },
            );
            if let Some(overlay) = ctx.to_prompt_overlay() {
                swarm.set_rulebook(overlay);
            }
        }

        self.swarm = Some(Arc::new(RwLock::new(swarm)));

        // Phase 6: start vil-lsp service for VIL projects
        if profile.is_vil_project && self.config.vil_lsp.enable {
            let lsp_config = &self.config.vil_lsp;
            match tokio::time::timeout(
                std::time::Duration::from_millis(lsp_config.startup_timeout_ms),
                crate::lsp::service::VilLspService::new(lsp_config, self.project_root.clone()),
            )
            .await
            {
                Ok(Ok(svc)) => {
                    if lsp_config.analyze_on_init {
                        let _ = svc.analyze_workspace().await;
                    }
                    info!("vil-lsp service started");
                    // Trace LSP start
                    if let Some(ref recorder) = self.trace_recorder {
                        if let Ok(mut rec) = recorder.lock() {
                            rec.record(
                                vac_trace::RecordType::LspStarted,
                                None,
                                serde_json::json!({ "binary": lsp_config.binary_path.display().to_string() }),
                            );
                            let _ = rec.flush();
                        }
                    }
                    self.vil_lsp = Some(Arc::new(svc));
                }
                Ok(Err(e)) => {
                    if lsp_config.fail_on_unavailable {
                        return Err(VacError::Other(anyhow::anyhow!("vil-lsp unavailable: {e}")));
                    }
                    warn!(error = %e, "vil-lsp unavailable; continuing without editor diagnostics");
                }
                Err(_) => {
                    if lsp_config.fail_on_unavailable {
                        return Err(VacError::Other(anyhow::anyhow!(
                            "vil-lsp startup timed out"
                        )));
                    }
                    warn!("vil-lsp startup timed out; continuing without editor diagnostics");
                }
            }
        }

        if let Some(ref swarm_arc) = self.swarm {
            let spawn_tool = SpawnSubtaskTool::new(swarm_arc.clone());
            registry.register(spawn_tool).await.map_err(|e| {
                VacError::Other(anyhow::anyhow!("Spawn tool registration error: {}", e))
            })?;
        }

        info!("All VAC subsystems initialized successfully.");
        Ok(warnings)
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
        self.run_task_with_cancel(description, updates, None).await
    }

    #[instrument(skip(self, updates, cancel), fields(task_id))]
    pub async fn run_task_with_cancel(
        &mut self,
        description: &str,
        updates: Option<mpsc::UnboundedSender<RuntimeUpdate>>,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> VacResult<TaskResult> {
        self.run_task_with_approvals(description, updates, cancel, None)
            .await
    }

    /// Run a task with image attachments and structured approval support.
    #[instrument(skip(self, updates, cancel, approval_rx, image_parts), fields(task_id))]
    pub async fn run_task_with_images(
        &mut self,
        description: &str,
        updates: Option<mpsc::UnboundedSender<RuntimeUpdate>>,
        cancel: Option<tokio_util::sync::CancellationToken>,
        approval_rx: Option<mpsc::UnboundedReceiver<vil_swarm::ApprovalResponse>>,
        image_parts: Vec<vil_llm::provider::ImagePart>,
    ) -> VacResult<TaskResult> {
        // Store image parts for the next agent loop execution
        if !image_parts.is_empty() {
            if let Some(ref swarm) = self.swarm {
                let mut s = swarm.write().await;
                s.set_pending_images(image_parts);
            }
        }
        self.run_task_with_approvals(description, updates, cancel, approval_rx)
            .await
    }

    /// Run a task with structured approval support.
    /// The `approval_rx` channel receives approval/rejection decisions from the TUI.
    #[instrument(skip(self, updates, cancel, approval_rx), fields(task_id))]
    pub async fn run_task_with_approvals(
        &mut self,
        description: &str,
        updates: Option<mpsc::UnboundedSender<RuntimeUpdate>>,
        cancel: Option<tokio_util::sync::CancellationToken>,
        approval_rx: Option<mpsc::UnboundedReceiver<vil_swarm::ApprovalResponse>>,
    ) -> VacResult<TaskResult> {
        // Substitute secrets before sending to LLM
        let safe_description = {
            let mut vault = self.privacy_vault.write().await;
            vault.substitute(description)
        };
        let task = Task::new(&safe_description);
        let task_id = task.id;
        tracing::Span::current().record("task_id", tracing::field::display(task_id.0));

        // Log redacted: never log original description to avoid secret leakage
        info!(task_id = %task_id.0, "Starting task execution");

        // If we didn't get an approval_rx, create one and register it scoped to this task.
        let final_approval_rx = if approval_rx.is_none() {
            let (app_tx, app_rx) = mpsc::unbounded_channel();
            let session_id = self.session.read().await.id;
            self.active_approvals
                .register(task_id.0, session_id, app_tx)
                .await;
            Some(app_rx)
        } else {
            approval_rx
        };

        {
            let mut session = self.session.write().await;
            session.tasks.push(task.clone());
        }

        let result = match self
            .execute_task_pipeline_with_approvals(task, updates.clone(), cancel, final_approval_rx)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                // Check if this is a cancellation
                let is_cancelled = matches!(e, VacError::Swarm(vil_swarm::SwarmError::Cancelled));

                if is_cancelled {
                    info!("Task cancelled by user");
                } else {
                    error!(error = %e, "Task execution failed");
                }

                if let Some(ref recorder) = self.trace_recorder {
                    if let Ok(mut rec) = recorder.lock() {
                        rec.record_task_failed(&task_id.0.to_string(), &e.to_string());
                        let _ = rec.flush();
                    }
                }

                // Emit appropriate event
                if let Some(ref tx) = updates {
                    if is_cancelled {
                        let _ = tx.send(RuntimeUpdate::Cancelled);
                    } else {
                        let _ = tx.send(RuntimeUpdate::Failed(e.to_string()));
                    }
                }

                TaskResult {
                    task_id,
                    status: if is_cancelled {
                        TaskStatus::Cancelled
                    } else {
                        TaskStatus::Failed(e.to_string())
                    },
                    summary: if is_cancelled {
                        "Task cancelled by user".to_string()
                    } else {
                        format!("Task failed: {e}")
                    },
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

        // Save checkpoint for potential resume
        self.save_checkpoint(&result).await;

        Ok(result)
    }

    async fn execute_task_pipeline_with_approvals(
        &mut self,
        task: Task,
        updates: Option<mpsc::UnboundedSender<RuntimeUpdate>>,
        cancel: Option<tokio_util::sync::CancellationToken>,
        approval_rx: Option<mpsc::UnboundedReceiver<vil_swarm::ApprovalResponse>>,
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

        // Wire context engine into ToolContext via session — provides SHM + semantic chunking
        // to all tools invoked during this task's execution.
        let context_shm = self.context_engine.as_ref().and_then(|ctx| ctx.shm_arc());
        let _ = context_shm; // SHM arc available for future ToolContext wiring per-tool

        info!("Starting agent loop execution...");
        let trace_handle = self.trace_recorder.clone();
        let update_tx = updates.clone();
        let (swarm_tx, swarm_rx) = mpsc::unbounded_channel::<vil_swarm::AgentLoopEvent>();
        let session_id = self.session.read().await.id;
        let project_root = self.project_root.clone();
        let privacy_vault = self.privacy_vault.clone();
        let approval_store = ApprovalStore::new(project_root.clone());
        let task_uuid = task_id.0;
        let session_uuid = session_id;

        // Phase 6: inject LSP diagnostic context into swarm before execution
        if let Some(ref lsp) = self.vil_lsp {
            let ctx = lsp
                .prompt_context(self.config.vil_lsp.max_prompt_items)
                .await;
            if !ctx.is_empty() {
                swarm.set_lsp_prompt_context(vil_swarm::ExternalDiagnosticContext {
                    total_errors: ctx.total_errors,
                    total_warnings: ctx.total_warnings,
                    top_findings: ctx.top_findings,
                });
            }
        }
        spawn_agent_event_bridge(
            swarm_rx,
            trace_handle,
            update_tx,
            privacy_vault,
            approval_store,
            session_uuid,
            task_uuid,
            self.inspector_ui.clone(),
        );

        let execution = swarm
            .agent_loop_with_full_context(
                &task.description,
                Some(swarm_tx),
                Some(session_id),
                Some(project_root),
                cancel,
                approval_rx,
            )
            .await?;

        info!("Phase 3: Validating...");
        let validation_score = if let Some(ir) = &self.ir_pipeline {
            tracing::debug!(
                "Running IR validation on {} modified files",
                execution.modified_files.len()
            );
            match vil_validate::validate_changes(ir, &execution.modified_files) {
                Ok(report) => {
                    info!(
                        score = report.score,
                        issues_count = report.issues.len(),
                        "Validation completed"
                    );
                    for issue in &report.issues {
                        warn!(issue = %issue, "Validation issue detected");
                    }
                    if let Some(ref tx) = updates {
                        let _ = tx.send(RuntimeUpdate::ValidationResult {
                            score: report.score,
                            issues: report.issues.clone(),
                        });
                    }
                    Some(report.score)
                }
                Err(e) => {
                    error!(error = %e, "Validation pipeline failed");
                    return Err(e.into());
                }
            }
        } else {
            tracing::debug!("Validation skipped: IR pipeline not available");
            None
        };

        // Phase 6: post-edit LSP recheck
        if let Some(ref lsp) = self.vil_lsp {
            if !execution.modified_files.is_empty() && self.config.vil_lsp.analyze_after_edit {
                // Incremental sync: notify LSP of each changed file immediately
                for file in &execution.modified_files {
                    lsp.notify_file_changed(file).await;
                }
                let _ = lsp.analyze_files(&execution.modified_files).await;
                let snapshot = lsp.snapshot().await;
                if let Some(ref tx) = updates {
                    let _ = tx.send(RuntimeUpdate::LspDiagnostics(snapshot.clone()));
                }
                // strict-vil: block if LSP still reports errors on modified files
                let profile =
                    crate::profile::ProfileOverride::resolve(&crate::profile::ProfileName::parse(
                        &std::env::var("VAC_PROFILE").unwrap_or_default(),
                    ));
                if profile.validator_blocks && snapshot.total_errors > 0 {
                    let post_errors = lsp
                        .diagnostics_for_files(&execution.modified_files)
                        .await
                        .into_iter()
                        .filter(|d| d.severity == crate::lsp::types::LspSeverity::Error)
                        .count();
                    if post_errors > 0 {
                        return Err(VacError::Task(format!(
                            "vil-lsp reports {post_errors} remaining semantic error(s) on modified files (strict-vil gate)"
                        )));
                    }
                }
            }
        }

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

    /// Save a checkpoint of the current session for potential resume.
    async fn save_checkpoint(&self, _result: &TaskResult) {
        // We no longer write the custom CheckpointEnvelope or dual semantics here.
        // SwarmOrchestrator already saves AgentRunState to `_state.json`.
        // If we want session-level info, we rely on the normal `Session::save`
        // which writes to `.vac/sessions/`.
        let session = self.session.read().await;
        if let Err(e) = session.save() {
            warn!(error = %e, "Failed to save session metadata");
        }
    }

    /// Resumes execution of a previously saved session.
    pub async fn resume_run_state(
        &mut self,
        session_id: uuid::Uuid,
        updates: Option<tokio::sync::mpsc::UnboundedSender<RuntimeUpdate>>,
        cancel: Option<tokio_util::sync::CancellationToken>,
        approval_rx: Option<tokio::sync::mpsc::UnboundedReceiver<vil_swarm::ApprovalResponse>>,
    ) -> VacResult<TaskResult> {
        let checkpoint_dir = self.project_root.join(".vac/checkpoints");
        let state_path = checkpoint_dir.join(format!("{}_state.json", session_id));

        if !state_path.exists() {
            return Err(VacError::Config(format!(
                "State checkpoint not found at {}",
                state_path.display()
            )));
        }

        let mut state = vil_swarm::run_state::AgentRunState::from_checkpoint(&state_path)
            .map_err(|e| VacError::Config(format!("Failed to load state: {}", e)))?;

        // Re-hydrate session in memory
        if let Ok(Some(session_disk)) =
            crate::session::Session::load(&self.project_root, session_id)
        {
            let mut session = self.session.write().await;
            session.id = session_id;
            session.metadata = session_disk.metadata;
            session.tasks = session_disk.tasks;
            session.results = session_disk.results;
        }

        let mut resume_task_id = None;
        if !state.pending_approvals.is_empty() {
            let store = ApprovalStore::new(self.project_root.clone());
            let first = state
                .pending_approvals
                .first()
                .map(|p| p.tool_call_id.clone())
                .unwrap_or_default();
            let record = tokio::task::spawn_blocking(move || store.load(&first))
                .await
                .map_err(|e| VacError::Task(format!("Approval store task failed: {e}")))?
                .map_err(|e| VacError::Task(format!("Approval store error: {e}")))?;
            resume_task_id = record.and_then(|r| r.task_id);
        }
        let resume_task_id = resume_task_id.unwrap_or_else(uuid::Uuid::new_v4);

        let mut final_approval_rx = approval_rx;
        if final_approval_rx.is_none() {
            let (app_tx, app_rx) = tokio::sync::mpsc::unbounded_channel();
            self.active_approvals
                .register(resume_task_id, session_id, app_tx)
                .await;
            final_approval_rx = Some(app_rx);
        }

        let mut swarm = self
            .swarm
            .as_ref()
            .ok_or_else(|| VacError::Task("Swarm not initialized. Run `vac init` first.".into()))?
            .write()
            .await;

        let task_desc = state
            .messages
            .iter()
            .find(|m| matches!(m.role, vil_llm::provider::Role::User))
            .map(|m| m.content.clone())
            .unwrap_or_else(|| "Resumed task".to_string());

        let trace_handle = self.trace_recorder.clone();
        let update_tx = updates.clone();
        let (swarm_tx, swarm_rx) =
            tokio::sync::mpsc::unbounded_channel::<vil_swarm::AgentLoopEvent>();
        let project_root = self.project_root.clone();
        let privacy_vault = self.privacy_vault.clone();
        let approval_store = ApprovalStore::new(project_root.clone());
        let task_uuid = resume_task_id;
        let session_uuid = session_id;

        spawn_agent_event_bridge(
            swarm_rx,
            trace_handle,
            update_tx,
            privacy_vault,
            approval_store,
            session_uuid,
            task_uuid,
            self.inspector_ui.clone(),
        );

        state.cancel = cancel.clone();

        let result = swarm
            .resume_agent_loop(
                &mut state,
                &task_desc,
                Some(swarm_tx),
                Some(session_id),
                Some(project_root),
                final_approval_rx,
            )
            .await;

        let task_result = match result {
            Ok(exec_res) => TaskResult {
                task_id: crate::task::TaskId(uuid::Uuid::new_v4()),
                status: crate::task::TaskStatus::Completed,
                summary: exec_res.summary,
                modified_files: exec_res.modified_files,
                created_files: exec_res.created_files,
                total_tokens_used: exec_res.total_tokens_used,
                agent_contributions: vec![],
                elapsed_ms: 0,
                validation_score: None,
            },
            Err(e) => {
                let _ = state.save_checkpoint(&state_path, Some(session_id));
                TaskResult {
                    task_id: crate::task::TaskId(uuid::Uuid::new_v4()),
                    status: crate::task::TaskStatus::Failed(e.to_string()),
                    summary: format!("Failed: {}", e),
                    modified_files: state.modified_files.clone(),
                    created_files: state.created_files.clone(),
                    total_tokens_used: state.total_tokens,
                    agent_contributions: vec![],
                    elapsed_ms: 0,
                    validation_score: None,
                }
            }
        };

        self.save_checkpoint(&task_result).await;

        Ok(task_result)
    }
    pub async fn status(&self) -> VacResult<EngineStatus> {
        let session = self.session.read().await;
        let inspector = self.inspector_ui.read().await.clone();
        Ok(EngineStatus {
            project_root: self.project_root.clone(),
            session_id: session.id,
            total_tasks: session.tasks.len(),
            completed_tasks: session.metadata.total_tasks_completed,
            failed_tasks: session.metadata.total_tasks_failed,
            total_tokens_used: session.metadata.total_tokens_used,
            subsystems_initialized: self.swarm.is_some(),
            inspector,
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

    /// Load a specific session by ID, swapping the current session in memory.
    /// This ensures transcript, history, and engine state are synchronized.
    pub async fn load_session(&mut self, session_id: uuid::Uuid) -> VacResult<()> {
        let session_path = self
            .project_root
            .join(".vac/sessions")
            .join(format!("{}.json", session_id));

        if !session_path.exists() {
            return Err(VacError::Other(anyhow::anyhow!(
                "Session not found: {}",
                session_id
            )));
        }

        let content = std::fs::read_to_string(&session_path)?;
        let session: Session = serde_json::from_str(&content)?;

        info!(session_id = %session_id, "Session loaded");

        let mut current = self.session.write().await;
        *current = session;

        Ok(())
    }

    /// Get current session ID.
    pub async fn session_id(&self) -> uuid::Uuid {
        self.session.read().await.id
    }

    /// List all available sessions for this project.
    pub async fn list_sessions(&self) -> VacResult<Vec<Session>> {
        Session::list_all(&self.project_root)
    }

    /// Get read access to the current session.
    pub fn session(&self) -> &Arc<RwLock<Session>> {
        &self.session
    }

    /// Approve a pending tool call for the currently running task.
    pub async fn approve_tool_call(&self, tool_call_id: String) -> VacResult<()> {
        Ok(self.approval_handle().approve(tool_call_id).await?)
    }

    /// Reject a pending tool call for the currently running task.
    pub async fn reject_tool_call(
        &self,
        tool_call_id: String,
        reason: Option<String>,
    ) -> VacResult<()> {
        Ok(self.approval_handle().reject(tool_call_id, reason).await?)
    }
}

/// Convert vac_core::VilArchetype to vil_swarm::VilArchetype (same shape, separate types).
fn convert_archetype(a: &crate::detector::VilArchetype) -> vil_swarm::VilArchetype {
    match a {
        crate::detector::VilArchetype::Server => vil_swarm::VilArchetype::Server,
        crate::detector::VilArchetype::Pipeline => vil_swarm::VilArchetype::Pipeline,
        crate::detector::VilArchetype::Plugin => vil_swarm::VilArchetype::Plugin,
        crate::detector::VilArchetype::Hybrid(parts) => {
            vil_swarm::VilArchetype::Hybrid(parts.iter().map(convert_archetype).collect())
        }
        crate::detector::VilArchetype::Unknown => vil_swarm::VilArchetype::Unknown,
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_agent_event_bridge(
    mut swarm_rx: mpsc::UnboundedReceiver<vil_swarm::AgentLoopEvent>,
    trace_handle: Option<std::sync::Arc<std::sync::Mutex<vac_trace::TraceRecorder>>>,
    update_tx: Option<mpsc::UnboundedSender<RuntimeUpdate>>,
    privacy_vault: Arc<RwLock<vac_tools::PrivacyVault>>,
    approval_store: ApprovalStore,
    session_uuid: uuid::Uuid,
    task_uuid: uuid::Uuid,
    inspector_ui: Arc<RwLock<InspectorUI>>,
) {
    tokio::spawn(async move {
        while let Some(event) = swarm_rx.recv().await {
            record_agent_event(trace_handle.as_ref(), &event);

            // Track Inspector UI metrics
            match &event {
                vil_swarm::AgentLoopEvent::ToolCall { name, .. } => {
                    let mut ui = inspector_ui.write().await;
                    let tools = ui.active_tools.entry(task_uuid).or_default();
                    tools.push(name.clone());
                    if name == "shell" || name == "run_command" {
                        let sessions = ui.shell_sessions.entry(task_uuid).or_default();
                        sessions.push("shell_session_active".to_string());
                    }
                }
                vil_swarm::AgentLoopEvent::ToolResult { name, .. } => {
                    let mut ui = inspector_ui.write().await;
                    if let Some(tools) = ui.active_tools.get_mut(&task_uuid) {
                        if let Some(pos) = tools.iter().position(|x| x == name) {
                            tools.remove(pos);
                        }
                    }
                    if name == "shell" || name == "run_command" {
                        if let Some(sessions) = ui.shell_sessions.get_mut(&task_uuid) {
                            if !sessions.is_empty() {
                                sessions.pop();
                            }
                        }
                    }
                }
                vil_swarm::AgentLoopEvent::ApprovalRequired { tool_name, .. } => {
                    let mut ui = inspector_ui.write().await;
                    let blockers = ui.blockers.entry(task_uuid).or_default();
                    blockers.push(format!("Pending approval for: {}", tool_name));
                }
                vil_swarm::AgentLoopEvent::ApprovalDecision { tool_name, .. } => {
                    let mut ui = inspector_ui.write().await;
                    if let Some(blockers) = ui.blockers.get_mut(&task_uuid) {
                        let target = format!("Pending approval for: {}", tool_name);
                        if let Some(pos) = blockers.iter().position(|x| x == &target) {
                            blockers.remove(pos);
                        }
                    }
                }
                _ => {}
            }

            if let Some(ref tx) = update_tx {
                if let Some(update) = runtime_update_from_agent_event(
                    event,
                    &privacy_vault,
                    &approval_store,
                    session_uuid,
                    task_uuid,
                )
                .await
                {
                    let _ = tx.send(update);
                }
            }
        }
    });
}

fn record_agent_event(
    trace_handle: Option<&std::sync::Arc<std::sync::Mutex<vac_trace::TraceRecorder>>>,
    event: &vil_swarm::AgentLoopEvent,
) {
    let Some(recorder) = trace_handle else {
        return;
    };

    match recorder.lock() {
        Ok(mut rec) => {
            match event {
                vil_swarm::AgentLoopEvent::LlmRequest {
                    provider,
                    model,
                    message_count,
                } => {
                    rec.record_llm_request(provider, model, *message_count);
                }
                vil_swarm::AgentLoopEvent::AssistantMessage {
                    content,
                    tool_calls,
                } => {
                    rec.record(
                        vac_trace::RecordType::AgentMessage,
                        None,
                        serde_json::json!({
                            "content": content,
                            "tool_calls": tool_calls,
                        }),
                    );
                }
                vil_swarm::AgentLoopEvent::ToolCall {
                    id: _,
                    name,
                    arguments,
                } => {
                    rec.record_tool_call(name, arguments);
                    // Paired AgentDecision for replay/eval. `rejected` is
                    // empty until AgentStrategy supplies alternatives.
                    rec.record_agent_decision(
                        None,
                        name,
                        &[],
                        Some(&format!("args_len={}", arguments.to_string().len())),
                    );
                }
                vil_swarm::AgentLoopEvent::ToolResult {
                    id: _,
                    name,
                    content,
                    success,
                } => {
                    rec.record_tool_result(name, content, *success);
                }
                vil_swarm::AgentLoopEvent::ApprovalDecision {
                    tool_call_id,
                    tool_name,
                    approved,
                    reason,
                } => {
                    rec.record(
                        vac_trace::RecordType::PolicyDecision,
                        None,
                        serde_json::json!({
                            "kind": "approval_decision",
                            "tool_call_id": tool_call_id,
                            "tool_name": tool_name,
                            "approved": approved,
                            "reason": reason,
                        }),
                    );
                }
                vil_swarm::AgentLoopEvent::ReasoningTransition { from, to, attempt } => {
                    rec.record(
                        vac_trace::RecordType::PolicyDecision,
                        None,
                        serde_json::json!({
                            "kind": "reasoning_transition",
                            "from": from,
                            "to": to,
                            "attempt": attempt,
                        }),
                    );
                }
                vil_swarm::AgentLoopEvent::ModelResponse { provider, model } => {
                    rec.record_llm_response(provider, model);
                }
                _ => {}
            }

            if let Err(e) = rec.flush() {
                warn!(error = %e, "Failed to write trace to disk");
            }
        }
        Err(e) => warn!(error = %e, "Trace recorder lock is poisoned"),
    }
}

async fn runtime_update_from_agent_event(
    event: vil_swarm::AgentLoopEvent,
    privacy_vault: &Arc<RwLock<vac_tools::PrivacyVault>>,
    approval_store: &ApprovalStore,
    session_uuid: uuid::Uuid,
    task_uuid: uuid::Uuid,
) -> Option<RuntimeUpdate> {
    match event {
        vil_swarm::AgentLoopEvent::Status(message) => Some(RuntimeUpdate::Status(message)),
        vil_swarm::AgentLoopEvent::ModelResponse { provider, model } => {
            Some(RuntimeUpdate::ModelInfo { provider, model })
        }
        vil_swarm::AgentLoopEvent::AssistantChunk(chunk) => {
            let restored = {
                let vault = privacy_vault.read().await;
                vault.restore(&chunk)
            };
            Some(RuntimeUpdate::AssistantChunk(restored))
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
            envelope: None,
        }),
        vil_swarm::AgentLoopEvent::ApprovalRequired {
            tool_call_id,
            tool_name,
            arguments,
            explanation,
        } => {
            persist_approval_request(
                approval_store,
                tool_call_id.clone(),
                tool_name.clone(),
                arguments.clone(),
                explanation.clone(),
                session_uuid,
                task_uuid,
            )
            .await;

            Some(RuntimeUpdate::ApprovalRequired {
                tool_call_id,
                tool_name,
                arguments,
                explanation,
            })
        }
        vil_swarm::AgentLoopEvent::ApprovalDecision { .. }
        | vil_swarm::AgentLoopEvent::ReasoningTransition { .. }
        | vil_swarm::AgentLoopEvent::AssistantMessage { .. }
        | vil_swarm::AgentLoopEvent::LlmRequest { .. } => None,
    }
}

async fn persist_approval_request(
    approval_store: &ApprovalStore,
    tool_call_id: String,
    tool_name: String,
    arguments: serde_json::Value,
    explanation: Option<String>,
    session_uuid: uuid::Uuid,
    task_uuid: uuid::Uuid,
) {
    let store = approval_store.clone();
    if let Err(e) = tokio::task::spawn_blocking(move || {
        store.record_request(
            tool_call_id,
            tool_name,
            arguments,
            explanation,
            Some(session_uuid),
            Some(task_uuid),
        )
    })
    .await
    .map_err(|e| VacError::Task(format!("Approval store task failed: {}", e)))
    .and_then(|r| r.map_err(|e| VacError::Task(format!("Approval store error: {}", e))))
    {
        warn!(error = %e, "Failed to persist approval request");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InspectorUI {
    pub active_tools: std::collections::HashMap<uuid::Uuid, Vec<String>>,
    pub shell_sessions: std::collections::HashMap<uuid::Uuid, Vec<String>>,
    pub blockers: std::collections::HashMap<uuid::Uuid, Vec<String>>,
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
    pub inspector: InspectorUI,
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
        success: bool,
        content: String,
        envelope: Option<vac_tool_core::ToolResultEnvelope>,
    },
    ValidationResult {
        score: f64,
        issues: Vec<String>,
    },
    LspStatus {
        available: bool,
        binary_path: String,
    },
    LspDiagnostics(crate::lsp::types::LspWorkspaceSnapshot),
    Completed(TaskResult),
    Failed(String),
    Cancelled,
    ApprovalRequired {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        explanation: Option<String>,
    },
    SpeculationReady {
        predicted_prompt: String,
        precomputed_context: std::collections::HashMap<String, String>,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::LlmProviderConfig;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_env() {
        // SAFETY: ENV_LOCK serializes env mutation within this test module.
        unsafe {
            for key in [
                "ANTHROPIC_API_KEY",
                "OPENAI_API_KEY",
                "GEMINI_API_KEY",
                "XAI_API_KEY",
                "MISTRAL_API_KEY",
                "OPENAI_COMPAT_API_KEY",
                "OPENAI_COMPAT_BASE_URL",
            ] {
                std::env::remove_var(key);
            }
        }
    }

    fn write_config(project_root: &std::path::Path, config: &VacConfig) {
        let vac_dir = project_root.join(".vac");
        std::fs::create_dir_all(&vac_dir).expect("create .vac");
        let rendered = toml::to_string(config).expect("serialize config");
        std::fs::write(vac_dir.join("config.toml"), rendered).expect("write config");
    }

    #[tokio::test]
    async fn test_engine_injects_privacy_vault() {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path().to_path_buf();

        // Simulate the bootstrap contract that `vac init` provides.
        write_config(&project_root, &VacConfig::default());
        std::fs::create_dir_all(project_root.join(".vac/memory")).unwrap();

        let mut engine = VacEngine::new(project_root.clone()).await.unwrap();

        // Disable things that require real setup
        engine.config.vil_lsp.enable = false;
        engine.config.trace.enable = false;

        engine.init().await.unwrap();

        let swarm_arc = engine.swarm.clone().expect("Swarm should be initialized");
        let swarm = swarm_arc.read().await;

        assert!(
            Arc::ptr_eq(&engine.privacy_vault, &swarm.privacy_vault),
            "Engine did not inject its shared privacy vault to the SwarmOrchestrator"
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    // ENV_LOCK is a std::sync::Mutex used purely to serialize env-var mutations
    // across tests in this module; the awaits inside never re-enter env code.
    async fn llm_router_requires_restart_for_config_file_swaps() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();

        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "anthropic-test");
        }

        let anthropic_dir = tempfile::tempdir().unwrap();
        let mut anthropic_cfg = VacConfig::default();
        anthropic_cfg.llm.default_provider = "anthropic".to_string();
        anthropic_cfg.llm.budget_tokens = 4096;
        anthropic_cfg
            .llm
            .providers
            .retain(|name, _| name == "anthropic");
        anthropic_cfg
            .llm
            .providers
            .get_mut("anthropic")
            .expect("anthropic provider")
            .model = Some("claude-sonnet-4".to_string());
        write_config(anthropic_dir.path(), &anthropic_cfg);

        let anthropic_engine = VacEngine::new(anthropic_dir.path().to_path_buf())
            .await
            .unwrap();
        let anthropic_router = anthropic_engine.build_llm_router();
        assert_eq!(anthropic_router.default_provider(), "anthropic");
        assert!(anthropic_router.has_provider("anthropic"));

        let compat_dir = tempfile::tempdir().unwrap();
        let mut compat_cfg = VacConfig::default();
        compat_cfg.llm.default_provider = "openai_compat".to_string();
        compat_cfg.llm.budget_tokens = 4096;
        compat_cfg.llm.providers.insert(
            "openai_compat".to_string(),
            LlmProviderConfig {
                api_key_env: None,
                model: None,
                base_url: None,
                ..Default::default()
            },
        );
        compat_cfg
            .llm
            .providers
            .retain(|name, _| name == "openai_compat");
        compat_cfg
            .llm
            .providers
            .get_mut("openai_compat")
            .expect("openai_compat provider")
            .base_url = Some("http://127.0.0.1:11434/v1".to_string());
        compat_cfg
            .llm
            .providers
            .get_mut("openai_compat")
            .expect("openai_compat provider")
            .model = Some("llama3.1".to_string());
        write_config(compat_dir.path(), &compat_cfg);

        let compat_engine = VacEngine::new(compat_dir.path().to_path_buf())
            .await
            .unwrap();
        let compat_router = compat_engine.build_llm_router();
        assert_eq!(compat_router.default_provider(), "openai_compat");
        assert!(compat_router.has_provider("openai_compat"));

        clear_env();
    }
}
