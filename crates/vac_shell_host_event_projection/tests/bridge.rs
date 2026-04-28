//! D10 — live activity feed bridge contract tests.

use futures::stream;
use vac_session_engine::stream::SubmitChunk;
use vac_session_engine::usage::UsageSnapshot;
use vac_shell_contracts::{Severity, ShellActivityKind};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_event_projection::spawn_activity_feed_bridge;
use vac_shell_test_support::{
    assert_activity_log_contains_kind, assert_activity_log_kind_severity,
    assert_activity_log_not_contains,
};

fn make_stream(chunks: Vec<SubmitChunk>) -> vac_session_engine::stream::SubmitStream {
    Box::pin(stream::iter(chunks))
}

#[tokio::test]
async fn bridge_maps_tool_chunks_to_activity_entries() {
    let log = ActivityLog::default();
    let chunks = vec![
        SubmitChunk::ToolRequested {
            id: "t1".into(),
            name: "bash_exec".into(),
            arguments: serde_json::json!({"cmd": "ls"}),
        },
        SubmitChunk::ToolResult {
            id: "t1".into(),
            name: "bash_exec".into(),
            payload: vac_tool_core::ToolResultEnvelope::ok("ok output", serde_json::json!({})),
        },
        SubmitChunk::Finished {
            usage: UsageSnapshot::default(),
        },
    ];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s1".into());
    handle.await.unwrap();

    assert_activity_log_contains_kind(&log, ShellActivityKind::ToolCall);
    assert_activity_log_contains_kind(&log, ShellActivityKind::ToolResult);
}

#[tokio::test]
async fn bridge_records_aborted_as_error_entry() {
    let log = ActivityLog::default();
    let chunks = vec![SubmitChunk::Aborted {
        reason: "timeout".into(),
    }];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s2".into());
    handle.await.unwrap();

    assert_activity_log_contains_kind(&log, ShellActivityKind::Error);
    let snap = log.snapshot();
    assert!(
        snap.iter().any(|e| e.detail.as_deref() == Some("timeout")),
        "expected reason='timeout' in detail, got: {snap:?}"
    );
}

#[tokio::test]
async fn bridge_skips_text_delta_chunks() {
    let log = ActivityLog::default();
    let chunks = vec![
        SubmitChunk::TextDelta { text: "a".into() },
        SubmitChunk::TextDelta { text: "b".into() },
        SubmitChunk::TextDelta { text: "c".into() },
        SubmitChunk::Finished {
            usage: UsageSnapshot::default(),
        },
    ];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s3".into());
    handle.await.unwrap();

    let snap = log.snapshot();
    assert_eq!(
        snap.len(),
        1,
        "expected exactly 1 entry (Finished), got: {snap:?}"
    );
    assert_eq!(snap[0].kind, ShellActivityKind::AgentThoughtSummary);
}

#[tokio::test]
async fn bridge_maps_llm_requested_to_thought_summary() {
    let log = ActivityLog::default();
    let chunks = vec![
        SubmitChunk::LlmRequested {
            provider: "anthropic".into(),
            model: "claude-sonnet-4.5".into(),
        },
        SubmitChunk::Finished {
            usage: UsageSnapshot::default(),
        },
    ];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s4".into());
    handle.await.unwrap();

    let snap = log.snapshot();
    let thoughts: Vec<_> = snap
        .iter()
        .filter(|e| e.kind == ShellActivityKind::AgentThoughtSummary)
        .collect();
    assert!(thoughts.len() >= 1);
    assert!(
        thoughts.iter().any(|e| e.title.contains("anthropic")),
        "expected provider name in thought summary, got: {thoughts:?}"
    );
}

// D10-HARDENING: raw tool arguments must never reach the ActivityLog.
#[tokio::test]
async fn bridge_never_leaks_tool_arguments_to_log() {
    const SECRET: &str = "SUPER_SECRET_TOKEN_XYZ";
    let log = ActivityLog::default();
    let chunks = vec![
        SubmitChunk::ToolRequested {
            id: "t1".into(),
            name: "bash_exec".into(),
            arguments: serde_json::json!({"cmd": SECRET}),
        },
        SubmitChunk::Finished {
            usage: UsageSnapshot::default(),
        },
    ];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s5".into());
    handle.await.unwrap();

    assert_activity_log_not_contains(&log, SECRET);
}

// D10-HARDENING: Warning ToolResultKind → Severity::Warn (not Error).
#[tokio::test]
async fn bridge_maps_warning_result_to_warn_severity() {
    let log = ActivityLog::default();
    let chunks = vec![
        SubmitChunk::ToolRequested {
            id: "t2".into(),
            name: "scan".into(),
            arguments: serde_json::json!({}),
        },
        SubmitChunk::ToolResult {
            id: "t2".into(),
            name: "scan".into(),
            payload: vac_tool_core::ToolResultEnvelope {
                kind: vac_tool_core::ToolResultKind::Warning,
                payload: serde_json::json!({}),
                summary: "partial match".into(),
                duration_ms: 0,
            },
        },
        SubmitChunk::Finished {
            usage: UsageSnapshot::default(),
        },
    ];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s6".into());
    handle.await.unwrap();

    assert_activity_log_kind_severity(&log, ShellActivityKind::ToolResult, Severity::Warn);
}

// D10-HARDENING: Cancelled ToolResultKind → Severity::Warn (not Error).
#[tokio::test]
async fn bridge_maps_cancelled_result_to_warn_severity() {
    let log = ActivityLog::default();
    let chunks = vec![
        SubmitChunk::ToolRequested {
            id: "t3".into(),
            name: "long_op".into(),
            arguments: serde_json::json!({}),
        },
        SubmitChunk::ToolResult {
            id: "t3".into(),
            name: "long_op".into(),
            payload: vac_tool_core::ToolResultEnvelope {
                kind: vac_tool_core::ToolResultKind::Cancelled,
                payload: serde_json::json!({}),
                summary: "operator abort".into(),
                duration_ms: 0,
            },
        },
        SubmitChunk::Finished {
            usage: UsageSnapshot::default(),
        },
    ];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s7".into());
    handle.await.unwrap();

    assert_activity_log_kind_severity(&log, ShellActivityKind::ToolResult, Severity::Warn);
}
