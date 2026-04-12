//! Hook/intercept abstraction for agent tool calls and LLM requests.
//! Adapted from stakpak/libs/agent-core/src/hooks.rs (Apache-2.0).
//! Generic control-plane — no VIL semantic policy here.

use vil_llm::provider::ToolCall;

/// Context passed to before_tool_call hook.
pub struct ToolCallCtx<'a> {
    pub tool_name: &'a str,
    pub call_id: &'a str,
    pub arguments: &'a serde_json::Value,
}

/// Context passed to after_tool_call hook.
pub struct ToolResultCtx<'a> {
    pub tool_name: &'a str,
    pub call_id: &'a str,
    pub success: bool,
    pub content: &'a str,
}

/// Decision returned by before_tool_call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    /// Allow the tool call to proceed.
    Allow,
    /// Deny the tool call with a reason.
    Deny(String),
}

/// Pluggable hook for agent tool call lifecycle.
pub trait AgentHook: Send + Sync {
    fn before_tool_call(&self, ctx: &ToolCallCtx<'_>) -> HookDecision;
    fn after_tool_call(&self, ctx: &ToolResultCtx<'_>);
}

/// No-op hook — default behavior, identical to no hook installed.
pub struct NoOpHook;

impl AgentHook for NoOpHook {
    fn before_tool_call(&self, _ctx: &ToolCallCtx<'_>) -> HookDecision {
        HookDecision::Allow
    }
    fn after_tool_call(&self, _ctx: &ToolResultCtx<'_>) {}
}

/// Run before_tool_call through an optional hook. Returns Allow if no hook.
pub fn run_before_hook(
    hook: Option<&dyn AgentHook>,
    call: &ToolCall,
) -> HookDecision {
    match hook {
        Some(h) => h.before_tool_call(&ToolCallCtx {
            tool_name: &call.name,
            call_id: &call.id,
            arguments: &call.arguments,
        }),
        None => HookDecision::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct DenyAllHook;
    impl AgentHook for DenyAllHook {
        fn before_tool_call(&self, ctx: &ToolCallCtx<'_>) -> HookDecision {
            HookDecision::Deny(format!("denied: {}", ctx.tool_name))
        }
        fn after_tool_call(&self, _ctx: &ToolResultCtx<'_>) {}
    }

    #[test]
    fn noop_hook_allows_all() {
        let call = ToolCall { id: "1".into(), name: "bash".into(), arguments: json!({}) };
        assert_eq!(run_before_hook(Some(&NoOpHook), &call), HookDecision::Allow);
    }

    #[test]
    fn deny_hook_blocks_call() {
        let call = ToolCall { id: "1".into(), name: "bash".into(), arguments: json!({}) };
        let result = run_before_hook(Some(&DenyAllHook), &call);
        assert!(matches!(result, HookDecision::Deny(_)));
    }

    #[test]
    fn no_hook_installed_allows() {
        let call = ToolCall { id: "1".into(), name: "bash".into(), arguments: json!({}) };
        assert_eq!(run_before_hook(None, &call), HookDecision::Allow);
    }
}
