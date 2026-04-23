//! VIL Swarm — Multi-agent Tri-Lane orchestrator.
//!
//! Manages specialist agents communicating via Trigger/Data/Control lanes
//! with fault isolation, checkpoint/restore, and parallel execution.

pub mod agent;
pub mod checkpoint;
pub mod context_budget;
pub mod context_crawler;
pub mod planner;
pub mod error;
pub mod events;
pub mod hooks;
pub mod lanes;
pub mod loop_control;
pub mod orchestrator;
pub mod patterns;
pub mod strategy;
pub mod policy_bridge;
pub mod protocol;
pub mod reasoning_fsm;
pub mod redaction;
pub mod run_state;
pub mod sandbox;
pub mod semantic;
pub mod stream_processor;
pub mod subagent;
pub mod tool_execution;
pub mod tool_executor;

pub use agent::{AgentDefinition, AgentId, AgentRole};
pub use checkpoint::SessionInfo;
pub use error::SwarmError;
pub use error::SwarmResult;
pub use lanes::{ControlLane, DataLane, TriggerLane};
pub use orchestrator::{
    AgentLoopEvent, ApprovalResponse, ExternalDiagnosticContext, SubtaskResult, SwarmOrchestrator,
    VilArchetype, VilProjectProfile,
};
pub use protocol::{ControlPayload, DataPayload, TriggerPayload, VapMessage};
pub use run_state::{AgentRunState, RunStage};
pub use sandbox::{SandboxHandle, SandboxMode, SandboxRegistry, SandboxStatus};
pub use semantic::{SemanticPlan, TaskSemanticKind};
