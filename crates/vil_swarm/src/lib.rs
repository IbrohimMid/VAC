//! VIL Swarm — Multi-agent Tri-Lane orchestrator.
//!
//! Manages specialist agents communicating via Trigger/Data/Control lanes
//! with fault isolation, checkpoint/restore, and parallel execution.

pub mod agent;
pub mod error;
pub mod lanes;
pub mod orchestrator;
pub mod patterns;
pub mod protocol;

pub use agent::{AgentDefinition, AgentId, AgentRole};
pub use error::SwarmError;
pub use error::SwarmResult;
pub use lanes::{ControlLane, DataLane, TriggerLane};
pub use orchestrator::{AgentLoopEvent, SwarmOrchestrator};
pub use protocol::{ControlPayload, DataPayload, TriggerPayload, VapMessage};
