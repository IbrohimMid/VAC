use serde::{Deserialize, Serialize};

/// Permission class — coarse gate applied by the policy layer before
/// `ToolCapability` is consulted. Matches Claude Code's four-tier
/// `ToolPermissionContext` partitioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermissionClass {
    /// No prompt, always allowed. Reserved for pure-read metadata
    /// tools (e.g. signal_list, tool_search).
    Safe,
    /// Prompt once per session. Reversible writes under
    /// `.vac/` or project root (e.g. file_edit with auto-backup).
    AskOnce,
    /// Prompt every call. Non-reversible reads/writes affecting the
    /// wider system (bash, cargo, git push).
    AskEveryCall,
    /// Requires a rulebook explicitly allowing it. Destructive ops
    /// (drop database, force-push, rm -rf) live here.
    Privileged,
}

impl ToolPermissionClass {
    /// Whether a tool with this class needs to prompt the operator
    /// before running (given no prior approval in this session).
    pub fn prompts_operator(&self) -> bool {
        !matches!(self, Self::Safe)
    }
}

impl Default for ToolPermissionClass {
    /// Default to the most conservative class; tools must opt down.
    fn default() -> Self {
        Self::AskEveryCall
    }
}
