//! Pin the on-disk JSONL schema documented in
//! `docs/runtime-integration/TRANSCRIPT_SCHEMA.md`. If a future
//! engine change adds, removes, or renames a `content` field, this
//! test fails — the operator-facing schema doc and the engine MUST
//! drift together.

mod common;

use async_trait::async_trait;
use common::{ToolEmittingAdapter, call};
use std::sync::Arc;
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EngineError, EngineResult, SlashCommand, SlashProcessor, SubmitContext,
    TranscriptWriter, TrivialCompactBoundary, UsageTracker, submit_one,
};
use vac_shell_test_support::read_jsonl_rows;

fn keys(v: &serde_json::Value) -> Vec<String> {
    v.as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

fn find_one<'a>(rows: &'a [serde_json::Value], kind: &str) -> &'a serde_json::Value {
    rows.iter()
        .find(|r| r["kind"] == kind)
        .unwrap_or_else(|| panic!("missing row of kind `{kind}`: {rows:?}"))
}

// ---------------------------------------------------------------------
// LLM adapter + slash command stubs.
// ---------------------------------------------------------------------

struct PingSlash;

#[async_trait]
impl SlashCommand for PingSlash {
    fn name(&self) -> &str {
        "ping"
    }
    fn description(&self) -> &str {
        "schema-test slash"
    }
    async fn handle(&self, _args: &str) -> EngineResult<vac_session_engine::slash::SlashResult> {
        Ok(vac_session_engine::slash::SlashResult {
            summary: "pong".into(),
            payload: serde_json::json!({ "pong": true }),
        })
    }
}

