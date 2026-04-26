//! D8F — VacToolDispatcher contract tests.
//!
//! These are intentionally small — they exercise dispatch
//! shape without spinning up the engine. Adapter-level
//! integration sits in `vac_shell_host_vac_command_adapter`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use vac_session_engine::{ToolCallRequest, ToolDispatcher};
use vac_tool_core::ToolResultKind;
use vac_tools::ToolError;
use vac_tools::ToolRegistry;
use vac_tools::registry::{ToolContext, VilTool};
use vac_shell_host_vac_tool_dispatcher::VacToolDispatcher;

// ---------------------------------------------------------------------
// Fake tools — exhaustive failure modes for envelope mapping.
// ---------------------------------------------------------------------

struct OkEcho {
    counter: Arc<AtomicUsize>,
}

#[async_trait]
impl VilTool for OkEcho {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        vac_tools::registry::default_spec(self)
    }
    fn name(&self) -> &str {
        "ok_echo"
    }
    fn description(&self) -> &str {
        "fake echo tool"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    fn trust_requirement(&self) -> &str {
        "none"
    }
    fn risk_level(&self) -> &str {
        "safe"
    }
    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::json!({ "echoed": args }))
    }
}

struct InvalidArgsTool;

#[async_trait]
impl VilTool for InvalidArgsTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        vac_tools::registry::default_spec(self)
    }
    fn name(&self) -> &str {
        "invalid_args"
    }
    fn description(&self) -> &str {
        "rejects every input"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    fn trust_requirement(&self) -> &str {
        "none"
    }
    fn risk_level(&self) -> &str {
        "safe"
    }
    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::InvalidArguments(
            "missing required field `query`".into(),
        ))
    }
}

struct ExecFailTool;

#[async_trait]
impl VilTool for ExecFailTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        vac_tools::registry::default_spec(self)
    }
    fn name(&self) -> &str {
        "exec_fail"
    }
    fn description(&self) -> &str {
        "always fails at runtime"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    fn trust_requirement(&self) -> &str {
        "none"
    }
    fn risk_level(&self) -> &str {
        "safe"
    }
    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::ExecutionFailed(
            "synthetic dispatcher failure".into(),
        ))
    }
}

async fn make_registry() -> (Arc<ToolRegistry>, Arc<AtomicUsize>) {
    let reg = ToolRegistry::new();
    let counter = Arc::new(AtomicUsize::new(0));
    reg.register(OkEcho {
        counter: counter.clone(),
    })
    .await
    .unwrap();
    reg.register(InvalidArgsTool).await.unwrap();
    reg.register(ExecFailTool).await.unwrap();
    (Arc::new(reg), counter)
}

fn call(name: &str, args: serde_json::Value) -> ToolCallRequest {
    ToolCallRequest {
        id: format!("c-{name}"),
        name: name.into(),
        arguments: args,
        reason: None,
        estimated_tokens: 0,
    }
}

// ---------------------------------------------------------------------
// 1. Known tool → ok envelope.
// ---------------------------------------------------------------------

