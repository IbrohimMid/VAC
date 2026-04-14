//! Swarm Orchestrator — manages agent lifecycle and task routing.

use crate::agent::*;
use crate::error::{SwarmError, SwarmResult};
use crate::semantic::{PlannerGateResult, evaluate_planner_output};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};
use vac_tools::registry::ToolContext;
use vac_tools::router::ToolRouter;
use vil_llm::LlmRouter;
use vil_llm::provider::{LlmRequest, Message, ToolDefinition};

pub struct ExecutionResult {
    pub summary: String,
    pub modified_files: Vec<String>,
    pub created_files: Vec<String>,
    pub total_tokens_used: u64,
    pub agent_contributions: Vec<AgentContribution>,
}

pub struct AgentContribution {
    pub agent_id: String,
    pub agent_role: String,
    pub actions: Vec<String>,
    pub tokens_used: u64,
}

#[derive(Debug, Clone)]
pub enum AgentLoopEvent {
    Status(String),
    LlmRequest {
        provider: String,
        model: String,
        message_count: usize,
    },
    ModelResponse {
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
    ApprovalRequired {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
}

/// Response from the TUI/host to a pending approval request.
#[derive(Debug, Clone)]
pub struct ApprovalResponse {
    pub tool_call_id: String,
    pub approved: bool,
    pub reason: Option<String>,
}

pub struct TaskPlan {
    pub subtasks: Vec<SubTask>,
    pub execution_order: Vec<usize>,
    pub can_parallelize: Vec<Vec<usize>>,
}

pub struct SubTask {
    pub description: String,
    pub assigned_role: AgentRole,
    pub dependencies: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct SubtaskResult {
    pub role: AgentRole,
    pub summary: String,
    pub modified_files: Vec<String>,
    pub created_files: Vec<String>,
    pub tokens_used: u64,
    pub success: bool,
    pub error: Option<String>,
}

pub struct SwarmOrchestrator {
    agents: HashMap<AgentId, AgentDefinition>,
    #[allow(dead_code)]
    max_concurrent: usize,
    #[allow(dead_code)]
    enable_parallel: bool,
    llm_router: Option<Arc<LlmRouter>>,
    tool_router: Option<Arc<ToolRouter>>,
    /// VIL project profile — drives archetype-aware system prompts
    pub project_profile: Option<vac_core_types::VilProjectProfile>,
    /// Loaded knowledge base — injected into planner/coder prompts
    pub knowledge: Option<Arc<vil_knowledge::KnowledgeBase>>,
    /// Rulebook overlay — team/repo constraints appended after VIL knowledge (never before)
    pub rulebook: Option<String>,
    /// Sandbox registry for subagent lifecycle management
    sandbox_registry: Arc<crate::sandbox::SandboxRegistry>,
    /// External LSP diagnostic context injected before agent loop
    lsp_context: Option<ExternalDiagnosticContext>,
    /// Optional hook for tool call interception
    pub hook: Option<Arc<dyn crate::hooks::AgentHook>>,
    /// Shared privacy vault for redacting sensitive information
    pub privacy_vault: Arc<tokio::sync::RwLock<vac_tools::PrivacyVault>>,
}

/// Lightweight diagnostic context from vil-lsp, decoupled from vac_core types.
#[derive(Debug, Clone, Default)]
pub struct ExternalDiagnosticContext {
    pub total_errors: usize,
    pub total_warnings: usize,
    pub top_findings: Vec<String>,
}

impl ExternalDiagnosticContext {
    pub fn is_empty(&self) -> bool {
        self.total_errors == 0 && self.total_warnings == 0
    }
}

/// Minimal re-export types needed from vac_core to avoid circular deps.
/// SwarmOrchestrator only needs VilProjectProfile + VilArchetype.
pub mod vac_core_types {
    pub use super::VilProjectProfile;
    pub use super::VilArchetype;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VilProjectProfile {
    pub archetype: VilArchetype,
    pub vil_deps: Vec<String>,
    pub detected_constructs: Vec<String>,
    pub is_vil_project: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VilArchetype {
    Server,
    Pipeline,
    Plugin,
    Hybrid(Vec<VilArchetype>),
    Unknown,
}

impl std::fmt::Display for VilArchetype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Server => write!(f, "VilServer"),
            Self::Pipeline => write!(f, "Pipeline (SDK)"),
            Self::Plugin => write!(f, "Plugin"),
            Self::Hybrid(parts) => {
                let names: Vec<_> = parts.iter().map(|p| format!("{p}")).collect();
                write!(f, "Hybrid({})", names.join("+"))
            }
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl SwarmOrchestrator {
    pub async fn new(
        max_concurrent: usize,
        enable_parallel: bool,
        llm_router: Option<Arc<LlmRouter>>,
        tool_router: Option<Arc<ToolRouter>>,
        privacy_vault: Arc<tokio::sync::RwLock<vac_tools::PrivacyVault>>,
    ) -> SwarmResult<Self> {
        let mut orchestrator = Self {
            agents: HashMap::new(),
            max_concurrent,
            enable_parallel,
            llm_router,
            tool_router,
            project_profile: None,
            knowledge: None,
            rulebook: None,
            sandbox_registry: Arc::new(crate::sandbox::SandboxRegistry::with_project_root(std::path::Path::new("."))),
            lsp_context: None,
            hook: None,
            privacy_vault,
        };

        let roles = [
            AgentRole::Architect,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Security,
            AgentRole::Deploy,
            AgentRole::Monitor,
            AgentRole::Optimizer,
            AgentRole::Documenter,
        ];

        for role in roles {
            let agent = AgentDefinition::new(role);
            info!(agent = %agent.name, role = ?role, "Registered agent");
            orchestrator.agents.insert(agent.id, agent);
        }

        Ok(orchestrator)
    }

    /// Set the VIL project profile for archetype-aware prompt injection.
    pub fn set_project_profile(&mut self, profile: VilProjectProfile) {
        self.project_profile = Some(profile);
    }

    /// Set the knowledge base for pattern injection into prompts.
    pub fn set_knowledge(&mut self, kb: vil_knowledge::KnowledgeBase) {
        self.knowledge = Some(Arc::new(kb));
    }

    /// Set rulebook overlay (formatted prompt string). Appended after VIL knowledge, never before.
    pub fn set_rulebook(&mut self, overlay: String) {
        self.rulebook = Some(overlay);
    }

    /// Set LSP diagnostic context to inject into agent prompts.
    pub fn set_lsp_prompt_context(&mut self, ctx: ExternalDiagnosticContext) {
        self.lsp_context = Some(ctx);
    }

    /// Spawn a subtask in a sandboxed environment.
    /// The subagent's ToolContext will have agent_zone = SandboxedSubagent,
    /// restricting it to read-only tools and denying needs-approval tools.
    pub async fn spawn_subtask_sandboxed(
        &self,
        role: AgentRole,
        task_description: &str,
        spec: crate::sandbox::SandboxSpec,
    ) -> SwarmResult<SubtaskResult> {
        info!(role = ?role, mode = ?spec.mode, "Spawning sandboxed subtask");

        let llm_router = self.llm_router.as_ref()
            .ok_or_else(|| SwarmError::Orchestration("LLM router not initialized".into()))?;
        let tool_router = self.tool_router.as_ref()
            .ok_or_else(|| SwarmError::Orchestration("Tool router not initialized".into()))?;

        let sandbox = self.sandbox_registry.spawn(&spec, task_description).await;
        let sandbox_id = sandbox.id;

        let messages = crate::subagent::build_subagent_messages(&role, task_description);
        let tool_defs = crate::subagent::build_tool_defs(tool_router.registry()).await;
        let context = crate::subagent::build_sandbox_context(sandbox.overlay_dir.clone());

        let mut state = crate::run_state::AgentRunState::new(messages, None);

        let result = self.execute_agent_loop(
            &mut state, tool_defs, None,
            &context, llm_router, tool_router,
        ).await;

        match result {
            Ok(summary) => {
                let patch = self.sandbox_registry.build_patch(sandbox_id).await;
                self.sandbox_registry.complete(sandbox_id, patch.clone()).await;
                Ok(SubtaskResult {
                    role,
                    summary,
                    modified_files: patch.as_ref().map(|p| p.modified_files.clone()).unwrap_or(state.modified_files),
                    created_files: patch.as_ref().map(|p| p.created_files.clone()).unwrap_or(state.created_files),
                    tokens_used: state.total_tokens,
                    success: true,
                    error: None,
                })
            }
            Err(e) => {
                self.sandbox_registry.fail(sandbox_id, e.to_string()).await;
                Ok(SubtaskResult { role, summary: String::new(), modified_files: state.modified_files, created_files: state.created_files, tokens_used: state.total_tokens, success: false, error: Some(e.to_string()) })
            }
        }
    }

    pub async fn spawn_subtask(
        &self,
        role: AgentRole,
        task_description: &str,
        _wait_for_completion: bool,
    ) -> SwarmResult<SubtaskResult> {
        info!(role = ?role, "Spawning subtask");

        let llm_router = self.llm_router.as_ref()
            .ok_or_else(|| SwarmError::Orchestration("LLM router not initialized".into()))?;
        let tool_router = self.tool_router.as_ref()
            .ok_or_else(|| SwarmError::Orchestration("Tool router not initialized".into()))?;

        // Spawn ephemeral sandbox for subtask isolation
        let spec = crate::sandbox::SandboxSpec {
            mode: crate::sandbox::SandboxMode::Ephemeral,
            working_dir: std::path::PathBuf::from("."),
            ..Default::default()
        };
        let sandbox = self.sandbox_registry.spawn(&spec, task_description).await;
        let sandbox_id = sandbox.id;

        let messages = crate::subagent::build_subagent_messages(&role, task_description);
        let tool_defs = crate::subagent::build_tool_defs(tool_router.registry()).await;
        let context = crate::subagent::build_parent_context(std::path::PathBuf::from("."));
        let mut state = crate::run_state::AgentRunState::new(messages, None);

        let result = self.execute_agent_loop(
            &mut state,
            tool_defs,
            None,
            &context,
            llm_router,
            tool_router,
        ).await;

        match result {
            Ok(summary) => {
                self.sandbox_registry.complete(sandbox_id, None).await;
                Ok(SubtaskResult {
                    role,
                    summary,
                    modified_files: state.modified_files,
                    created_files: state.created_files,
                    tokens_used: state.total_tokens,
                    success: true,
                    error: None,
                })
            }
            Err(e) => {
                self.sandbox_registry.fail(sandbox_id, e.to_string()).await;
                Ok(SubtaskResult {
                    role,
                    summary: String::new(),
                    modified_files: state.modified_files,
                    created_files: state.created_files,
                    tokens_used: state.total_tokens,
                    success: false,
                    error: Some(e.to_string()),
                })
            }
        }
    }

    fn semantic_planner_prompt() -> String {
        "You are the VIL Semantic Planner. Your task is to analyze the user's request and produce a Semantic Plan before any code is written.

IMPORTANT: For tasks related to VIL (Vastar Intermediate Language), you MUST use the `vil_knowledge` tool first to find relevant patterns. Record the pattern names you consulted in `knowledge_refs`.

Once you have consulted the knowledge base (if needed) and analyzed the task, you MUST produce a JSON plan in exactly this format and wrap it in ```json ... ```:
```json
{
  \"kind\": \"VilServer\" | \"SdkPipeline\" | \"Plugin\" | \"Sidecar\" | \"Wasm\" | \"Connector\" | \"SemanticMessageLayer\" | \"GenericRust\" | \"Unknown\",
  \"semantic_roles\": [\"vil_state\", \"vil_event\", \"vil_fault\", \"vil_decision\", \"generic\"],
  \"lanes\": [\"Trigger\", \"Data\", \"Control\"],
  \"zero_copy_expected\": true,
  \"generated_plumbing_expected\": true,
  \"forbidden_constructs\": [\"Json<T>\", \"Extension<T>\"],
  \"required_patterns\": [\"vx_app_handler\"],
  \"knowledge_refs\": [\"vx_app_handler\", \"vil_response\"],
  \"rationale\": \"Why this architecture was chosen\"
}
```
Rules:
- `knowledge_refs` MUST list every VIL pattern name you looked up via `vil_knowledge`. Leave empty only for GenericRust/Unknown tasks.
- Only use `GenericRust` or `Unknown` if the task is completely unrelated to VIL concepts.
- Always validate your choices against VIL's Tri-Lane and zero-copy semantics.".to_string()
    }

    fn coder_system_prompt() -> String {
        // Fallback when no project profile is available
        "You are a VIL-native Coder agent. You do NOT write generic Rust/Axum code on VIL paths.\n\
        IMPORTANT: Call `vil_knowledge` FIRST for any VIL-related task to get the correct pattern.\n\
        Forbidden on VIL paths: Json<T>, Extension<T>, Json(data) responses.\n\
        Use instead: ShmSlice, ServiceCtx, VilResponse::ok(data).\n\
        Canonical VIL terms: use `vil-expr` (not `v-cel`), `Rule` (not `VRule`), `VilServer` (not `VxApp`).\n\
        Legacy aliases (`v-cel`, `VRule`, `VxApp`) are accepted as input only. Never emit them in new artifacts.".to_string()
    }

    /// Build a VIL-native coder system prompt enriched with archetype context and knowledge patterns.
    pub fn build_vil_coder_prompt_pub(
        profile: &VilProjectProfile,
        knowledge: Option<&vil_knowledge::KnowledgeBase>,
    ) -> String {
        Self::build_vil_coder_prompt(profile, knowledge)
    }

    /// Build a VIL-native coder system prompt enriched with archetype context and knowledge patterns.
    fn build_vil_coder_prompt(
        profile: &VilProjectProfile,
        knowledge: Option<&vil_knowledge::KnowledgeBase>,
    ) -> String {
        let archetype_context = match &profile.archetype {
            VilArchetype::Server =>
                "This is a **VilServer** project.\n\
                - Handlers: `#[vil_handler(shm)] async fn h(ctx: ServiceCtx, slice: ShmSlice) -> VilResponse<T>`\n\
                - State: `ctx.state::<T>()` NOT `Extension<T>`\n\
                - Body: `ShmSlice` + `vil_json::from_slice()` NOT `Json<T>`\n\
                - Response: `VilResponse::ok(data)` NOT `Json(data)`\n\
                - Semantic types: `#[vil_state]`, `#[vil_event]`, `#[vil_fault]`, `#[vil_decision]`",
            VilArchetype::Pipeline =>
                "This is a **SDK_PIPELINE** project.\n\
                - Macro: `vil_workflow! { name, token: ShmToken, instances: [...], routes: [...] }`\n\
                - Sources: `HttpSourceBuilder::new().url().format(HttpFormat::SSE).dialect(...)`\n\
                - Token: `ShmToken` for high-throughput, `GenericToken` for simple cases\n\
                - Routes: `sink.out -> source.in (LoanWrite)`",
            VilArchetype::Plugin =>
                "This is a **VilPlugin** project.\n\
                - Trait: `impl VilPlugin for T { fn register(&self, ctx: &mut PluginContext) }`\n\
                - Registration: `ctx.state(...)`, `ctx.endpoint(...)`, `ctx.middleware(...)`",
            VilArchetype::Hybrid(_) =>
                "This is a **Hybrid VIL** project. Apply the correct pattern per component:\n\
                - Server: ShmSlice + ServiceCtx + VilResponse\n\
                - Pipeline: vil_workflow! + ShmToken\n\
                - Plugin: VilPlugin + PluginContext",
            VilArchetype::Unknown =>
                "VIL project type not detected. Use vil_knowledge to identify the correct pattern.",
        };

        let pattern_context = if let Some(kb) = knowledge {
            let categories: &[&str] = match &profile.archetype {
                VilArchetype::Server => &["server", "patterns"],
                VilArchetype::Pipeline => &["pipeline", "patterns"],
                VilArchetype::Plugin => &["plugin"],
                VilArchetype::Hybrid(_) => &["server", "pipeline", "plugin"],
                VilArchetype::Unknown => &[],
            };
            let patterns: Vec<String> = categories
                .iter()
                .flat_map(|cat| kb.patterns_by_category(cat))
                .take(3)
                .map(|p| format!("- **{}**: {}", p.name, p.description))
                .collect();
            if patterns.is_empty() { String::new() }
            else { format!("\n\n**Pre-loaded VIL patterns:**\n{}", patterns.join("\n")) }
        } else {
            String::new()
        };

        format!(
            "You are a VIL-native Coder agent. You do NOT write generic Rust/Axum code on VIL paths.\n\n\
            {archetype_context}{pattern_context}\n\n\
            **Forbidden on VIL paths:** `Json<T>`, `Extension<T>`, `Json(data)`, manual queue/metrics plumbing\n\n\
            **Canonical VIL terms:** use `vil-expr` (not `v-cel`), `Rule` (not `VRule`), `VilServer` (not `VxApp`). \
            Legacy aliases are accepted as input only — never emit them in new artifacts.\n\n\
            **Workflow:** call `vil_knowledge` FIRST → read codebase → implement VIL-native → verify → report"
        )
    }

    pub async fn agent_loop(&mut self, task_description: &str) -> SwarmResult<ExecutionResult> {
        self.agent_loop_with_events(task_description, None).await
    }

    pub async fn execute_agent_loop(
        &self,
        state: &mut crate::run_state::AgentRunState,
        tool_defs: Vec<ToolDefinition>,
        updates: Option<mpsc::UnboundedSender<AgentLoopEvent>>,
        context: &ToolContext,
        llm_router: &Arc<vil_llm::LlmRouter>,
        tool_router: &Arc<vac_tools::router::ToolRouter>,
    ) -> SwarmResult<String> {
        self.execute_agent_loop_with_approvals(state, tool_defs, updates, context, llm_router, tool_router, None).await
    }

    pub async fn execute_agent_loop_with_approvals(
        &self,
        state: &mut crate::run_state::AgentRunState,
        tool_defs: Vec<ToolDefinition>,
        updates: Option<mpsc::UnboundedSender<AgentLoopEvent>>,
        context: &ToolContext,
        llm_router: &Arc<vil_llm::LlmRouter>,
        tool_router: &Arc<vac_tools::router::ToolRouter>,
        mut approval_rx: Option<mpsc::UnboundedReceiver<ApprovalResponse>>,
    ) -> SwarmResult<String> {
        loop {
            // Step 1: Cancellation check
            if state.is_cancelled() {
                state.stage = crate::run_state::RunStage::Cancelled;
                return Err(SwarmError::Cancelled);
            }
            // Step 2: Iteration cap check
            if let Err(e) = crate::loop_control::check_iteration_cap(state.iterations) {
                warn!(iterations = state.iterations, "Max iterations reached, terminating agent loop");
                return Err(e);
            }
            state.iterations += 1;
            state.collector.push(crate::events::AgentEvent::IterationStarted { iteration: state.iterations });
            // Step 3: Context reduction
            let prev_boundary = state.trim_boundary;
            let reduced = crate::context_budget::reduce_messages(
                state.messages.clone(),
                &mut state.trim_boundary,
                &mut state.trim_store,
            );
            if state.trim_boundary > prev_boundary {
                state.collector.context_reduced(state.messages.len(), reduced.len(), state.trim_boundary);
            }
            // Step 4: Build LLM request using LlmMessage canonical path
            let llm_messages: Vec<vil_llm::models::LlmMessage> = reduced.iter()
                .map(vil_llm::models::LlmMessage::from)
                .collect();
            let request = LlmRequest::from_llm_messages(llm_messages)
                .with_max_tokens(4000)
                .with_tools(tool_defs.clone());

            if let Some(tx) = &updates {
                let _ = tx.send(AgentLoopEvent::Status("Thinking".to_string()));
                let _ = tx.send(AgentLoopEvent::LlmRequest {
                    provider: "kilo".to_string(),
                    model: "default".to_string(),
                    message_count: reduced.len(),
                });
            }

            // Step 5: Stream processing
            let rx = llm_router
                .stream(&request)
                .await
                .map_err(|e| SwarmError::Orchestration(format!("LLM error: {}", e)))?;

            let stream_result = crate::stream_processor::process_stream(rx, &updates).await?;
            let response = stream_result.response;
            state.total_tokens += response.usage.total_tokens;

            if let Some(tx) = &updates {
                let _ = tx.send(AgentLoopEvent::ModelResponse {
                    provider: "kilo".to_string(),
                    model: response.model.clone(),
                });
            }

            match response.finish_reason {
                vil_llm::provider::FinishReason::Stop => {
                    if let Some(tx) = &updates {
                        let _ =
                            tx.send(AgentLoopEvent::Status("Preparing final answer".to_string()));
                    }
                    info!("Agent loop completed successfully (Stop reason)");
                    state.collector.push(crate::events::AgentEvent::LoopCompleted {
                        total_iterations: state.iterations,
                        total_tokens: state.total_tokens,
                    });
                    state.stage = crate::run_state::RunStage::Completed;
                    return Ok(response.content);
                }
                vil_llm::provider::FinishReason::ToolUse => {
                    info!(count = response.tool_calls.len(), "Received tool calls");
                    state.messages.push(Message::assistant_with_tool_calls(
                        response.content.clone(),
                        response.tool_calls.clone(),
                    ));

                    // Classify tools: Data Lane (parallel reads) / Control Lane (serial writes)
                    let (parallel_reads, serial_writes) = crate::tool_execution::partition_calls(response.tool_calls);

                    // Step 6: Tool execution via tool_executor module
                    let all_calls: Vec<_> = parallel_reads.into_iter().chain(serial_writes).collect();
                    
                    // Emit ToolCall events
                    if let Some(tx) = &updates {
                        for call in &all_calls {
                            let _ = tx.send(AgentLoopEvent::ToolCall {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            });
                        }
                    }

                    crate::tool_executor::execute_tools(all_calls, state, context, tool_router, &updates, self.hook.as_deref()).await?;

                    // Step 7: Wait for approval responses if any tools require approval
                    if !state.pending_approvals.is_empty() {
                        if let Some(ref mut rx) = approval_rx {
                            info!(count = state.pending_approvals.len(), "Waiting for approval responses");
                            let approval_timeout = std::time::Duration::from_secs(300); // 5 min max wait
                            while !state.pending_approvals.is_empty() {
                                let recv_future = rx.recv();
                                match tokio::time::timeout(approval_timeout, recv_future).await {
                                    Ok(Some(response)) => {
                                        let pos = state.pending_approvals.iter().position(|p| p.tool_call_id == response.tool_call_id);
                                        if let Some(idx) = pos {
                                            let pending = state.pending_approvals.remove(idx);
                                            if response.approved {
                                                state.approved_tools.insert(response.tool_call_id.clone());
                                                info!(tool = %pending.tool_name, "Tool approved, re-executing");
                                                let approved_call = vil_llm::provider::ToolCall {
                                                    id: pending.tool_call_id.clone(),
                                                    name: pending.tool_name.clone(),
                                                    arguments: pending.arguments.clone(),
                                                };
                                                crate::tool_executor::execute_tools(
                                                    vec![approved_call], state, context, tool_router, &updates, self.hook.as_deref()
                                                ).await?;
                                            } else {
                                                let reason = response.reason.unwrap_or_else(|| "User rejected".to_string());
                                                info!(tool = %pending.tool_name, reason = %reason, "Tool rejected by user");
                                                state.messages.push(Message::tool(
                                                    pending.tool_name,
                                                    pending.tool_call_id,
                                                    format!("Rejected: {reason}"),
                                                ));
                                            }
                                        } else {
                                            warn!(tool_call_id = %response.tool_call_id, "Received approval for unknown tool call, ignoring");
                                        }
                                    }
                                    Ok(None) => {
                                        warn!("Approval channel closed while waiting for responses");
                                        // Reject all remaining pending approvals
                                        for pending in state.pending_approvals.drain(..) {
                                            state.messages.push(Message::tool(
                                                pending.tool_name,
                                                pending.tool_call_id,
                                                "Rejected: approval channel closed".to_string(),
                                            ));
                                        }
                                        break;
                                    }
                                    Err(_) => {
                                        warn!("Approval wait timeout, rejecting remaining pending approvals");
                                        for pending in state.pending_approvals.drain(..) {
                                            state.messages.push(Message::tool(
                                                pending.tool_name,
                                                pending.tool_call_id,
                                                "Rejected: approval timeout".to_string(),
                                            ));
                                        }
                                        break;
                                    }
                                }
                            }
                        } else {
                            // No approval channel — auto-reject all pending
                            warn!("Pending approvals but no approval channel, auto-rejecting");
                            for pending in state.pending_approvals.drain(..) {
                                state.messages.push(Message::tool(
                                    pending.tool_name,
                                    pending.tool_call_id,
                                    "Rejected: no approval channel configured".to_string(),
                                ));
                            }
                        }
                    }

                    if let Some(tx) = &updates {
                        let _ = tx.send(AgentLoopEvent::Status("Reviewing tool results".to_string()));
                    }
                }
                vil_llm::provider::FinishReason::MaxTokens => {
                    warn!("Context window limit reached, applying emergency context reduction");
                    state.trim_boundary = crate::loop_control::emergency_trim_boundary(state.messages.len(), state.trim_boundary);
                    info!(trim_boundary = state.trim_boundary, "Context budget emergency: trim_boundary advanced");
                }
                _ => {
                    warn!(reason = ?response.finish_reason, "Unknown finish reason, terminating loop");
                    return Ok(response.content);
                }
            }
        }
    }

    pub async fn agent_loop_with_events(
        &mut self,
        task_description: &str,
        updates: Option<mpsc::UnboundedSender<AgentLoopEvent>>,
    ) -> SwarmResult<ExecutionResult> {
        self.agent_loop_with_context(task_description, updates, None, None, None).await
    }

    pub async fn agent_loop_with_context(
        &mut self,
        task_description: &str,
        updates: Option<mpsc::UnboundedSender<AgentLoopEvent>>,
        session_id: Option<uuid::Uuid>,
        project_root: Option<std::path::PathBuf>,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> SwarmResult<ExecutionResult> {
        self.agent_loop_with_full_context(task_description, updates, session_id, project_root, cancel, None).await
    }

    pub async fn agent_loop_with_full_context(
        &mut self,
        task_description: &str,
        updates: Option<mpsc::UnboundedSender<AgentLoopEvent>>,
        session_id: Option<uuid::Uuid>,
        project_root: Option<std::path::PathBuf>,
        cancel: Option<tokio_util::sync::CancellationToken>,
        approval_rx: Option<mpsc::UnboundedReceiver<ApprovalResponse>>,
    ) -> SwarmResult<ExecutionResult> {
        info!(task = %task_description, "Starting Semantic VIL-native agent loop");

        let llm_router = self
            .llm_router
            .as_ref()
            .ok_or_else(|| SwarmError::Orchestration("LLM router not initialized".into()))?;
        let tool_router = self
            .tool_router
            .as_ref()
            .ok_or_else(|| SwarmError::Orchestration("Tool router not initialized".into()))?;

        let root = project_root.unwrap_or_else(|| std::path::PathBuf::from("."));
        let mut context = ToolContext::new(root);
        context.privacy = self.privacy_vault.clone();
        if let Some(sid) = session_id {
            context = context.with_session_id(sid);
        }

        let tool_defs: Vec<ToolDefinition> = tool_router
            .registry()
            .list()
            .await
            .into_iter()
            .map(|tool| ToolDefinition {
                name: tool.name,
                description: tool.description,
                input_schema: tool.input_schema,
            })
            .collect();

        // STAGE 1: SEMANTIC PLANNER
        let planner_messages = vec![
            Message::system(Self::semantic_planner_prompt()),
            Message::user(task_description.to_string()),
        ];

        let mut state = crate::run_state::AgentRunState::new(planner_messages, cancel.clone());
        state.stage = crate::run_state::RunStage::Planner;

        let plan_output = self
            .execute_agent_loop(
                &mut state,
                tool_defs.clone(), // Planner needs access to vil_knowledge
                updates.clone(),
                &context,
                llm_router,
                tool_router,
            )
            .await?;
        // strict_mode: if planner_gate is active (strict-vil profile), ParseFailed = hard stop
        let strict_mode = std::env::var("VAC_PROFILE")
            .map(|p| p == "strict-vil" || p == "spec-hardening")
            .unwrap_or(false);

        let plan_context = match evaluate_planner_output(&plan_output) {
            PlannerGateResult::Passed(plan) => {
                info!(kind = ?plan.kind, knowledge_refs = ?plan.knowledge_refs, "SemanticPlan passed gate");
                plan.to_markdown()
            }
            PlannerGateResult::KnowledgeGateFailed(plan) if strict_mode => {
                return Err(SwarmError::Orchestration(format!(
                    "Planner gate FAILED (strict-vil): VIL task '{:?}' produced no knowledge_refs. \
                    Planner must call vil_knowledge before planning.",
                    plan.kind
                )));
            }
            PlannerGateResult::KnowledgeGateFailed(plan) => {
                warn!(kind = ?plan.kind, "SemanticPlan knowledge gate FAILED — injecting remediation");
                format!(
                    "{}\n\n> **WARNING**: Planner did not consult `vil_knowledge`. \
                    Coder MUST call `vil_knowledge` before writing any code.",
                    plan.to_markdown()
                )
            }
            PlannerGateResult::ParseFailed(raw) if strict_mode => {
                return Err(SwarmError::Orchestration(
                    "Planner gate FAILED (strict-vil): planner did not produce valid SemanticPlan JSON. \
                    Cannot proceed without a typed plan in strict mode.".to_string()
                ));
            }
            PlannerGateResult::ParseFailed(raw) => {
                warn!("Failed to parse SemanticPlan — injecting remediation context");
                format!(
                    "### Planner Output (unparsed)\n{}\n\n\
                    > **WARNING**: Planner did not produce a valid SemanticPlan JSON. \
                    Coder MUST call `vil_knowledge` first and follow VIL-native patterns.",
                    raw
                )
            }
        };

        // STAGE 2: CODER — use archetype-aware prompt if profile is available
        let coder_prompt = if let Some(ref profile) = self.project_profile {
            let mut prompt = Self::build_vil_coder_prompt(profile, self.knowledge.as_deref());
            // LSP diagnostics injected AFTER VIL knowledge, BEFORE rulebook
            if let Some(ref lsp) = self.lsp_context {
                if !lsp.is_empty() {
                    prompt.push_str(&format!(
                        "\n\n---\n**vil-lsp diagnostics** ({} errors, {} warnings):\n{}\nFix these before writing new code.",
                        lsp.total_errors,
                        lsp.total_warnings,
                        lsp.top_findings.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n")
                    ));
                }
            }
            // Rulebook overlay appended AFTER VIL knowledge — never overrides VIL semantics
            if let Some(ref rb) = self.rulebook {
                prompt.push_str(rb);
            }
            prompt
        } else {
            Self::coder_system_prompt()
        };

        // Reset state for coder stage — keep tokens/files, replace messages
        let coder_messages = vec![
            Message::system(coder_prompt),
            Message::user(format!("Task: {}\n\n{}", task_description, plan_context)),
        ];
        state.messages = coder_messages;
        state.trim_boundary = 0;
        state.iterations = 0;
        state.stage = crate::run_state::RunStage::Coder;
        state.cancel = cancel;

        let final_output = self
            .execute_agent_loop_with_approvals(
                &mut state,
                tool_defs,
                updates,
                &context,
                llm_router,
                tool_router,
                approval_rx,
            )
            .await?;

        Ok(ExecutionResult {
            summary: final_output,
            modified_files: state.modified_files,
            created_files: state.created_files,
            total_tokens_used: state.total_tokens,
            agent_contributions: vec![AgentContribution {
                agent_id: "vil-native-agent".to_string(),
                agent_role: "SemanticEngineer".to_string(),
                actions: Vec::new(),
                tokens_used: state.total_tokens,
            }],
        })
    }

    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    pub fn agents(&self) -> &HashMap<AgentId, AgentDefinition> {
        &self.agents
    }
}


