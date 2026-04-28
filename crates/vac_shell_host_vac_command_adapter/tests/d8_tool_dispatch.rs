mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use common::{cmd, mapped};
use vac_session_engine::{
    CompositeGate, EngineError, EngineResult, GateDecision, LlmAdapter, LlmRequest, LlmResponse,
    ToolCallRequest, ToolCheckCtx, ToolDispatcher, ToolGate, read_tool_use_rows,
};
use vac_shell_host_commands::ShellCommandExecutor;
use vac_shell_host_vac_command_adapter::{AdapterConfigError, VacCommandExecutorAdapter};
use vac_shell_host_vac_tool_dispatcher::VacToolDispatcher;
use vac_tool_core::ToolResultKind;
use vac_tools::ToolError;
use vac_tools::ToolRegistry;
use vac_tools::registry::{ToolContext, VilTool};

struct ToolEmittingLlm;

#[async_trait]
impl LlmAdapter for ToolEmittingLlm {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "test".into(),
            model: "test-1".into(),
            content: "want tool".into(),
            input_tokens: 1,
            output_tokens: 1,
            tool_calls: vec![ToolCallRequest {
                id: "live-1".into(),
                name: "ok_echo".into(),
                arguments: serde_json::json!({"x": 7}),
                reason: None,
                estimated_tokens: 0,
            }],
        })
    }
}

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
        "fake echo"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object"})
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
        Ok(serde_json::json!({"echoed": args}))
    }
}

#[derive(Debug)]
struct AlwaysDenyGate;

#[async_trait]
impl ToolGate for AlwaysDenyGate {
    fn label(&self) -> &'static str {
        "deny"
    }
    async fn check(&self, _ctx: &ToolCheckCtx) -> GateDecision {
        GateDecision::Deny {
            reason: "policy refused".into(),
        }
    }
}

#[derive(Debug)]
struct CountingDispatcher {
    inner: VacToolDispatcher,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolDispatcher for CountingDispatcher {
    async fn dispatch(
        &self,
        call: &ToolCallRequest,
    ) -> EngineResult<vac_tool_core::ToolResultEnvelope> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.dispatch(call).await
    }
}

async fn make_registry_with_ok_echo() -> (Arc<ToolRegistry>, Arc<AtomicUsize>) {
    let reg = ToolRegistry::new();
    let counter = Arc::new(AtomicUsize::new(0));
    reg.register(OkEcho {
        counter: counter.clone(),
    })
    .await
    .unwrap();
    (Arc::new(reg), counter)
}

fn build_dispatcher(
    registry: Arc<ToolRegistry>,
    root: std::path::PathBuf,
) -> Arc<VacToolDispatcher> {
    let ctx = Arc::new(ToolContext::new(root));
    Arc::new(VacToolDispatcher::new(registry, ctx))
}

fn mapped_with_tool_emitting_llm(
    root: std::path::PathBuf,
) -> vac_shell_host_vac_command_adapter::AdapterConfig {
    mapped(root).with_llm(Arc::new(ToolEmittingLlm))
}

