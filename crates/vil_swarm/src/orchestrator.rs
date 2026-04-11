//! Swarm Orchestrator — manages agent lifecycle and task routing.

use crate::agent::*;
use crate::error::{SwarmError, SwarmResult};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
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
2. If you need to read existing files, use file_read tool
3. Write new or modified code using file_write tool
4. If compilation is needed, use bash tool to run cargo build or similar
5. Report success or any errors encountered

You have access to tools for:
- Reading files: file_read
- Writing files: file_write
- Running commands: bash (cargo build, cargo test, etc.)

When you encounter errors:
1. Read the error message carefully
2. Fix the issue in the code
3. Re-run to verify the fix
4. Repeat until successful or report failure".to_string()
    }

    pub async fn plan_task(&mut self, task_description: &str) -> SwarmResult<TaskPlan> {
        info!(task = %task_description, "Planning task via Architect agent");

        let router = self
            .llm_router
            .as_ref()
            .ok_or_else(|| SwarmError::Orchestration("LLM router not initialized".into()))?;

        let system_prompt = Self::architect_system_prompt();
        let request = LlmRequest::new(vec![
            Message::system(system_prompt),
            Message::user(task_description),
        ])
        .with_max_tokens(4000);

        let response = router
            .complete(&request)
            .await
            .map_err(|e| SwarmError::Orchestration(format!("LLM error: {}", e)))?;

        let plan = self.parse_architect_response(&response.content)?;

        info!(
            subtasks = plan.subtasks.len(),
            "Task plan generated successfully"
        );
        Ok(plan)
    }

    fn parse_architect_response(&self, content: &str) -> SwarmResult<TaskPlan> {
        let json_start = content.find('{').or_else(|| content.find('['));
        let json_end = content.rfind('}').or_else(|| content.rfind(']'));

        let json_str = if let (Some(start), Some(end)) = (json_start, json_end) {
            &content[start..=end]
        } else {
            content
        };

        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| SwarmError::Orchestration(format!("Failed to parse plan JSON: {}", e)))?;

        let subtasks: Vec<SubTask> = parsed["subtasks"]
            .as_array()
            .ok_or_else(|| SwarmError::Orchestration("Missing subtasks in plan".into()))?
            .iter()
            .map(|t| {
                let role_str = t["assigned_role"].as_str().unwrap_or("Coder");
                let role = match role_str {
                    "Architect" => AgentRole::Architect,
                    "Coder" => AgentRole::Coder,
                    "Tester" => AgentRole::Tester,
                    "Security" => AgentRole::Security,
                    "Deploy" => AgentRole::Deploy,
                    "Monitor" => AgentRole::Monitor,
                    "Optimizer" => AgentRole::Optimizer,
                    "Documenter" => AgentRole::Documenter,
                    _ => AgentRole::Coder,
                };
                SubTask {
                    description: t["description"].as_str().unwrap_or("").to_string(),
                    assigned_role: role,
                    dependencies: t["dependencies"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as usize))
                                .collect()
                        })
                        .unwrap_or_default(),
                }
            })
            .collect();

        let execution_order: Vec<usize> = parsed["execution_order"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_else(|| (0..subtasks.len()).collect());

        let can_parallelize: Vec<Vec<usize>> = parsed["can_parallelize"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|v| {
                        v.as_array()
                            .map(|inner| {
                                inner
                                    .iter()
                                    .filter_map(|x| x.as_u64().map(|n| n as usize))
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(TaskPlan {
            subtasks,
            execution_order,
            can_parallelize,
        })
    }

    pub async fn execute_plan(&mut self, plan: &TaskPlan) -> SwarmResult<ExecutionResult> {
        info!(subtasks = plan.subtasks.len(), "Executing plan");

        let mut contributions = Vec::new();
        let mut total_tokens = 0u64;
        let mut modified_files = Vec::new();
        let mut created_files = Vec::new();
        let mut subtask_logs = Vec::new();

        let llm_router = self
            .llm_router
            .as_ref()
            .ok_or_else(|| SwarmError::Orchestration("LLM router not initialized".into()))?;
        let tool_router = self
            .tool_router
            .as_ref()
            .ok_or_else(|| SwarmError::Orchestration("Tool router not initialized".into()))?;

        for (i, idx) in plan.execution_order.iter().enumerate() {
            let subtask = &plan.subtasks[*idx];
            info!(step = i + 1, role = ?subtask.assigned_role, desc = %subtask.description, "Executing subtask");

            let role = subtask.assigned_role;
            let system_prompt = match role {
                AgentRole::Coder => Self::coder_system_prompt(),
                _ => format!(
                    "You are the {:?} agent. Execute the subtask: {}",
                    role, subtask.description
                ),
            };

            let context = ToolContext::new(std::path::PathBuf::from("."));

            let mut messages = vec![
                Message::system(system_prompt),
                Message::user(subtask.description.clone()),
            ];

            let tools = vec![
                ToolDefinition {
                    name: "file_read".to_string(),
                    description: "Read contents of a file".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Path to file to read"}
                        },
                        "required": ["path"]
                    }),
                },
                ToolDefinition {
                    name: "file_write".to_string(),
                    description: "Write content to a file".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Path to file to write"},
                            "content": {"type": "string", "description": "Content to write"}
                        },
                        "required": ["path", "content"]
                    }),
                },
                ToolDefinition {
                    name: "bash".to_string(),
                    description: "Run a bash command".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "command": {"type": "string", "description": "Command to run"}
                        },
                        "required": ["command"]
                    }),
                },
            ];

            let mut iterations = 0;
            let max_iterations = 10;
            let mut subtask_success = false;

            while iterations < max_iterations {
                iterations += 1;

                let request = LlmRequest::new(messages.clone())
                    .with_max_tokens(4000)
                    .with_tools(tools.clone());

                let response = match llm_router.complete(&request).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        warn!(error = %e, "LLM request failed");
                        break;
                    }
                };

                total_tokens += response.usage.total_tokens;

                if response.tool_calls.is_empty() {
                    if !response.content.is_empty() {
                        subtask_logs.push(format!("{:?}: {}", role, response.content));
                    }
                    subtask_success = true;
                    break;
                }

                for tool_call in &response.tool_calls {
                    info!(tool = %tool_call.name, "Executing tool");

                    let tool_name = &tool_call.name;
                    let args = tool_call.arguments.clone();

                    match tool_router.route(tool_name, args.clone(), &context).await {
                        Ok(result) => {
                            let result_str =
                                serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string());
                            info!(tool = %tool_name, result = %result_str, "Tool executed");

                            if tool_name == "file_write" {
                                if let Ok(path) = serde_json::from_value::<String>(
                                    args.get("path").cloned().unwrap_or(json!(null)).clone(),
                                ) {
                                    if !modified_files.contains(&path)
                                        && !created_files.contains(&path)
                                    {
                                        if std::path::Path::new(&path).exists() {
                                            modified_files.push(path.clone());
                                        } else {
                                            created_files.push(path.clone());
                                        }
                                    }
                                }
                            }

                            messages.push(Message {
                                role: Role::Tool,
                                content: result_str,
                                name: Some(tool_call.name.clone()),
                            });
                        }
                        Err(e) => {
                            error!(tool = %tool_name, error = %e, "Tool execution failed");

                            messages.push(Message {
                                role: Role::Tool,
                                content: format!("Error: {}", e),
                                name: Some(tool_call.name.clone()),
                            });
                        }
                    }
                }

                if response.finish_reason == vil_llm::provider::FinishReason::Stop {
                    subtask_success = true;
                    break;
                }
            }

            if !subtask_success {
                warn!(subtask = %subtask.description, "Subtask failed after max iterations");
            }

            contributions.push(AgentContribution {
                agent_id: format!("{:?}-agent", role).to_lowercase(),
                agent_role: format!("{:?}", role),
                actions: subtask_logs.clone(),
                tokens_used: 0,
            });
        }

        Ok(ExecutionResult {
            summary: format!(
                "Task executed via swarm ({} subtasks completed)",
                plan.subtasks.len()
            ),
            modified_files,
            created_files,
            total_tokens_used: total_tokens,
            agent_contributions: contributions,
        })
    }

    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    pub fn agents(&self) -> &HashMap<AgentId, AgentDefinition> {
        &self.agents
    }
}
