//! F10.1 — End-to-end integration test.
//!
//! Composes the four load-bearing crates landed in Fase 1–7 into one
//! flow so a regression in any of them fails here:
//!
//! 1. `vac_session_engine::submit_one` with a mock adapter runs a
//!    full Accepted → LlmRequest → LlmResponse → Finished transcript.
//! 2. `vac_signal::SignalBuffer` captures lines produced during the
//!    turn (simulated via `push_line`, since session-engine does not
//!    yet own tool-call execution).
//! 3. `vac_memory::Consolidator` runs with the same raw lines as
//!    input and writes a memdir file.
//! 4. `vac_tui_runtime::services::memory_banner::push_consolidation_banner`
//!    surfaces the report into a TUI banner state.
//!
//! Assertions cover every seam: transcript ends in Finished, the
//! consolidation report fired a policy, the banner queue has a
//! pending notice, and the memdir file exists on disk.

use std::sync::Arc;
use uuid::Uuid;

use async_trait::async_trait;
use vac_session_engine::{
    CompactConfig, EchoAdapter, EngineResult, LlmAdapter, LlmRequest, LlmResponse,
    SlashProcessor, SubmitContext, TranscriptKind, TranscriptWriter,
    TrivialCompactBoundary, UsageTracker, submit_one,
};

use vac_memory::{
    ConsolidatorConfig, MemoryScanner,
    consolidator::Consolidator,
    policy::{ConsolidationInput, builtin_policy_set},
};

use vac_signal::{SignalBuffer, buffer::SignalStreamKind};

use vac_tui_runtime::app::types::BannerState;
use vac_tui_runtime::services::memory_banner::push_consolidation_banner;

// F10.1 reuses vac_session_engine::EchoAdapter for the mock LLM
// seam — no need for a local duplicate adapter. Any F2 regression to
// EchoAdapter's contract fails this test the same way it fails the
// engine's own suite, by design.

#[tokio::test]
async fn vac_end_to_end_boot_through_consolidation() {
    // ── Shared project root (acts as both transcript root and memdir) ──
    let project = tempfile::tempdir().unwrap();
    let root = project.path().to_path_buf();

    // ── 1. Session engine: drive one submit ──
    let writer = TranscriptWriter::new(root.clone());
    let ctx = SubmitContext::new(Uuid::new_v4(), "learn: use nextest, never cargo test");
    let sid = ctx.session_id;
    let snap = submit_one(
        ctx,
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &EchoAdapter,
        CompactConfig::default(),
        None,
    )
    .await
    .expect("submit_one completes");
    assert!(snap.total_tokens() > 0);

    // Transcript assertions — the durability contract must hold end-to-end.
    let rows = writer.read(sid).await.unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
    assert_eq!(kinds.first(), Some(&TranscriptKind::Accepted));
    assert_eq!(kinds.last(), Some(&TranscriptKind::Finished));
    assert!(kinds.contains(&TranscriptKind::LlmRequest));
    assert!(kinds.contains(&TranscriptKind::LlmResponse));

    // ── 2. Signal buffer: capture tool-call stream with lines a
    //       builtin consolidator policy will recognize. The buffer
    //       is the seam between runtime output and memory — the
    //       next step *feeds the buffer's contents directly into
    //       the consolidator* so a shape regression on either side
    //       fails this test.
    let mut buf = SignalBuffer::new(SignalStreamKind::RuntimeJob, 128);
    buf.push_line("learn: use nextest, never cargo test");
    buf.push_line("runtime anomaly: ingest hit OOM during build");
    assert_eq!(buf.len(), 2);

    // ── 3. Memory consolidator consumes the signal buffer ──
    let memdir_root = root.join(".vac").join("memory");
    let scanner = MemoryScanner::new(memdir_root.clone());
    scanner.ensure_layout().await.unwrap();
    let consolidator = Consolidator::new(
        scanner.clone(),
        ConsolidatorConfig {
            min_session_count: 0,
            min_interval: std::time::Duration::ZERO,
            stale_lock_after: std::time::Duration::from_secs(60),
        },
    );
    // Wire the signal buffer directly into the consolidator so the
    // e2e actually exercises the seam between runtime output and
    // memory. A regression that changes either surface breaks here.
    let raw_lines: Vec<String> = buf.iter().map(|e| e.text.clone()).collect();
    let input = ConsolidationInput {
        raw_lines,
        session_count: 5,
    };
    let report = consolidator
        .run_once(&builtin_policy_set(), &input)
        .await
        .expect("consolidator cycle");
    assert!(
        !report.was_skipped(),
        "consolidator must not skip; gate reason: {:?}",
        report.skipped_reason,
    );
    assert!(
        !report.files_written.is_empty(),
        "at least one policy must fire",
    );
    // Memdir files must actually exist on disk under the active shelf.
    let all_mem = scanner.scan_all().await.unwrap();
    assert!(
        !all_mem.is_empty(),
        "memdir scan must see at least one consolidator output",
    );

    // ── 4. TUI banner: bridge the report into operator-visible state ──
    let mut banner = BannerState::default();
    push_consolidation_banner(&mut banner, &report);
    let visible = banner.queue.current();
    assert!(
        visible.is_some(),
        "consolidator report must surface into the banner queue",
    );
}

/// Slash short-circuit variant of the end-to-end flow: the engine
/// never touches the mock adapter, the transcript still ends
/// Finished, and nothing downstream breaks.
#[tokio::test]
async fn vac_end_to_end_slash_short_circuit_stays_clean() {
    use vac_session_engine::SlashCommand;

    struct Pong;
    #[async_trait]
    impl SlashCommand for Pong {
        fn name(&self) -> &str {
            "ping"
        }
        fn description(&self) -> &str {
            "e2e slash"
        }
        async fn handle(
            &self,
            _args: &str,
        ) -> EngineResult<vac_session_engine::slash::SlashResult> {
            Ok(vac_session_engine::slash::SlashResult {
                summary: "pong".into(),
                payload: serde_json::json!({ "ok": true }),
            })
        }
    }

    let project = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(project.path().to_path_buf());
    let mut slash = SlashProcessor::new();
    slash.register(Arc::new(Pong));
    let ctx = SubmitContext::new(Uuid::new_v4(), "/ping");
    let sid = ctx.session_id;
    submit_one(
        ctx,
        &writer,
        &slash,
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        // Use an adapter that would error if called.
        &FailingAdapter,
        CompactConfig::default(),
        None,
    )
    .await
    .expect("slash path must not reach the adapter");

    let rows = writer.read(sid).await.unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&TranscriptKind::Slash));
    assert!(!kinds.contains(&TranscriptKind::LlmRequest));
    assert_eq!(kinds.last(), Some(&TranscriptKind::Finished));
}

struct FailingAdapter;
#[async_trait]
impl LlmAdapter for FailingAdapter {
    async fn complete(&self, _: LlmRequest) -> EngineResult<LlmResponse> {
        Err(vac_session_engine::EngineError::Other(
            "slash path must not touch the adapter".into(),
        ))
    }
}
