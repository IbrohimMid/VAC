//! D10 — live activity feed bridge contract tests.

use futures::stream;
use vac_session_engine::stream::SubmitChunk;
use vac_session_engine::usage::UsageSnapshot;
use vac_shell_contracts::ShellActivityKind;
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_event_projection::spawn_activity_feed_bridge;

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
            payload: vac_tool_core::ToolResultEnvelope::ok(
                "ok output",
                serde_json::json!({}),
            ),
        },
        SubmitChunk::Finished {
            usage: UsageSnapshot::default(),
        },
    ];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s1".into());
    handle.await.unwrap();

    let snap = log.snapshot();
    assert!(
        snap.iter().any(|e| e.kind == ShellActivityKind::ToolCall),
        "expected ToolCall entry, got: {snap:?}"
    );
    assert!(
        snap.iter().any(|e| e.kind == ShellActivityKind::ToolResult),
        "expected ToolResult entry, got: {snap:?}"
    );
}

#[tokio::test]
async fn bridge_records_aborted_as_error_entry() {
    let log = ActivityLog::default();
    let chunks = vec![SubmitChunk::Aborted {
        reason: "timeout".into(),
    }];

    let handle = spawn_activity_feed_bridge(make_stream(chunks), log.clone(), "s2".into());
    handle.await.unwrap();

    let snap = log.snapshot();
    assert!(
        snap.iter().any(|e| e.kind == ShellActivityKind::Error),
        "expected Error entry, got: {snap:?}"
    );
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
    // Only Finished is bridged (as AgentThoughtSummary). TextDelta is skipped.
    assert_eq!(snap.len(), 1, "expected exactly 1 entry (Finished), got: {snap:?}");
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
