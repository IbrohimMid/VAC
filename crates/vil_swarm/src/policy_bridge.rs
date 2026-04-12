//! Execution decision bridge — normalizes approval/permission flow.
//! Generic control-plane adapter. Does not own VIL semantic policy.

/// Unified execution decision for tool calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionDecision {
    /// Tool call is allowed to proceed immediately.
    Allow,
    /// Tool call is denied with a reason.
    Deny(String),
    /// Tool call requires explicit user approval before proceeding.
    NeedsApproval(String),
}

impl ExecutionDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn denial_reason(&self) -> Option<&str> {
        match self {
            Self::Deny(r) | Self::NeedsApproval(r) => Some(r),
            Self::Allow => None,
        }
    }
}

/// Resolve an execution decision from the current policy context.
/// `needs_approval` comes from the existing tool_policy layer.
/// `is_sandboxed` indicates the call is in a restricted subagent context.
pub fn resolve_decision(
    tool_name: &str,
    needs_approval: bool,
    is_sandboxed: bool,
) -> ExecutionDecision {
    if is_sandboxed && is_risky_in_sandbox(tool_name) {
        return ExecutionDecision::Deny(format!(
            "tool '{}' is not permitted in sandboxed subagent context", tool_name
        ));
    }
    if needs_approval {
        ExecutionDecision::NeedsApproval(format!("approval required for '{}'", tool_name))
    } else {
        ExecutionDecision::Allow
    }
}

fn is_risky_in_sandbox(name: &str) -> bool {
    matches!(name, "bash" | "cargo" | "git" | "file_write" | "file_edit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_safe_tool() {
        assert_eq!(resolve_decision("file_read", false, false), ExecutionDecision::Allow);
    }

    #[test]
    fn needs_approval_when_flagged() {
        let d = resolve_decision("bash", true, false);
        assert!(matches!(d, ExecutionDecision::NeedsApproval(_)));
    }

    #[test]
    fn deny_risky_in_sandbox() {
        let d = resolve_decision("bash", false, true);
        assert!(matches!(d, ExecutionDecision::Deny(_)));
    }

    #[test]
    fn allow_read_in_sandbox() {
        assert_eq!(resolve_decision("file_read", false, true), ExecutionDecision::Allow);
    }
}