// ---------------------------------------------------------------------
// 1. LLM-path schema: accepted, llm_request, llm_response, tool_call,
//    tool_result, finished.
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn llm_path_row_shapes_match_schema_doc() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let session_id = Uuid::new_v4();
    let ctx = SubmitContext::new(session_id, "schema check");
    let llm = ToolEmittingAdapter {
        tool_calls: vec![call("tc-1", "search", serde_json::json!({"q": "x"}))],
    };
    submit_one(
        ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &llm,
        CompactConfig::default(),
        None,
    )
    .await
    .unwrap();
    let path = tmp
        .path()
        .join(".vac")
        .join("sessions")
        .join(format!("{session_id}.jsonl"));
    let rows = read_jsonl_rows(&path);

    // Wrapper shape: every row must have the documented top-level keys.
    for row in &rows {
        let row_keys = keys(row);
        for required in ["id", "session_id", "kind", "timestamp", "content"] {
            assert!(
                row_keys.iter().any(|k| k == required),
                "row missing top-level key `{required}`: {row}"
            );
        }
    }

    // accepted
    let accepted = find_one(&rows, "accepted");
    let acc = &accepted["content"];
    let acc_keys: std::collections::HashSet<_> = keys(acc).into_iter().collect();
    let expected: std::collections::HashSet<_> = ["input", "submitted_at", "metadata"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(acc_keys, expected, "accepted content keys: {acc}");

    // llm_request
    let llm_req = find_one(&rows, "llm_request");
    let req_keys: std::collections::HashSet<_> = keys(&llm_req["content"]).into_iter().collect();
    assert_eq!(
        req_keys,
        ["prompt"].into_iter().map(String::from).collect(),
        "llm_request content: {}",
        llm_req["content"]
    );

    // llm_response
    let llm_resp = find_one(&rows, "llm_response");
    let resp_keys: std::collections::HashSet<_> = keys(&llm_resp["content"]).into_iter().collect();
    let expected_resp: std::collections::HashSet<_> = [
        "provider",
        "model",
        "content",
        "input_tokens",
        "output_tokens",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(resp_keys, expected_resp, "llm_response content");

    // tool_call
    let tc = find_one(&rows, "tool_call");
    let tc_keys: std::collections::HashSet<_> = keys(&tc["content"]).into_iter().collect();
    let expected_tc: std::collections::HashSet<_> =
        ["id", "name", "arguments", "reason", "estimated_tokens"]
            .into_iter()
            .map(String::from)
            .collect();
    assert_eq!(tc_keys, expected_tc, "tool_call content");

    // tool_result
    let tr = find_one(&rows, "tool_result");
    let tr_keys: std::collections::HashSet<_> = keys(&tr["content"]).into_iter().collect();
    let expected_tr: std::collections::HashSet<_> = ["id", "name", "envelope"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(tr_keys, expected_tr, "tool_result content");
    let env = &tr["content"]["envelope"];
    let env_keys: std::collections::HashSet<_> = keys(env).into_iter().collect();
    let expected_env: std::collections::HashSet<_> = ["kind", "payload", "summary", "duration_ms"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(env_keys, expected_env, "tool_result.envelope keys");

    // finished
    let fin = find_one(&rows, "finished");
    let fin_keys: std::collections::HashSet<_> = keys(&fin["content"]).into_iter().collect();
    let expected_fin: std::collections::HashSet<_> =
        ["via", "usage"].into_iter().map(String::from).collect();
    assert_eq!(fin_keys, expected_fin, "finished content");
    assert_eq!(fin["content"]["via"], "llm");
}

// ---------------------------------------------------------------------
// 2. Slash-path: slash content keys + finished `via` = "slash".
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slash_path_row_shapes_match_schema_doc() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let mut slash = SlashProcessor::new();
    slash.register(Arc::new(PingSlash));
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let session_id = Uuid::new_v4();
    let ctx = SubmitContext::new(session_id, "/ping hello");
    let llm = ToolEmittingAdapter { tool_calls: vec![] };
    submit_one(
        ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &llm,
        CompactConfig::default(),
        None,
    )
    .await
    .unwrap();
    let rows = read_jsonl_rows(
        &tmp.path()
            .join(".vac")
            .join("sessions")
            .join(format!("{session_id}.jsonl")),
    );

    let s = find_one(&rows, "slash");
    let s_keys: std::collections::HashSet<_> = keys(&s["content"]).into_iter().collect();
    let expected_s: std::collections::HashSet<_> = ["command", "args", "summary", "payload"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(s_keys, expected_s, "slash content");
    assert_eq!(s["content"]["command"], "ping");

    let fin = find_one(&rows, "finished");
    assert_eq!(fin["content"]["via"], "slash");
}

// ---------------------------------------------------------------------
// 3. Aborted shape — `reason` + `kind` discriminator.
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborted_row_shape_includes_reason_and_kind() {
    // Force the BudgetExceeded path: any positive max_budget_tokens
    // with prior usage above it triggers the aborted branch.
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    // Pre-load the usage tracker over the budget so the engine
    // aborts before contacting the LLM.
    usage.add_input_tokens(1_000);
    usage.add_output_tokens(0);
    let session_id = Uuid::new_v4();
    let ctx = SubmitContext::new(session_id, "force abort");
    let llm = ToolEmittingAdapter { tool_calls: vec![] };
    let cfg = CompactConfig {
        max_budget_tokens: Some(1),
        ..CompactConfig::default()
    };
    let res = submit_one(ctx, &writer, &slash, &compact, &usage, &llm, cfg, None).await;
    assert!(matches!(res, Err(EngineError::BudgetExceeded { .. })));

    let rows = read_jsonl_rows(
        &tmp.path()
            .join(".vac")
            .join("sessions")
            .join(format!("{session_id}.jsonl")),
    );
    let aborted = find_one(&rows, "aborted");
    let a_keys: std::collections::HashSet<_> = keys(&aborted["content"]).into_iter().collect();
    let expected_a: std::collections::HashSet<_> =
        ["reason", "kind"].into_iter().map(String::from).collect();
    assert_eq!(a_keys, expected_a, "aborted content");
    assert_eq!(aborted["content"]["kind"], "budget_exceeded");
}
