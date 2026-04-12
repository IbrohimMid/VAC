//! Loop control helpers for agent execution loop.
//! Extracted from orchestrator.rs — generic infra, not VIL semantic policy.

use vil_llm::provider::FinishReason;
use crate::error::{SwarmError, SwarmResult};

pub const MAX_ITERATIONS: usize = 30;

/// Check iteration cap. Returns Err if exceeded.
pub fn check_iteration_cap(iteration: usize) -> SwarmResult<()> {
    if iteration >= MAX_ITERATIONS {
        Err(SwarmError::Orchestration(format!(
            "Agent loop exceeded {} iterations without completing", MAX_ITERATIONS
        )))
    } else {
        Ok(())
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
mod tests {
    use super::*;

    #[test]
    fn iteration_cap_allows_under_limit() {
        assert!(check_iteration_cap(0).is_ok());
        assert!(check_iteration_cap(29).is_ok());
    }

    #[test]
    fn iteration_cap_rejects_at_limit() {
        assert!(check_iteration_cap(30).is_err());
        assert!(check_iteration_cap(100).is_err());
    }

    #[test]
    fn emergency_trim_advances_boundary() {
        assert_eq!(emergency_trim_boundary(20, 0), 10);
        assert_eq!(emergency_trim_boundary(20, 12), 12); // doesn't go backward
    }
}
