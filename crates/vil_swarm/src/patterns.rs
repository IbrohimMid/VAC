//! Swarm patterns: Pipeline Relay, Parallel Sweep, Consensus Coding, Guardian Loop.
//! Phase 2+ implementation.

use crate::agent::AgentRole;

#[derive(Debug, Clone)]
pub enum SwarmPattern {
    /// Sequential: Agent A -> Agent B -> Agent C
    PipelineRelay { stages: Vec<AgentRole> },
    /// Parallel: Multiple agents work simultaneously
    ParallelSweep { agents: Vec<AgentRole> },
    /// Multiple coders generate solutions, then vote
    ConsensusCoding { coder_count: usize },
    /// Continuous loop: Code -> Test -> Fix -> Validate
    GuardianLoop { max_iterations: usize },
}

impl SwarmPattern {
    pub fn default_for_task(task_type: &str) -> Self {
        match task_type {
            "implement" => Self::PipelineRelay {
                stages: vec![AgentRole::Architect, AgentRole::Coder, AgentRole::Tester],
            },
            "review" => Self::ParallelSweep {
                agents: vec![AgentRole::Security, AgentRole::Tester, AgentRole::Optimizer],
            },
            "critical" => Self::ConsensusCoding { coder_count: 3 },
            _ => Self::GuardianLoop { max_iterations: 5 },
        }
    }
}