#[test]
fn default_path_still_unsupported_no_real_tool_execution() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter =
        VacCommandExecutorAdapter::new(mapped_with_tool_emitting_llm(tmp.path().to_path_buf()));
    assert!(!adapter.has_live_tool_dispatcher());
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();
    let path = adapter.last_transcript().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("\"kind\":\"error\""),
        "default path must produce error tool_result envelope: {body}"
    );
    assert!(
        body.contains("no ToolDispatcher") || body.contains("dispatch error"),
        "default path must mention no-dispatcher / dispatch-error: {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_tool_dispatcher_and_gate_allow_writes_ok_tool_result_row() {
    let (registry, counter) = make_registry_with_ok_echo().await;
    let tmp = tempfile::tempdir().unwrap();
    let dispatcher = build_dispatcher(registry, tmp.path().to_path_buf());
    let cfg = mapped_with_tool_emitting_llm(tmp.path().to_path_buf())
        .with_tool_dispatcher(dispatcher, Arc::new(CompositeGate::new()));
    let adapter = VacCommandExecutorAdapter::new(cfg);
    assert!(adapter.has_live_tool_dispatcher());

    let result = tokio::task::spawn_blocking(move || {
        adapter.execute(&cmd("memorize", "/memorize")).unwrap();
        adapter.last_transcript().unwrap()
    })
    .await
    .unwrap();

    let body = std::fs::read_to_string(&result).unwrap();
    assert!(
        body.contains("\"kind\":\"ok\""),
        "live dispatcher must produce ok tool_result envelope: {body}"
    );
    assert!(body.contains("\"echoed\""));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_tool_dispatcher_without_gate_rejected_preflight() {
    let (registry, _) = make_registry_with_ok_echo().await;
    let tmp = tempfile::tempdir().unwrap();
    let dispatcher = build_dispatcher(registry, tmp.path().to_path_buf());
    let res = mapped(tmp.path().to_path_buf()).try_with_tool_dispatcher(dispatcher, None);
    assert!(matches!(
        res,
        Err(AdapterConfigError::DispatcherWithoutGate)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gate_deny_skips_dispatcher_and_writes_error_row() {
    let (registry, _exec_count) = make_registry_with_ok_echo().await;
    let tmp = tempfile::tempdir().unwrap();
    let inner = build_dispatcher(registry, tmp.path().to_path_buf());
    let dispatch_calls = Arc::new(AtomicUsize::new(0));
    let counting = Arc::new(CountingDispatcher {
        inner: VacToolDispatcher::new(inner.registry().clone(), inner.context().clone()),
        calls: dispatch_calls.clone(),
    });
    let gate = Arc::new(CompositeGate::new().with_gate(Arc::new(AlwaysDenyGate)));

    let cfg = mapped_with_tool_emitting_llm(tmp.path().to_path_buf())
        .with_tool_dispatcher(counting, gate);
    let adapter = VacCommandExecutorAdapter::new(cfg);

    let path = tokio::task::spawn_blocking(move || {
        adapter.execute(&cmd("memorize", "/memorize")).unwrap();
        adapter.last_transcript().unwrap()
    })
    .await
    .unwrap();

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("blocked by gate"),
        "envelope must mention blocked-by-gate: {body}"
    );
    assert!(body.contains("policy refused"));
    assert_eq!(
        dispatch_calls.load(Ordering::SeqCst),
        0,
        "gate Deny must short-circuit before dispatcher"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execute_path_with_live_dispatcher_visible_in_transcript() {
    let (registry, counter) = make_registry_with_ok_echo().await;
    let tmp = tempfile::tempdir().unwrap();
    let dispatcher = build_dispatcher(registry, tmp.path().to_path_buf());
    let cfg = mapped_with_tool_emitting_llm(tmp.path().to_path_buf())
        .with_tool_dispatcher(dispatcher, Arc::new(CompositeGate::new()));
    let adapter = VacCommandExecutorAdapter::new(cfg);
    let path = tokio::task::spawn_blocking(move || {
        adapter.execute(&cmd("memorize", "/memorize")).unwrap();
        adapter.last_transcript().unwrap()
    })
    .await
    .unwrap();

    let views = read_tool_use_rows(&path).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].id, "live-1");
    assert_eq!(views[0].name, "ok_echo");
    let env = views[0].result.as_ref().expect("result row present");
    assert_eq!(env.kind, ToolResultKind::Ok);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

struct GlobEmittingLlm;

#[async_trait]
impl LlmAdapter for GlobEmittingLlm {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "smoke".into(),
            model: "smoke-1".into(),
            content: "calling glob".into(),
            input_tokens: 1,
            output_tokens: 1,
            tool_calls: vec![ToolCallRequest {
                id: "smoke-glob-1".into(),
                name: "glob".into(),
                arguments: serde_json::json!({"pattern": "*.toml"}),
                reason: None,
                estimated_tokens: 0,
            }],
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dogfood_tool_dispatch_smoke_writes_ok_tool_result_row() {
    use vac_session_engine::{
        SlashProcessor, SubmitContext, TranscriptWriter, TrivialCompactBoundary, UsageTracker,
        submit_one,
    };
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("Cargo.toml"), b"# fixture").unwrap();

    let registry = vac_tools::ToolRegistry::new();
    registry
        .register(vac_tools::builtin::glob::GlobTool)
        .await
        .unwrap();
    let registry = Arc::new(registry);
    let ctx = Arc::new(ToolContext::new(tmp.path().to_path_buf()));
    let dispatcher = Arc::new(VacToolDispatcher::new(registry, ctx));
    let gate = Arc::new(CompositeGate::new());

    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let session_id = uuid::Uuid::new_v4();
    let submit_ctx = SubmitContext::new(session_id, "smoke");
    let mut compact_cfg = vac_session_engine::CompactConfig::default();
    compact_cfg.dispatcher = Some(dispatcher);
    compact_cfg.gate = Some(gate);
    submit_one(
        submit_ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &GlobEmittingLlm,
        compact_cfg,
        None,
    )
    .await
    .unwrap();

    let path = tmp
        .path()
        .join(".vac")
        .join("sessions")
        .join(format!("{session_id}.jsonl"));
    let views = read_tool_use_rows(&path).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "glob");
    let env = views[0].result.as_ref().expect("result row");
    assert_eq!(
        env.kind,
        ToolResultKind::Ok,
        "live dispatch must produce Ok envelope: {env:?}"
    );
}

#[test]
fn replay_tolerates_missing_transcript_file() {
    let views = read_tool_use_rows(std::env::temp_dir().join("does-not-exist-d8.jsonl")).unwrap();
    assert!(views.is_empty());
}

#[test]
fn replay_tolerates_old_transcript_without_tool_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("old.jsonl");
    let body = r#"{"id":"00000000-0000-0000-0000-000000000001","session_id":"00000000-0000-0000-0000-000000000002","kind":"accepted","timestamp":"2026-04-26T00:00:00Z","content":{"input":"hi","submitted_at":"2026-04-26T00:00:00Z","metadata":null}}
{"id":"00000000-0000-0000-0000-000000000003","session_id":"00000000-0000-0000-0000-000000000002","kind":"finished","timestamp":"2026-04-26T00:00:00Z","content":{"via":"llm","usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0}}}
"#;
    std::fs::write(&path, body).unwrap();
    let views = read_tool_use_rows(&path).unwrap();
    assert!(views.is_empty());
}

#[test]
fn replay_pairs_call_and_result_in_transcript_order() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("multi.jsonl");
    let body = r#"{"id":"r1","session_id":"s","kind":"tool_call","timestamp":"t","content":{"id":"a","name":"alpha","arguments":{},"reason":null,"estimated_tokens":0}}
{"id":"r2","session_id":"s","kind":"tool_call","timestamp":"t","content":{"id":"b","name":"beta","arguments":{},"reason":null,"estimated_tokens":0}}
{"id":"r3","session_id":"s","kind":"tool_call","timestamp":"t","content":{"id":"c","name":"gamma","arguments":{},"reason":null,"estimated_tokens":0}}
{"id":"r4","session_id":"s","kind":"tool_result","timestamp":"t","content":{"id":"b","name":"beta","envelope":{"kind":"ok","payload":{"i":2},"summary":"beta ok","duration_ms":0}}}
{"id":"r5","session_id":"s","kind":"tool_result","timestamp":"t","content":{"id":"a","name":"alpha","envelope":{"kind":"ok","payload":{"i":1},"summary":"alpha ok","duration_ms":0}}}
{"id":"r6","session_id":"s","kind":"tool_result","timestamp":"t","content":{"id":"c","name":"gamma","envelope":{"kind":"error","payload":{"error":"boom"},"summary":"gamma failed","duration_ms":0}}}
"#;
    std::fs::write(&path, body).unwrap();
    let views = read_tool_use_rows(&path).unwrap();
    assert_eq!(
        views.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert_eq!(views[0].result.as_ref().unwrap().kind, ToolResultKind::Ok);
    assert_eq!(views[1].result.as_ref().unwrap().kind, ToolResultKind::Ok);
    assert_eq!(
        views[2].result.as_ref().unwrap().kind,
        ToolResultKind::Error
    );
}

#[test]
fn replay_keeps_call_when_result_row_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("partial.jsonl");
    let body = r#"{"id":"r1","session_id":"s","kind":"tool_call","timestamp":"t","content":{"id":"only","name":"alpha","arguments":{"q":"x"},"reason":null,"estimated_tokens":0}}
"#;
    std::fs::write(&path, body).unwrap();
    let views = read_tool_use_rows(&path).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].id, "only");
    assert!(views[0].result.is_none());
}

fn _suppress_unused() {
    let _ = EngineError::Other("noop".into());
}
