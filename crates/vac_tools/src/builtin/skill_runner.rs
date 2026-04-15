//! VIL Skill Runner — executes multi-step VIL skill workflows.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::error::ToolError;
use crate::registry::{ToolContext, ToolRegistry, VilTool};
use crate::skills::{SkillLoader, substitute_params_json};

#[derive(Debug, Deserialize)]
struct SkillRunnerInput {
    action: String,
    #[serde(default)]
    parameters: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct SkillListOutput {
    skills: Vec<SkillInfo>,
}

#[derive(Debug, Serialize)]
struct SkillInfo {
    name: String,
    description: String,
    category: String,
    parameters: Vec<ParamInfo>,
}

#[derive(Debug, Serialize)]
struct ParamInfo {
    name: String,
    description: String,
    required: bool,
    default: Option<String>,
}

#[derive(Debug, Serialize)]
struct SkillRunOutput {
    skill: String,
    steps_executed: usize,
    results: Vec<StepResult>,
}

#[derive(Debug, Serialize)]
struct StepResult {
    step: usize,
    tool: String,
    description: String,
    success: bool,
    output_preview: String,
}

pub struct SkillRunnerTool {
    loader: SkillLoader,
    registry: Arc<ToolRegistry>,
}

impl SkillRunnerTool {
    pub fn new(skills_dir: PathBuf, registry: Arc<ToolRegistry>) -> Self {
        Self {
            loader: SkillLoader::new(skills_dir),
            registry,
        }
    }
}

#[async_trait]
impl VilTool for SkillRunnerTool {
    fn name(&self) -> &str {
        "run_skill"
    }

    fn description(&self) -> &str {
        "Run a pre-built VIL skill workflow. Use action='list' to see available skills, or provide a skill name to execute it. Skills automate common VIL tasks like creating servers, pipelines, plugins, LLM/RAG setup."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Skill name to run, or 'list' to list all available skills"
                },
                "parameters": {
                    "type": "object",
                    "description": "Key-value parameters for the skill (e.g., {\"app_name\": \"my-server\", \"port\": \"8080\"})"
                }
            },
            "required": ["action"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "trusted"
    }
    fn risk_level(&self) -> &str {
        "medium"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: SkillRunnerInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        if input.action == "list" {
            let skills = self.loader.load_skills().await?;
            let output = SkillListOutput {
                skills: skills
                    .iter()
                    .map(|s| SkillInfo {
                        name: s.name.clone(),
                        description: s.description.clone(),
                        category: s.category.clone(),
                        parameters: s
                            .parameters
                            .iter()
                            .map(|p| ParamInfo {
                                name: p.name.clone(),
                                description: p.description.clone(),
                                required: p.required,
                                default: p.default.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            };
            return serde_json::to_value(output)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()));
        }

        let skill =
            self.loader.find_skill(&input.action).await.ok_or_else(|| {
                ToolError::NotFound(format!("Skill '{}' not found", input.action))
            })?;

        info!(skill = %skill.name, steps = skill.steps.len(), "Running skill");

        let mut params = HashMap::new();
        for p in &skill.parameters {
            if let Some(ref default) = p.default {
                params.insert(p.name.clone(), default.clone());
            }
        }
        params.extend(input.parameters);

        for p in &skill.parameters {
            if p.required && !params.contains_key(&p.name) {
                return Err(ToolError::InvalidArguments(format!(
                    "Missing required parameter: '{}' ({})",
                    p.name, p.description
                )));
            }
        }

        let mut results = Vec::new();
        for (i, step) in skill.steps.iter().enumerate() {
            debug!(step = i + 1, tool = %step.tool, desc = %step.description, "Executing skill step");

            let substituted_args = substitute_params_json(&step.arguments, &params);

            match self
                .registry
                .execute(&step.tool, substituted_args, context)
                .await
            {
                Ok(output) => {
                    let preview = output.to_string().chars().take(200).collect::<String>();
                    results.push(StepResult {
                        step: i + 1,
                        tool: step.tool.clone(),
                        description: step.description.clone(),
                        success: true,
                        output_preview: preview,
                    });
                }
                Err(e) => {
                    warn!(step = i + 1, error = %e, "Skill step failed");
                    results.push(StepResult {
                        step: i + 1,
                        tool: step.tool.clone(),
                        description: step.description.clone(),
                        success: false,
                        output_preview: e.to_string(),
                    });
                }
            }
        }

        let output = SkillRunOutput {
            skill: skill.name.clone(),
            steps_executed: results.len(),
            results,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
