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
    vil_lsp: Option<Arc<crate::lsp::service::VilLspService>>,
    pub privacy_vault: Arc<RwLock<vac_tools::PrivacyVault>>,
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
            vil_lsp: None,
            privacy_vault: Arc::new(RwLock::new(vac_tools::PrivacyVault::new())),
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
        let mut swarm = vil_swarm::SwarmOrchestrator::new(
            self.config.swarm.max_concurrent_agents,
            self.config.swarm.enable_parallel,
            Some(llm_router),
            Some(Arc::new(tool_router)),
            self.privacy_vault.clone(),
        )
        .await?;

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
                if profile.is_vil_project { Some(&archetype_str) } else { None },
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
            ).await {
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
                        return Err(VacError::Other(anyhow::anyhow!("vil-lsp startup timed out")));
                    }
                    warn!("vil-lsp startup timed out; continuing without editor diagnostics");
                }
            }
        }

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
        self.run_task_with_cancel(description, updates, None).await
    }

    #[instrument(skip(self, updates, cancel), fields(task_id))]
    pub async fn run_task_with_cancel(
        &mut self,
        description: &str,
        updates: Option<mpsc::UnboundedSender<RuntimeUpdate>>,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> VacResult<TaskResult> {
        self.run_task_with_approvals(description, updates, cancel, None).await
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

        {
            let mut session = self.session.write().await;
            session.tasks.push(task.clone());
        }

        let result = match self.execute_task_pipeline_with_approvals(task, updates.clone(), cancel, approval_rx).await {
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
        let (swarm_tx, mut swarm_rx) = mpsc::unbounded_channel::<vil_swarm::AgentLoopEvent>();
        let session_id = self.session.read().await.id;
        let project_root = self.project_root.clone();
        let privacy_vault = self.privacy_vault.clone();

        // Phase 6: inject LSP diagnostic context into swarm before execution
        if let Some(ref lsp) = self.vil_lsp {
            let ctx = lsp.prompt_context(self.config.vil_lsp.max_prompt_items).await;
            if !ctx.is_empty() {
                swarm.set_lsp_prompt_context(vil_swarm::ExternalDiagnosticContext {
                    total_errors: ctx.total_errors,
                    total_warnings: ctx.total_warnings,
                    top_findings: ctx.top_findings,
                });
            }
        }
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
                        }),
                        vil_swarm::AgentLoopEvent::ApprovalRequired {
                            tool_call_id,
                            tool_name,
                            arguments,
                        } => Some(RuntimeUpdate::ApprovalRequired {
                            tool_call_id,
                            tool_name,
                            arguments,
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
            .agent_loop_with_full_context(&task.description, Some(swarm_tx), Some(session_id), Some(project_root), cancel, approval_rx)
            .await?;

        info!("Phase 3: Validating...");
        let validation_score = if let Some(ir) = &self.ir_pipeline {
            tracing::debug!("Running IR validation on {} modified files", execution.modified_files.len());
            match vil_validate::validate_changes(ir, &execution.modified_files) {
                Ok(report) => {
                    info!(score = report.score, issues_count = report.issues.len(), "Validation completed");
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
                let profile = crate::profile::ProfileOverride::resolve(
                    &crate::profile::ProfileName::from_str(
                        &std::env::var("VAC_PROFILE").unwrap_or_default()
                    )
                );
                if profile.validator_blocks && snapshot.total_errors > 0 {
                    let post_errors = lsp.diagnostics_for_files(&execution.modified_files).await
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
            return Err(VacError::Config(format!("State checkpoint not found at {}", state_path.display())));
        }

        let mut state = vil_swarm::run_state::AgentRunState::from_checkpoint(&state_path)
            .map_err(|e| VacError::Config(format!("Failed to load state: {}", e)))?;

        // Re-hydrate session in memory
        if let Ok(Some(session_disk)) = crate::session::Session::load(&self.project_root, session_id) {
            let mut session = self.session.write().await;
            session.id = session_id;
            session.metadata = session_disk.metadata;
            session.tasks = session_disk.tasks;
            session.results = session_disk.results;
        }

        let mut swarm = self
            .swarm
            .as_ref()
            .ok_or_else(|| VacError::Task("Swarm not initialized. Run `vac init` first.".into()))?
            .write()
            .await;

        let task_desc = state.messages.iter()
            .find(|m| matches!(m.role, vil_llm::provider::Role::User))
            .map(|m| m.content.clone())
            .unwrap_or_else(|| "Resumed task".to_string());

        let task = Task::new(&task_desc);
        let task_id = task.id;

        let trace_handle = self.trace_recorder.clone();
        let update_tx = updates.clone();
        let (swarm_tx, mut swarm_rx) = tokio::sync::mpsc::unbounded_channel::<vil_swarm::AgentLoopEvent>();
        let project_root = self.project_root.clone();
        let privacy_vault = self.privacy_vault.clone();

        tokio::spawn(async move {
            while let Some(event) = swarm_rx.recv().await {
                if let Some(ref recorder) = trace_handle {
                    match recorder.lock() {
                        Ok(mut rec) => {
                            match &event {
                                vil_swarm::AgentLoopEvent::LlmRequest { provider, model, message_count } => {
                                    rec.record_llm_request(provider, model, *message_count);
                                }
                                vil_swarm::AgentLoopEvent::ToolCall { id: _, name, arguments } => {
                                    rec.record_tool_call(name, arguments);
                                }
                                vil_swarm::AgentLoopEvent::ToolResult { id: _, name, content, success } => {
                                    rec.record_tool_result(name, content, *success);
                                }
                                vil_swarm::AgentLoopEvent::ModelResponse { provider, model } => {
                                    rec.record_llm_response(provider, model);
                                }
                                _ => {}
                            }
                            let _ = rec.flush();
                        }
                        Err(_) => {}
                    }
                }
                if let Some(ref tx) = update_tx {
                    let update = match event {
                        vil_swarm::AgentLoopEvent::Status(message) => Some(RuntimeUpdate::Status(message)),
                        vil_swarm::AgentLoopEvent::ModelResponse { provider, model } => Some(RuntimeUpdate::ModelInfo { provider, model }),
                        vil_swarm::AgentLoopEvent::AssistantChunk(chunk) => {
                            let restored = {
                                let vault = privacy_vault.read().await;
                                vault.restore(&chunk)
                            };
                            Some(RuntimeUpdate::AssistantChunk(restored))
                        }
                        vil_swarm::AgentLoopEvent::ToolCall { id, name, arguments } => Some(RuntimeUpdate::ToolCall { id, name, arguments }),
                        vil_swarm::AgentLoopEvent::ToolResult { id, name, content, success } => Some(RuntimeUpdate::ToolResult { id, name, content, success }),
                        vil_swarm::AgentLoopEvent::ApprovalRequired { tool_call_id, tool_name, arguments } => Some(RuntimeUpdate::ApprovalRequired { tool_call_id, tool_name, arguments }),
                        vil_swarm::AgentLoopEvent::LlmRequest { .. } => None,
                    };
                    if let Some(up) = update {
                        let _ = tx.send(up);
                    }
                }
            }
        });

        state.cancel = cancel.clone();

        let result = swarm
            .resume_agent_loop(
                &mut state,
                &task_desc,
                Some(swarm_tx),
                Some(session_id),
                Some(project_root),
                approval_rx,
            )
            .await;

        let task_result = match result {
            Ok(exec_res) => {
                TaskResult {
                    task_id: crate::task::TaskId(uuid::Uuid::new_v4()),
                    status: crate::task::TaskStatus::Completed,
                    summary: exec_res.summary,
                    modified_files: exec_res.modified_files,
                    created_files: exec_res.created_files,
                    total_tokens_used: exec_res.total_tokens_used,
                    agent_contributions: vec![],
                    elapsed_ms: 0,
                    validation_score: None,
                }
            }
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

    /// Load a specific session by ID, swapping the current session in memory.
    /// This ensures transcript, history, and engine state are synchronized.
    pub async fn load_session(&mut self, session_id: uuid::Uuid) -> VacResult<()> {
        let session_path = self.project_root
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
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_injects_privacy_vault() {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        
        // Write minimal config so init won't fail
        std::fs::write(project_root.join("vac.toml"), r#"
[llm]
default_provider = "anthropic"
[llm.providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-3-5-sonnet-20241022"
"#).unwrap();

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
}

