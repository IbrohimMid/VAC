use serde::{Deserialize, Serialize};

use crate::capability::ToolCapability;
use crate::permission::ToolPermissionClass;
use crate::render::ToolRenderHints;

/// Complete formal specification of a tool. Every tool implementation
/// in `vac_tools::builtin::*` returns one of these at registration
/// time; the registry, router, trace exporter, and bridge all read
/// from this single source of truth.
///
/// Inspired by Claude Code's `Tool.ts` but trimmed to the five fields
/// that actually affect routing + rendering. Extension points
/// (description i18n, permission overrides per-project, etc.) are
/// intentionally left for future work rather than pre-added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Stable identifier. Matches the string the LLM sees in its tool
    /// manifest. Must be unique within a registry.
    pub name: String,
    /// One-line description for the LLM + `/help` surface.
    pub description: String,
    /// JSON Schema for the `execute` input.
    pub input_schema: serde_json::Value,
    /// Capability flags (read_only, destructive, concurrency_safe,
    /// requires_runtime, requires_vil_semantics).
    pub capability: ToolCapability,
    /// Permission class — what level of operator prompt this tool
    /// needs before execution.
    pub permission: ToolPermissionClass,
    /// Rendering hints for TUI, transcript, and bridge surfaces.
    pub render: ToolRenderHints,
}

impl ToolSpec {
    /// Minimal constructor — for read-only metadata tools like
    /// `signal_list` or `tool_search` where every default is correct.
    pub fn read_only(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            capability: ToolCapability::default(),
            permission: ToolPermissionClass::Safe,
            render: ToolRenderHints::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_ctor_sets_safe_defaults() {
        let spec = ToolSpec::read_only(
            "signal_list",
            "list signal streams",
            serde_json::json!({ "type": "object" }),
        );
        assert!(spec.capability.read_only);
        assert!(!spec.capability.destructive);
        assert!(spec.capability.concurrency_safe);
        assert_eq!(spec.permission, ToolPermissionClass::Safe);
    }

    #[test]
    fn capability_destructive_preset_unmasks_flags() {
        let cap = ToolCapability::destructive();
        assert!(!cap.read_only);
        assert!(cap.destructive);
        assert!(!cap.concurrency_safe);
    }

    #[test]
    fn permission_prompts_operator_matches_class() {
        assert!(!ToolPermissionClass::Safe.prompts_operator());
        assert!(ToolPermissionClass::AskOnce.prompts_operator());
        assert!(ToolPermissionClass::AskEveryCall.prompts_operator());
        assert!(ToolPermissionClass::Privileged.prompts_operator());
    }

    #[test]
    fn result_ok_and_error_helpers_set_kind() {
        let ok = crate::ToolResultEnvelope::ok("done", serde_json::json!({"n": 3}));
        assert_eq!(ok.kind, crate::ToolResultKind::Ok);
        assert_eq!(ok.summary, "done");

        let err = crate::ToolResultEnvelope::error("failed", "disk full");
        assert_eq!(err.kind, crate::ToolResultKind::Error);
        assert_eq!(err.payload["error"], "disk full");
    }

    #[test]
    fn spec_serde_roundtrip() {
        let spec = ToolSpec::read_only("x", "y", serde_json::json!({"type": "object"}));
        let json = serde_json::to_string(&spec).unwrap();
        let back: ToolSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "x");
        assert_eq!(back.permission, ToolPermissionClass::Safe);
    }
}
