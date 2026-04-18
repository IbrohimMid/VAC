//! Core error types for VAC engine.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum VacError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IR pipeline error: {0}")]
    IrPipeline(#[from] vil_ir::IrError),

    #[error("Context engine error: {0}")]
    Context(#[from] vil_context::ContextError),

    #[error("Memory error: {0}")]
    Memory(#[from] vil_memory::MemoryError),

    #[error("Swarm orchestration error: {0}")]
    Swarm(#[from] vil_swarm::SwarmError),

    #[error("Tool execution error: {0}")]
    Tool(#[from] vac_tools::ToolError),

    #[error("LLM provider error: {0}")]
    Llm(#[from] vil_llm::LlmError),

    #[error("Trust/policy violation: {0}")]
    Trust(#[from] vil_trust::TrustError),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Task error: {0}")]
    Task(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("approval error: {0}")]
    Approval(#[from] vac_approvals::ApprovalError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type VacResult<T> = Result<T, VacError>;
