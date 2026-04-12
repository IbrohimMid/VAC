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
pub mod task_done;
pub mod todo;

use crate::ToolError;
use crate::registry::ToolRegistry;

pub async fn register_builtin_tools(registry: &ToolRegistry) -> Result<(), ToolError> {
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
    Ok(())
}
