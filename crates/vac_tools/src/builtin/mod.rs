pub mod bash;
pub mod cargo;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod git;
pub mod glob;
pub mod grep;
pub mod knowledge;
pub mod search;
pub mod sequential_think;
pub mod skill_runner;
pub mod task_done;
pub mod todo;
pub mod vil_status;

use std::sync::Arc;
use crate::ToolError;
use crate::registry::ToolRegistry;

pub async fn register_builtin_tools(registry: &Arc<ToolRegistry>) -> Result<(), ToolError> {
    registry.register(file_read::FileReadTool::new()).await?;
    registry.register(file_write::FileWriteTool::new()).await?;
    registry.register(file_edit::FileEditTool).await?;
    registry.register(glob::GlobTool).await?;
    registry.register(grep::GrepTool).await?;
    registry.register(knowledge::KnowledgeTool::new()).await?;
    registry.register(bash::BashTool::new()).await?;
    registry.register(git::GitTool::new()).await?;
    registry.register(cargo::CargoTool::new()).await?;
    registry.register(search::SearchTool::new()).await?;
    registry.register(task_done::TaskDoneTool::new()).await?;
    registry.register(todo::TodoTool::default()).await?;
    registry.register(sequential_think::SequentialThinkTool::new()).await?;
    registry.register(vil_status::VilStatusTool::new(registry.clone())).await?;
    
    let skills_dir = std::path::PathBuf::from(".vac/skills");
    registry.register(skill_runner::SkillRunnerTool::new(skills_dir, registry.clone())).await?;
    
    Ok(())
}
