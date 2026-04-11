//! VacEngine — the main entry point for all VAC operations.

use crate::{
    config::VacConfig,
    error::{VacError, VacResult},
    session::Session,
    task::{Task, TaskResult, TaskStatus},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
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
    swarm: Option<vil_swarm::SwarmOrchestrator>,
    tool_router: Option<vac_tools::ToolRouter>,
    llm_router: Option<std::sync::Arc<vil_llm::LlmRouter>>,
    trace_recorder: Option<vac_trace::TraceRecorder>,
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
        info!("Initializing VAC subsystems...");

        info!("Initializing IR pipeline...");
        let ir = vil_ir::IrPipeline::new(&self.project_root)?;
        self.ir_pipeline = Some(ir);

        info!("Initializing context engine...");
        let ctx_config = vil_context::ContextConfig {
            max_context_tokens: 8192,
            chunk_size: 512,
            chunk_overlap: 50,
            shm_path: PathBuf::from("/tmp/vil_context.shm"),
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
        let tools = vac_tools::ToolRouter::with_default_policy(registry.clone());
        self.tool_router = Some(tools);

        info!("Initializing LLM router...");
        let budget_limit = self.config.llm.max_tokens_per_task;
        let llm_router = LlmRouter::new(&self.config.llm.default_provider, budget_limit);
        let llm_router = Arc::new(llm_router);
        self.llm_router = Some(llm_router.clone());

        if self.config.trace.enable {
            info!("Initializing trace recorder...");
            let trace = vac_trace::TraceRecorder::new(
                self.config.trace.output_path.clone(),
                self.config.trace.enable_signing,
            )
            .map_err(|e| VacError::Other(anyhow::anyhow!("Trace error: {}", e)))?;
            self.trace_recorder = Some(trace);
        }

        info!("Initializing swarm orchestrator...");
        let tool_router = vac_tools::ToolRouter::with_default_policy(registry);
        let swarm = vil_swarm::SwarmOrchestrator::new(
            self.config.swarm.max_concurrent_agents,
            self.config.swarm.enable_parallel,
            Some(llm_router),
            Some(Arc::new(tool_router)),
        )
        .await?;
        self.swarm = Some(swarm);

        info!("All VAC subsystems initialized successfully.");
        Ok(())
    }

    /// Execute a task end-to-end.
    #[instrument(skip(self), fields(task_id))]
    pub async fn run_task(&mut self, description: &str) -> VacResult<TaskResult> {
        let task = Task::new(description);
        let task_id = task.id;
        tracing::Span::current().record("task_id", tracing::field::display(task_id.0));

        info!(task = %description, "Starting task execution");

        {
            let mut session = self.session.write().await;
            session.tasks.push(task.clone());
        }

        let result = match self.execute_task_pipeline(task).await {
            Ok(result) => result,
            Err(e) => {
                error!(error = %e, "Task execution failed");
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

    async fn execute_task_pipeline(&mut self, task: Task) -> VacResult<TaskResult> {
        let start = std::time::Instant::now();
        let task_id = task.id;

        let swarm = self
            .swarm
            .as_mut()
            .ok_or_else(|| VacError::Task("Swarm not initialized. Run `vac init` first.".into()))?;
        let _context = self
            .context_engine
            .as_ref()
            .ok_or_else(|| VacError::Task("Context engine not initialized.".into()))?;
        let _tools = self
            .tool_router
            .as_ref()
            .ok_or_else(|| VacError::Task("Tool router not initialized.".into()))?;

        info!("Phase 1: Planning...");
        let plan = swarm.plan_task(&task.description).await?;

        info!("Phase 2: Executing...");
        let execution = swarm.execute_plan(&plan).await?;

        info!("Phase 3: Validating...");
        let validation_score = if let Some(ir) = &self.ir_pipeline {
            Some(vil_validate::validate_changes(
                ir,
                &execution.modified_files,
            )?)
        } else {
            None
        };

        if let Some(recorder) = &mut self.trace_recorder {
            let task_id_str = task_id.0.to_string();
            recorder
                .record_task(&task_id_str, &task.description)
                .map_err(|e| VacError::Other(anyhow::anyhow!("Trace error: {}", e)))?;
        }

        let elapsed = start.elapsed();

        Ok(TaskResult {
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
        })
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
