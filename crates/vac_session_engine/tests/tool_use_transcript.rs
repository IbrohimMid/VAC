mod common;

use async_trait::async_trait;
use common::{ToolEmittingAdapter, call};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, CompositeGate, EngineError, EngineResult, GateDecision, SlashProcessor,
    SubmitContext, ToolCallRequest, ToolCheckCtx, ToolDispatcher, TranscriptWriter,
    TrivialCompactBoundary, UsageTracker, submit_one,
};
use vac_shell_test_support::read_jsonl_rows;

async fn drive_with(
    project_root: &std::path::Path,
    adapter: ToolEmittingAdapter,
    cfg: CompactConfig,
) -> Vec<serde_json::Value> {
    let writer = TranscriptWriter::new(project_root.to_path_buf());
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let session_id = Uuid::new_v4();
    let ctx = SubmitContext::new(session_id, "drive D7E");
    submit_one(ctx, &writer, &slash, &compact, &usage, &adapter, cfg, None)
        .await
        .expect("submit_one must succeed");
    let path = project_root
        .join(".vac")
        .join("sessions")
        .join(format!("{session_id}.jsonl"));
    read_jsonl_rows(&path)
}

fn kinds_in_order(rows: &[serde_json::Value]) -> Vec<String> {
    rows.iter()
        .map(|r| r["kind"].as_str().unwrap_or("").to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsupported_dispatcher_writes_tool_call_and_tool_result_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = ToolEmittingAdapter {
        tool_calls: vec![call("t1", "search", serde_json::json!({"q": "needle"}))],
    };
    let rows = drive_with(tmp.path(), adapter, CompactConfig::default()).await;
    let kinds = kinds_in_order(&rows);

    let tool_call_idx = kinds
        .iter()
        .position(|k| k == "tool_call")
        .expect("tool_call row missing");
    let tool_result_idx = kinds
        .iter()
        .position(|k| k == "tool_result")
        .expect("tool_result row missing");
    let finished_idx = kinds
        .iter()
        .position(|k| k == "finished")
        .expect("finished row missing");

    assert!(
        tool_call_idx < tool_result_idx,
        "tool_call must precede tool_result: {kinds:?}"
    );
    assert!(
        tool_result_idx < finished_idx,
        "tool_result must precede finished: {kinds:?}"
    );

    let tool_call = &rows[tool_call_idx]["content"];
    assert_eq!(tool_call["id"], "t1");
    assert_eq!(tool_call["name"], "search");
    assert_eq!(tool_call["arguments"], serde_json::json!({"q": "needle"}));
    assert_eq!(tool_call["reason"], serde_json::Value::Null);
    assert_eq!(tool_call["estimated_tokens"], 0);

    let tool_result = &rows[tool_result_idx]["content"];
    assert_eq!(tool_result["id"], "t1");
    assert_eq!(tool_result["name"], "search");
    let envelope = &tool_result["envelope"];
    assert_eq!(envelope["kind"], "error");
    let dump = serde_json::to_string(envelope).unwrap();
    assert!(
        dump.contains("no ToolDispatcher")
            || dump.contains("cannot run tool")
            || dump.contains("dispatch error"),
        "envelope must mention the no-dispatcher / dispatch-error path: {dump}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiple_tool_calls_preserve_transcript_order() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = ToolEmittingAdapter {
        tool_calls: vec![
            call("a", "alpha", serde_json::json!({"i": 1})),
            call("b", "beta", serde_json::json!({"i": 2})),
            call("c", "gamma", serde_json::json!({"i": 3})),
        ],
    };
    let rows = drive_with(tmp.path(), adapter, CompactConfig::default()).await;

    let mut tool_call_ids = Vec::new();
    let mut tool_result_ids = Vec::new();
    for r in &rows {
        match r["kind"].as_str() {
            Some("tool_call") => {
                tool_call_ids.push(r["content"]["id"].as_str().unwrap().to_string())
            }
            Some("tool_result") => {
                tool_result_ids.push(r["content"]["id"].as_str().unwrap().to_string())
            }
            _ => {}
        }
    }
    assert_eq!(tool_call_ids, vec!["a", "b", "c"]);
    assert_eq!(tool_result_ids, vec!["a", "b", "c"]);
}

#[derive(Debug)]
struct AlwaysDenyGate;

#[async_trait]
impl vac_session_engine::ToolGate for AlwaysDenyGate {
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
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolDispatcher for CountingDispatcher {
    async fn dispatch(
        &self,
        _call: &ToolCallRequest,
    ) -> EngineResult<vac_tool_core::ToolResultEnvelope> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vac_tool_core::ToolResultEnvelope::ok(
            "ok",
            serde_json::json!({"hit": true}),
        ))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gate_deny_writes_error_tool_result_and_skips_dispatcher() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = ToolEmittingAdapter {
        tool_calls: vec![call("d1", "search", serde_json::json!({}))],
    };
    let dispatch_calls = Arc::new(AtomicUsize::new(0));
    let cfg = CompactConfig {
        gate: Some(Arc::new(
            CompositeGate::new().with_gate(Arc::new(AlwaysDenyGate)),
        )),
        dispatcher: Some(Arc::new(CountingDispatcher {
            calls: dispatch_calls.clone(),
        })),
        ..CompactConfig::default()
    };
    let rows = drive_with(tmp.path(), adapter, cfg).await;
    assert_eq!(
        dispatch_calls.load(Ordering::SeqCst),
        0,
        "gate Deny must short-circuit dispatch"
    );

    let tr = rows
        .iter()
        .find(|r| r["kind"] == "tool_result")
        .expect("tool_result row");
    let envelope = &tr["content"]["envelope"];
    assert_eq!(envelope["kind"], "error");
    let dump = serde_json::to_string(envelope).unwrap();
    assert!(
        dump.contains("blocked by gate"),
        "envelope must mention blocked-by-gate: {dump}"
    );
    assert!(
        dump.contains("policy refused"),
        "gate reason must propagate into envelope: {dump}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatcher_ok_writes_ok_tool_result_row() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = ToolEmittingAdapter {
        tool_calls: vec![call("ok-1", "echo", serde_json::json!({"x": 42}))],
    };
    let dispatch_calls = Arc::new(AtomicUsize::new(0));
    let cfg = CompactConfig {
        dispatcher: Some(Arc::new(CountingDispatcher {
            calls: dispatch_calls.clone(),
        })),
        ..CompactConfig::default()
    };
    let rows = drive_with(tmp.path(), adapter, cfg).await;
    assert_eq!(
        dispatch_calls.load(Ordering::SeqCst),
        1,
        "dispatcher must be called exactly once"
    );

    let tr = rows
        .iter()
        .find(|r| r["kind"] == "tool_result")
        .expect("tool_result row");
    let envelope = &tr["content"]["envelope"];
    assert_eq!(envelope["kind"], "ok");
    assert_eq!(envelope["payload"], serde_json::json!({"hit": true}));
}

#[derive(Debug)]
struct AlwaysErrDispatcher;

#[async_trait]
impl ToolDispatcher for AlwaysErrDispatcher {
    async fn dispatch(
        &self,
        call: &ToolCallRequest,
    ) -> EngineResult<vac_tool_core::ToolResultEnvelope> {
        Err(EngineError::Other(format!(
            "synthetic dispatcher failure for `{}`",
            call.name
        )))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_call_row_persists_when_dispatcher_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = ToolEmittingAdapter {
        tool_calls: vec![call("e1", "search", serde_json::json!({}))],
    };
    let cfg = CompactConfig {
        dispatcher: Some(Arc::new(AlwaysErrDispatcher)),
        ..CompactConfig::default()
    };
    let rows = drive_with(tmp.path(), adapter, cfg).await;
    let kinds = kinds_in_order(&rows);

    assert!(
        kinds.iter().any(|k| k == "tool_call"),
        "tool_call row must remain even when dispatcher errors: {kinds:?}"
    );
    let tr = rows
        .iter()
        .find(|r| r["kind"] == "tool_result")
        .expect("tool_result row");
    let envelope = &tr["content"]["envelope"];
    assert_eq!(envelope["kind"], "error");
    let dump = serde_json::to_string(envelope).unwrap();
    assert!(
        dump.contains("synthetic dispatcher failure"),
        "envelope must preserve the dispatcher error message: {dump}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_result_row_appears_before_finished_when_event_receiver_dropped() {
    use vac_session_engine::{
        SlashProcessor, SubmitContext, TranscriptWriter, TrivialCompactBoundary, UsageTracker,
        submit_one,
    };
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let session_id = Uuid::new_v4();
    let ctx = SubmitContext::new(session_id, "drive D7E hardening");
    let llm = ToolEmittingAdapter {
        tool_calls: vec![call("d-recv", "search", serde_json::json!({}))],
    };
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    drop(rx); // receiver gone before submit_one even starts
    submit_one(
        ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &llm,
        CompactConfig::default(),
        Some(tx),
    )
    .await
    .expect("submit_one must succeed even when the event receiver is dropped");

    let path = tmp
        .path()
        .join(".vac")
        .join("sessions")
        .join(format!("{session_id}.jsonl"));
    let rows = read_jsonl_rows(&path);
    let kinds: Vec<String> = rows
        .iter()
        .map(|r| r["kind"].as_str().unwrap_or("").to_string())
        .collect();

    let tr_idx = kinds
        .iter()
        .position(|k| k == "tool_result")
        .expect("tool_result row must persist when event receiver is dropped");
    let fin_idx = kinds
        .iter()
        .position(|k| k == "finished")
        .expect("finished row must persist when event receiver is dropped");
    assert!(
        tr_idx < fin_idx,
        "tool_result must precede finished even when nobody is listening: {kinds:?}"
    );
}
