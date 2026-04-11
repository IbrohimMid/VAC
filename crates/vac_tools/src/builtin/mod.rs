pub mod bash;
pub mod cargo;
pub mod file_read;
pub mod file_write;
pub mod git;
pub mod search;
pub mod task_done;

use crate::registry::ToolRegistry;

pub fn register_builtin_tools(registry: &ToolRegistry) {
    std::mem::drop(registry.register(file_read::FileReadTool::new()));
    std::mem::drop(registry.register(file_write::FileWriteTool::new()));
    std::mem::drop(registry.register(bash::BashTool::new()));
    std::mem::drop(registry.register(git::GitTool::new()));
    std::mem::drop(registry.register(cargo::CargoTool::new()));
    std::mem::drop(registry.register(search::SearchTool::new()));
    std::mem::drop(registry.register(task_done::TaskDoneTool::new()));
}
