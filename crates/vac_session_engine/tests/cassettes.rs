//! NS.5 — `CassetteAdapter` replay regression.
//!
//! Drives one full `submit_one` per provider cassette, asserting
//! that the recorded wire shape round-trips through the engine's
//! transcript durability invariant. If a provider adds a new
//! field to `LlmResponse` or `ToolCallRequest` and the cassette
//! hasn't been re-recorded, the JSON parse fails loudly rather
//! than silently replaying stale data.

use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use vac_session_engine::{
    CassetteAdapter, CompactConfig, LlmAdapter, LlmRequest, SlashProcessor, SubmitContext,
    SubmitEvent, TranscriptKind, TranscriptWriter, TrivialCompactBoundary, UsageTracker,
    submit_one,
};

fn cassette_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cassettes")
        .join(format!("{name}.json"))
}

async fn assert_cassette_submits_cleanly(provider: &str) {
    let cassette = CassetteAdapter::from_file(cassette_path(provider))
        .unwrap_or_else(|e| panic!("load {provider} cassette: {e}"));
    assert_eq!(cassette.provider(), provider);
    assert!(!cassette.is_empty());

    // Direct complete check: the "hello" prompt must round-trip.
    let resp = cassette
        .complete(LlmRequest {
            prompt: "hello".into(),
            context: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(resp.provider, provider);
    assert!(!resp.content.is_empty());

    // Drive through submit_one so transcript durability is exercised.
    let tmp = tempfile::tempdir().unwrap();
    let w = TranscriptWriter::new(tmp.path().to_path_buf());
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SubmitEvent>();
    submit_one(
        SubmitContext::new(Uuid::new_v4(), "hello"),
        &w,
        &slash,
        &compact,
        &usage,
        &cassette,
        CompactConfig::default(),
        Some(tx),
    )
    .await
    .unwrap();

    let mut saw_terminal = false;
    while let Ok(ev) = rx.try_recv() {
        if matches!(
            ev,
            SubmitEvent::Finished { .. } | SubmitEvent::Aborted { .. }
        ) {
            saw_terminal = true;
        }
    }
    assert!(
        saw_terminal,
        "{provider} cassette must end on Finished/Aborted"
    );
}

#[tokio::test]
async fn anthropic_cassette_round_trips() {
    assert_cassette_submits_cleanly("anthropic").await;
}

#[tokio::test]
async fn openai_cassette_round_trips() {
    assert_cassette_submits_cleanly("openai").await;
}

#[tokio::test]
async fn gemini_cassette_round_trips() {
    assert_cassette_submits_cleanly("gemini").await;
}

#[tokio::test]
async fn xai_cassette_round_trips() {
    assert_cassette_submits_cleanly("xai").await;
}

/// Every cassette must surface a tool_calls shape for the
/// "list_files" prompt — that's the drift guard. A provider whose
/// tool_call deserialisation breaks (new required field, removed
/// optional) fails this test loudly rather than silently replaying
/// an empty list.
#[tokio::test]
async fn every_cassette_has_tool_call_variant() {
    for provider in ["anthropic", "openai", "gemini", "xai"] {
        let c = CassetteAdapter::from_file(cassette_path(provider)).unwrap();
        let resp = c
            .complete(LlmRequest {
                prompt: "list the files in the current directory".into(),
                context: Vec::new(),
            })
            .await
            .unwrap();
        assert!(
            !resp.tool_calls.is_empty(),
            "{provider} cassette missing tool_call for list_files prompt",
        );
        assert_eq!(resp.tool_calls[0].name, "list_files");
    }
}

#[tokio::test]
async fn cassette_miss_returns_loud_error() {
    let c = CassetteAdapter::from_file(cassette_path("anthropic")).unwrap();
    let err = c
        .complete(LlmRequest {
            prompt: "prompt that was never recorded".into(),
            context: Vec::new(),
        })
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("cassette miss"), "{err}");
}

#[test]
fn cassette_rejects_malformed_json() {
    let bad = r#"{ "provider": "x", "model": "y" }"#;
    assert!(CassetteAdapter::from_str(bad).is_err());
}
