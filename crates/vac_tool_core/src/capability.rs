use serde::{Deserialize, Serialize};

/// Capability metadata for a tool. Routing layers (executor, policy
/// gate, bridge permission mediator) inspect this to decide parallelism
/// safety, destructiveness, and whether the tool needs VIL semantics
/// or a running runtime.
///
/// Intentionally a single struct with bools — extensible without
/// breaking consumers. Add new fields with `#[serde(default)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCapability {
    /// `true` if the tool never mutates observable state (file edits,
    /// process spawns, network calls all count as mutation).
    pub read_only: bool,
    /// `true` if the tool can permanently destroy data (delete files,
    /// drop databases, force-push). Subset of non-`read_only`.
    pub destructive: bool,
    /// `true` if multiple invocations of this tool in the same turn
    /// can safely run concurrently without data races.
    pub concurrency_safe: bool,
    /// `true` if the tool needs the `vac_runtime` task scheduler to be
    /// online (e.g. ScheduleCronTool, TaskCreateTool).
    pub requires_runtime: bool,
    /// `true` if the tool reads/writes VIL semantic artefacts (VWFD,
    /// IR, knowledge base) and should not run outside a VIL project.
    pub requires_vil_semantics: bool,
}

impl Default for ToolCapability {
    /// Safest possible default: read-only, non-destructive, safe to
    /// parallel, no runtime/VIL requirements. Tools override fields
    /// that don't match.
    fn default() -> Self {
        Self {
            read_only: true,
            destructive: false,
            concurrency_safe: true,
            requires_runtime: false,
            requires_vil_semantics: false,
        }
    }
}

impl ToolCapability {
    /// Builder style: start from default and flip bits.
    pub fn destructive() -> Self {
        Self {
            read_only: false,
            destructive: true,
            concurrency_safe: false,
            ..Self::default()
        }
    }

    /// Write but reversible (file edit with backup).
    pub fn mutating() -> Self {
        Self {
            read_only: false,
            destructive: false,
            concurrency_safe: false,
            ..Self::default()
        }
    }
}