#[tokio::test]
async fn dispatch_known_tool_returns_ok_envelope() {
    let (registry, counter) = make_registry().await;
    let ctx = Arc::new(ToolContext::new(std::env::temp_dir()));
    let dispatcher = VacToolDispatcher::new(registry, ctx);
    let env = dispatcher
        .dispatch(&call("ok_echo", serde_json::json!({"x": 1})))
        .await
        .unwrap();
    assert_eq!(env.kind, ToolResultKind::Ok);
    assert!(env.summary.contains("ok_echo"));
    assert_eq!(env.payload, serde_json::json!({"echoed": {"x": 1}}));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------
// 2. Unknown tool → error envelope (no panic, never registered).
// ---------------------------------------------------------------------

#[tokio::test]
async fn unknown_tool_returns_error_envelope() {
    let (registry, _) = make_registry().await;
    let ctx = Arc::new(ToolContext::new(std::env::temp_dir()));
    let dispatcher = VacToolDispatcher::new(registry, ctx);
    let env = dispatcher
        .dispatch(&call("does_not_exist", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(env.kind, ToolResultKind::Error);
    let dump = serde_json::to_string(&env).unwrap();
    assert!(
        dump.contains("not registered"),
        "envelope must explain unknown tool: {dump}"
    );
}

// ---------------------------------------------------------------------
// 3. InvalidArguments → error envelope with "rejected arguments" summary.
// ---------------------------------------------------------------------

#[tokio::test]
async fn malformed_arguments_returns_error_envelope() {
    let (registry, _) = make_registry().await;
    let ctx = Arc::new(ToolContext::new(std::env::temp_dir()));
    let dispatcher = VacToolDispatcher::new(registry, ctx);
    let env = dispatcher
        .dispatch(&call("invalid_args", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(env.kind, ToolResultKind::Error);
    assert!(
        env.summary.contains("rejected arguments"),
        "summary should distinguish argument errors: {}",
        env.summary
    );
    let dump = serde_json::to_string(&env).unwrap();
    assert!(dump.contains("missing required field"));
}

// ---------------------------------------------------------------------
// 4. ExecutionFailed → error envelope preserves message.
// ---------------------------------------------------------------------

#[tokio::test]
async fn dispatcher_error_preserved_in_envelope() {
    let (registry, _) = make_registry().await;
    let ctx = Arc::new(ToolContext::new(std::env::temp_dir()));
    let dispatcher = VacToolDispatcher::new(registry, ctx);
    let env = dispatcher
        .dispatch(&call("exec_fail", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(env.kind, ToolResultKind::Error);
    let dump = serde_json::to_string(&env).unwrap();
    assert!(
        dump.contains("synthetic dispatcher failure"),
        "envelope must preserve the inner error message: {dump}"
    );
}

// ---------------------------------------------------------------------
// 4b. D8 hardening — registry.execute path is the active code
//     path. This test pins it: an unknown-tool name produces
//     the registry's `ToolError::NotFound` (variant message
//     "Tool not found: <n>"), which the dispatcher maps to
//     summary `tool '<n>' not registered`. A bypass that
//     called `VilTool::execute` directly could not reach this
//     branch because there is no tool to call.
// ---------------------------------------------------------------------

#[tokio::test]
async fn dispatch_unknown_tool_uses_registry_not_found_path() {
    let (registry, _) = make_registry().await;
    let ctx = Arc::new(ToolContext::new(std::env::temp_dir()));
    let dispatcher = VacToolDispatcher::new(registry, ctx);
    let env = dispatcher
        .dispatch(&call("absolutely_unknown", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(env.kind, ToolResultKind::Error);
    let dump = serde_json::to_string(&env).unwrap();
    assert!(
        dump.contains("Tool not found"),
        "envelope must surface the registry's ToolError::NotFound message: {dump}"
    );
    assert!(
        env.summary.contains("not registered"),
        "summary must mention the dispatcher's not-registered phrasing: {}",
        env.summary
    );
}

// ---------------------------------------------------------------------
// 5. Bad payload (random JSON) does not panic; tool either accepts
//    it or returns an error envelope.
// ---------------------------------------------------------------------

#[tokio::test]
async fn no_panic_on_bad_tool_call_payload() {
    let (registry, _) = make_registry().await;
    let ctx = Arc::new(ToolContext::new(std::env::temp_dir()));
    let dispatcher = VacToolDispatcher::new(registry, ctx);
    // Deliberately weird arguments shape; OkEcho accepts everything.
    let env = dispatcher
        .dispatch(&call(
            "ok_echo",
            serde_json::json!([1, "two", null, {"nested": true}]),
        ))
        .await
        .unwrap();
    assert!(matches!(env.kind, ToolResultKind::Ok));
    // And bad-target with same payload still returns Error, not panic.
    let env_unknown = dispatcher
        .dispatch(&call(
            "ghost",
            serde_json::json!({"any": "shape"}),
        ))
        .await
        .unwrap();
    assert_eq!(env_unknown.kind, ToolResultKind::Error);
}
