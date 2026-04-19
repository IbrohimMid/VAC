//! Loop control helpers for agent execution loop.
//! Extracted from orchestrator.rs — generic infra, not VIL semantic policy.

use crate::error::{SwarmError, SwarmResult};
use vil_llm::provider::FinishReason;

#[derive(Debug, Clone, Copy)]
pub struct LoopConfig {
    pub max_iterations: usize,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self { max_iterations: 30 }
    }
}

pub struct LoopController {
    pub config: LoopConfig,
}

impl LoopController {
    pub fn new(config: LoopConfig) -> Self {
        Self { config }
    }

    /// Check iteration cap. Returns Err if exceeded.
    pub fn check_iteration_cap(&self, iteration: usize) -> SwarmResult<()> {
        if iteration >= self.config.max_iterations {
            Err(SwarmError::Orchestration(format!(
                "Agent loop exceeded {} iterations without completing",
                self.config.max_iterations
            )))
        } else {
            Ok(())
        }
    }
}

/// Map a finish reason to a human-readable loop action description.
pub fn describe_finish_reason(reason: &FinishReason) -> &'static str {
    match reason {
        FinishReason::Stop => "completed",
        FinishReason::ToolUse => "tool calls pending",
        FinishReason::MaxTokens => "context limit reached",
        FinishReason::Error => "error",
    }
}

/// Compute the new trim_boundary for a MaxTokens emergency.
/// Advances boundary to mid-point of current message list.
pub fn emergency_trim_boundary(message_count: usize, current_boundary: usize) -> usize {
    (message_count / 2).max(current_boundary)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn iteration_cap_allows_under_limit() {
        let controller = LoopController::new(LoopConfig::default());
        assert!(controller.check_iteration_cap(0).is_ok());
        assert!(controller.check_iteration_cap(29).is_ok());
    }

    #[test]
    fn iteration_cap_rejects_at_limit() {
        let controller = LoopController::new(LoopConfig::default());
        assert!(controller.check_iteration_cap(30).is_err());
        assert!(controller.check_iteration_cap(100).is_err());
    }

    #[test]
    fn loop_controller_respects_custom_max_iterations() {
        let controller = LoopController::new(LoopConfig { max_iterations: 5 });
        assert!(controller.check_iteration_cap(4).is_ok());
        assert!(controller.check_iteration_cap(5).is_err());
    }

    #[test]
    fn emergency_trim_advances_boundary() {
        assert_eq!(emergency_trim_boundary(20, 0), 10);
        assert_eq!(emergency_trim_boundary(20, 12), 12); // doesn't go backward
    }
}
