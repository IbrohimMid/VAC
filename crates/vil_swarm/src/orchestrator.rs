//! Swarm Orchestrator — manages agent lifecycle and task routing.

use crate::agent::*;
use crate::error::{SwarmError, SwarmResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use vac_tools::registry::ToolContext;
use vac_tools::router::ToolRouter;
use vil_llm::LlmRouter;
use vil_llm::provider::{LlmRequest, Message, Role, ToolDefinition};

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

pub struct SwarmOrchestrator {
    agents: HashMap<AgentId, AgentDefinition>,
    #[allow(dead_code)]
    max_concurrent: usize,
    #[allow(dead_code)]
    enable_parallel: bool,
    llm_router: Option<Arc<LlmRouter>>,
    tool_router: Option<Arc<ToolRouter>>,
}

impl SwarmOrchestrator {
    pub async fn new(
        max_concurrent: usize,
        enable_parallel: bool,
        llm_router: Option<Arc<LlmRouter>>,
        tool_router: Option<Arc<ToolRouter>>,
    ) -> SwarmResult<Self> {
        let mut orchestrator = Self {
            agents: HashMap::new(),
            max_concurrent,
            enable_parallel,
            llm_router,
            tool_router,
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

    fn architect_system_prompt() -> String {
        "You are the Architect agent. Your role is to break down complex tasks into subtasks and determine execution order.

Analyze the user's task and create a detailed plan. For each subtask:
- description: What needs to be done
- assigned_role: Which agent should execute it (Architect, Coder, Tester, Security, Deploy, Monitor, Optimizer, Documenter)
- dependencies: Which subtask indices must complete first

Respond with a JSON plan in this format:
{
  \"subtasks\": [
    {\"description\": \"...\", \"assigned_role\": \"Coder\", \"dependencies\": []}
  ],
  \"execution_order\": [0, 1, ...],
  \"can_parallelize\": [[1, 2], ...]
}

Rules:
- Dependencies must form a valid DAG (no cycles)
- Independent tasks can be parallelized
- Coder should come before Tester
- Security review before Deploy
- Documenter last for documentation tasks".to_string()
    }

    fn coder_system_prompt() -> String {
        "You are the Coder agent. Your role is to implement features and fixes based on subtask descriptions.

When given a subtask:
1. Understand what needs to be built
2. Use read/search tools like file_read, glob, grep, and search to inspect the codebase
3. Write or modify code using file_write or file_edit
4. Track progress with todo_write when a task has multiple steps
5. If compilation is needed, use bash, cargo, or git tools as appropriate
6. Report success or any errors encountered

You have access to tools for:
- Reading files: file_read
- Searching files: glob, grep, search
- Writing files: file_write, file_edit
- Running commands: bash, cargo, git
- Task tracking: todo_write, task_done

When you encounter errors:
1. Read the error message carefully
2. Fix the issue in the code
3. Re-run to verify the fix
4. Repeat until successful or report failure".to_string()
    }

    pub async fn agent_loop(&mut self, task_description: &str) -> SwarmResult<ExecutionResult> {
        self.agent_loop_with_events(task_description, None).await
    }

    pub async fn agent_loop_with_events(
        &mut self,
        task_description: &str,
        updates: Option<mpsc::UnboundedSender<AgentLoopEvent>>,
    ) -> SwarmResult<ExecutionResult> {
        info!(task = %task_description, "Starting Claude Code compatible agent loop");

        let llm_router = self
            .llm_router
            .as_ref()
            .ok_or_else(|| SwarmError::Orchestration("LLM router not initialized".into()))?;
        let tool_router = self
            .tool_router
            .as_ref()
            .ok_or_else(|| SwarmError::Orchestration("Tool router not initialized".into()))?;

        let mut total_tokens = 0u64;
        let mut modified_files = Vec::new();
        let mut created_files = Vec::new();
        let context = ToolContext::new(std::path::PathBuf::from("."));

        let mut messages = vec![
            Message::system(Self::coder_system_prompt()),
            Message::user(task_description.to_string()),
        ];

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

        loop {
            let request = LlmRequest::new(messages.clone())
                .with_max_tokens(4000)
                .with_tools(tool_defs.clone());

            if let Some(tx) = &updates {
                let _ = tx.send(AgentLoopEvent::Status("Thinking".to_string()));
                let _ = tx.send(AgentLoopEvent::LlmRequest {
                    provider: "kilo".to_string(), // In future this can be dynamic
                    model: "default".to_string(),
                    message_count: messages.len(),
                });
            }

            let response = llm_router
                .complete(&request)
                .await
                .map_err(|e| SwarmError::Orchestration(format!("LLM error: {}", e)))?;

            total_tokens += response.usage.total_tokens;

            if let Some(tx) = &updates {
                let _ = tx.send(AgentLoopEvent::ModelResponse {
                    provider: "kilo".to_string(),
                    model: response.model.clone(),
                });
            }

            if !response.content.is_empty() {
                if let Some(tx) = &updates {
                    for ch in response.content.chars() {
                        let _ = tx.send(AgentLoopEvent::AssistantChunk(ch.to_string()));
                    }
                }
            }

            match response.finish_reason {
                vil_llm::provider::FinishReason::Stop => {
                    if let Some(tx) = &updates {
                        let _ =
                            tx.send(AgentLoopEvent::Status("Preparing final answer".to_string()));
                    }
                    info!("Agent loop completed successfully (Stop reason)");
                    break;
                }
                vil_llm::provider::FinishReason::ToolUse => {
                    info!(count = response.tool_calls.len(), "Received tool calls");
                    messages.push(Message::assistant_with_tool_calls(
                        response.content.clone(),
                        response.tool_calls.clone(),
                    ));

                    // Classify tools: Data Lane (parallel reads) / Control Lane (serial writes)
                    let mut parallel_reads = Vec::new();
                    let mut serial_writes = Vec::new();

                    for call in response.tool_calls {
                        if let Some(tx) = &updates {
                            let _ = tx.send(AgentLoopEvent::ToolCall {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            });
                        }
                        match call.name.as_str() {
                            "file_read" | "glob" | "grep" | "search" => parallel_reads.push(call),
                            _ => serial_writes.push(call),
                        }
                    }

                    // Execute parallel read operations first (Data Lane)
                    if !parallel_reads.is_empty() {
                        if let Some(tx) = &updates {
                            let _ = tx.send(AgentLoopEvent::Status(
                                "Reading files and searching the workspace".to_string(),
                            ));
                        }
                        info!(
                            count = parallel_reads.len(),
                            "Executing parallel read tools"
                        );
                        let futures = parallel_reads.into_iter().map(|call| async {
                            let res = tool_router
                                .route(&call.name, call.arguments.clone(), &context)
                                .await;
                            (call, res)
                        });

                        let results = futures::future::join_all(futures).await;

                        for (call, result) in results {
                            match result {
                                Ok(result_value) => {
                                    let result_str = serde_json::to_string(&result_value)
                                        .unwrap_or_else(|_| "[]".to_string());
                                    if let Some(tx) = &updates {
                                        let _ = tx.send(AgentLoopEvent::ToolResult {
                                            id: call.id.clone(),
                                            name: call.name.clone(),
                                            content: result_str.clone(),
                                            success: true,
                                        });
                                    }
                                    messages.push(Message::tool(call.name, call.id, result_str));
                                }
                                Err(e) => {
                                    error!(tool = %call.name, error = %e, "Parallel tool failed");
                                    if let Some(tx) = &updates {
                                        let _ = tx.send(AgentLoopEvent::ToolResult {
                                            id: call.id.clone(),
                                            name: call.name.clone(),
                                            content: format!("Error: {}", e),
                                            success: false,
                                        });
                                    }
                                    messages.push(Message::tool(
                                        call.name,
                                        call.id,
                                        format!("Error: {}", e),
                                    ));
                                }
                            }
                        }
                    }

                    // Execute serial write operations one at a time (Control Lane)
                    for call in serial_writes {
                        if let Some(tx) = &updates {
                            let _ = tx.send(AgentLoopEvent::Status(status_for_tool(&call.name)));
                        }
                        info!(tool = %call.name, "Executing serial write tool");

                        match tool_router
                            .route(&call.name, call.arguments.clone(), &context)
                            .await
                        {
                            Ok(result) => {
                                let result_str = serde_json::to_string(&result)
                                    .unwrap_or_else(|_| "[]".to_string());
                                info!(tool = %call.name, "Tool executed successfully");
                                if let Some(tx) = &updates {
                                    let _ = tx.send(AgentLoopEvent::ToolResult {
                                        id: call.id.clone(),
                                        name: call.name.clone(),
                                        content: result_str.clone(),
                                        success: true,
                                    });
                                }

                                if call.name == "file_write" || call.name == "file_edit" {
                                    let path_arg = match call.name.as_str() {
                                        "file_write" => call.arguments.get("path"),
                                        "file_edit" => call.arguments.get("file_path"),
                                        _ => None,
                                    };

                                    if let Some(path_value) = path_arg {
                                        if let Ok(path) =
                                            serde_json::from_value::<String>(path_value.clone())
                                        {
                                            let is_create = call.name == "file_write"
                                                && result_str.contains("\"created\":true");
                                            if is_create {
                                                if !created_files.contains(&path) {
                                                    created_files.push(path);
                                                }
                                            } else if !modified_files.contains(&path) {
                                                modified_files.push(path);
                                            }
                                        }
                                    }
                                }

                                messages.push(Message {
                                    role: Role::Tool,
                                    content: result_str,
                                    name: Some(call.name),
                                    tool_call_id: Some(call.id),
                                    tool_calls: vec![],
                                });
                            }
                            Err(e) => {
                                error!(tool = %call.name, error = %e, "Serial tool execution failed");
                                if let Some(tx) = &updates {
                                    let _ = tx.send(AgentLoopEvent::ToolResult {
                                        id: call.id.clone(),
                                        name: call.name.clone(),
                                        content: format!("Error: {}", e),
                                        success: false,
                                    });
                                }
                                messages.push(Message::tool(
                                    call.name,
                                    call.id,
                                    format!("Error: {}", e),
                                ));
                            }
                        }
                    }
                    if let Some(tx) = &updates {
                        let _ =
                            tx.send(AgentLoopEvent::Status("Reviewing tool results".to_string()));
                    }
                }
                vil_llm::provider::FinishReason::MaxTokens => {
                    warn!("Context window limit reached, compressing context");
                    // Keep system prompt and last 50% of messages
                    let keep_count = (messages.len() / 2).max(4);
                    let system_msg = messages.remove(0);
                    messages = vec![system_msg]
                        .into_iter()
                        .chain(messages.drain(messages.len() - keep_count..))
                        .collect();
                    info!(remaining = messages.len(), "Context compressed");
                }
                _ => {
                    warn!(reason = ?response.finish_reason, "Unknown finish reason, terminating loop");
                    break;
                }
            }
        }

        Ok(ExecutionResult {
            summary: "Task executed via single-agent loop".to_string(),
            modified_files,
            created_files,
            total_tokens_used: total_tokens,
            agent_contributions: vec![AgentContribution {
                agent_id: "unified-agent".to_string(),
                agent_role: "Unified".to_string(),
                actions: Vec::new(),
                tokens_used: total_tokens,
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

fn status_for_tool(tool_name: &str) -> String {
    match tool_name {
        "bash" => "Running shell command".to_string(),
        "cargo" => "Running cargo task".to_string(),
        "git" => "Running git command".to_string(),
        "file_edit" => "Editing files".to_string(),
        "file_write" => "Writing files".to_string(),
        "todo_write" => "Updating task list".to_string(),
        "task_done" => "Marking task complete".to_string(),
        other => format!("Using {}", other),
    }
}
