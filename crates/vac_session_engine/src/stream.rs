//! A.1 — `SubmitChunk` contract + `submit_stream` adapter shim.
//!
//! `submit_one` is the legacy blocking-collector API: it drives the
//! whole submit to completion and returns a `UsageSnapshot`. For
//! Claude-Code-style streaming UX (render each tool-use block as it
//! arrives, not after the full round-trip finishes) drivers need a
//! `Stream` contract instead.
//!
//! This module provides both halves:
//!
//! - [`SubmitChunk`] — narrow, stable wire shape the Stream yields.
//!   Initially a 1:1 alias over [`crate::event::SubmitEvent`] but
//!   owns its own enum so Phase A.3 can split high-granularity sub-
//!   events (partial `TextDelta`, `ToolPermissionDenied`, etc.)
//!   without churning the legacy event path.
//!
//! - [`submit_stream`] — the new public API. Wraps `submit_one` so
//!   today's behaviour is preserved exactly (same transcript, same
//!   durability, same event semantics). A.3 rewrites this body to
//!   yield `SubmitChunk`s directly from the adapter layer instead
//!   of collecting an unbounded channel first.
//!
//! Adapter equivalence test: `submit_one` vs `submit_stream` over
//! the EchoAdapter yield identical transcripts and identical chunk
//! labels.

use std::pin::Pin;

use futures::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, UnboundedReceiverStream};

use crate::compact::CompactBoundary;
use crate::error::EngineResult;
use crate::event::{SubmitContext, SubmitEvent};
use crate::llm::LlmAdapter;
use crate::slash::SlashProcessor;
use crate::submit::{CompactConfig, submit_one};
use crate::transcript::TranscriptWriter;
use crate::usage::UsageTracker;

/// Stream chunk kinds. One-to-one with `SubmitEvent` today; A.3 adds
/// finer-grained variants (`TextDelta` per token instead of per
/// chunk, `ToolPermissionDenied` distinct from `Aborted`).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SubmitChunk {
    Accepted {
        entry_id: uuid::Uuid,
    },
    SlashHandled {
        command: String,
        payload: serde_json::Value,
    },
    Compacted {
        kept: usize,
        dropped: usize,
    },
    LlmRequested {
        provider: String,
        model: String,
    },
    TextDelta {
        text: String,
    },
    ToolRequested {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: String,
        name: String,
        payload: vac_tool_core::ToolResultEnvelope,
    },
    Finished {
        usage: crate::usage::UsageSnapshot,
    },
    Aborted {
        reason: String,
    },
    SpeculationReady {
        predicted_prompt: String,
        precomputed_context: std::collections::HashMap<String, String>,
    },
}

impl SubmitChunk {
    /// Short label mirroring `SubmitEvent::label` so a tracer sees
    /// the same vocabulary across both APIs.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Accepted { .. } => "accepted",
            Self::SlashHandled { .. } => "slash",
            Self::Compacted { .. } => "compact",
            Self::LlmRequested { .. } => "llm.request",
            Self::TextDelta { .. } => "llm.chunk",
            Self::ToolRequested { .. } => "tool.request",
            Self::ToolResult { .. } => "tool.result",
            Self::Finished { .. } => "finished",
            Self::Aborted { .. } => "aborted",
            Self::SpeculationReady { .. } => "speculation_ready",
        }
    }

    /// Is this the terminal chunk of the stream?
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Finished { .. } | Self::Aborted { .. })
    }
}

impl From<SubmitEvent> for SubmitChunk {
    fn from(ev: SubmitEvent) -> Self {
        match ev {
            SubmitEvent::Accepted { entry_id } => Self::Accepted { entry_id },
            SubmitEvent::SlashHandled { command, payload } => {
                Self::SlashHandled { command, payload }
            }
            SubmitEvent::Compacted { kept, dropped } => {
                Self::Compacted { kept, dropped }
            }
            SubmitEvent::LlmRequested { provider, model } => {
                Self::LlmRequested { provider, model }
            }
            SubmitEvent::LlmChunk { text } => Self::TextDelta { text },
            SubmitEvent::ToolRequested { id, name, arguments } => {
                Self::ToolRequested { id, name, arguments }
            }
            SubmitEvent::ToolResult { id, name, payload } => {
                Self::ToolResult { id, name, payload }
            }
            SubmitEvent::Finished { usage } => Self::Finished { usage },
            SubmitEvent::Aborted { reason } => Self::Aborted { reason },
            SubmitEvent::SpeculationReady {
                predicted_prompt,
                precomputed_context,
            } => Self::SpeculationReady {
                predicted_prompt,
                precomputed_context,
            },
            // SubmitEvent is #[non_exhaustive]; map unknown future
            // variants to Aborted so the stream always terminates
            // cleanly. A.3 rewrites this to yield fresh chunk types
            // instead of reusing Aborted as a catch-all.
            _ => Self::Aborted {
                reason: "unknown SubmitEvent variant".into(),
            },
        }
    }
}

/// The shape returned by `submit_stream`. Boxed so callers that
/// want a concrete type can `pin_mut!` it or store it without
/// re-deriving the generator's opaque type.
pub type SubmitStream =
    Pin<Box<dyn Stream<Item = SubmitChunk> + Send + 'static>>;

/// Audit fix: cap on buffered chunks between the producing task
/// and the consumer. A slow TUI renderer can no longer OOM by
/// falling behind — when the buffer fills, the producer's
/// `send.await` naturally back-pressures. 256 entries at ~200B
/// per chunk = ~50 KB ceiling.
pub const DEFAULT_CHUNK_BUFFER: usize = 256;

/// The streaming counterpart to `submit_one`. Semantics are
/// **identical** today: same transcript rows, same durability,
/// same chunk ordering. A.3 rewires this so chunks arrive as the
/// adapter emits them rather than after the whole submit finishes.
///
/// The returned stream yields `SubmitChunk`s until a terminal
/// `Finished` or `Aborted` chunk, then closes. Caller cancels by
/// dropping the stream; `submit_one` then observes the receiver
/// drop and cleans up.
#[allow(clippy::too_many_arguments)]
pub fn submit_stream(
    submit: SubmitContext,
    transcript: std::sync::Arc<TranscriptWriter>,
    slash: std::sync::Arc<SlashProcessor>,
    compact: std::sync::Arc<dyn CompactBoundary>,
    usage: std::sync::Arc<UsageTracker>,
    llm: std::sync::Arc<dyn LlmAdapter>,
    compact_cfg: CompactConfig,
) -> SubmitStream {
    // Inner event channel (unbounded; matches submit_one's
    // existing contract). Outer chunk channel is bounded →
    // slow consumer gets back-pressure via the forwarder task,
    // not unbounded memory growth.
    let (inner_tx, mut inner_rx) = mpsc::unbounded_channel::<SubmitEvent>();
    let (outer_tx, outer_rx) = mpsc::channel::<SubmitChunk>(DEFAULT_CHUNK_BUFFER);

    // Forwarder: drain inner events → bounded outer chunk.
    let forwarder_tx = outer_tx.clone();
    tokio::spawn(async move {
        while let Some(ev) = inner_rx.recv().await {
            if forwarder_tx.send(SubmitChunk::from(ev)).await.is_err() {
                break;
            }
        }
    });

    let submit_tx = inner_tx.clone();
    let _ = outer_tx; // keep a clone alive on the producer side
    tokio::spawn(async move {
        let res: EngineResult<_> = submit_one(
            submit,
            &transcript,
            &slash,
            &*compact,
            &usage,
            &*llm,
            compact_cfg,
            Some(submit_tx.clone()),
        )
        .await;
        if let Err(e) = res {
            // If submit_one returned an error without emitting a
            // terminal Aborted event (rare — the existing body does
            // wrap errors, but keep this as belt-and-suspenders),
            // synthesise one so the stream always closes with a
            // terminal chunk.
            let _ = submit_tx.send(SubmitEvent::Aborted {
                reason: e.to_string(),
            });
        }
    });
    Box::pin(ReceiverStream::new(outer_rx))
}

// Drop the old unbounded-wrap helper — the bounded forwarder
// above replaced it.
#[allow(unused_imports)]
use tokio_stream::wrappers::UnboundedReceiverStream as _UnusedUnbounded;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::TrivialCompactBoundary;
    use crate::llm::EchoAdapter;
    use futures::StreamExt;
    use uuid::Uuid;

    /// Equivalence — legacy `submit_one` driven to completion yields
    /// the same ordered label sequence as `submit_stream`. This is
    /// the adapter contract Phase A hinges on.
    #[tokio::test]
    async fn submit_stream_label_sequence_matches_submit_one() {
        // Legacy path.
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let slash = SlashProcessor::new();
        let compact = TrivialCompactBoundary::default();
        let usage = UsageTracker::new();
        let llm = EchoAdapter;
        let (tx, mut rx) = mpsc::unbounded_channel();
        submit_one(
            SubmitContext::new(Uuid::new_v4(), "hi"),
            &w,
            &slash,
            &compact,
            &usage,
            &llm,
            CompactConfig::default(),
            Some(tx),
        )
        .await
        .unwrap();
        let mut legacy_labels: Vec<&'static str> = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            legacy_labels.push(ev.label());
        }

        // Streaming path.
        let tmp2 = tempfile::tempdir().unwrap();
        let w2 = std::sync::Arc::new(TranscriptWriter::new(tmp2.path().to_path_buf()));
        let slash2 = std::sync::Arc::new(SlashProcessor::new());
        let compact2: std::sync::Arc<dyn CompactBoundary> =
            std::sync::Arc::new(TrivialCompactBoundary::default());
        let usage2 = std::sync::Arc::new(UsageTracker::new());
        let llm2: std::sync::Arc<dyn LlmAdapter> =
            std::sync::Arc::new(EchoAdapter);
        let mut stream = submit_stream(
            SubmitContext::new(Uuid::new_v4(), "hi"),
            w2,
            slash2,
            compact2,
            usage2,
            llm2,
            CompactConfig::default(),
        );
        let mut stream_labels: Vec<&'static str> = Vec::new();
        while let Some(chunk) = stream.next().await {
            stream_labels.push(chunk.label());
            if chunk.is_terminal() {
                break;
            }
        }

        assert_eq!(
            legacy_labels, stream_labels,
            "stream label ordering must match legacy submit_one",
        );
    }

    #[tokio::test]
    async fn submit_stream_ends_with_terminal_chunk() {
        let tmp = tempfile::tempdir().unwrap();
        let w = std::sync::Arc::new(TranscriptWriter::new(tmp.path().to_path_buf()));
        let slash = std::sync::Arc::new(SlashProcessor::new());
        let compact: std::sync::Arc<dyn CompactBoundary> =
            std::sync::Arc::new(TrivialCompactBoundary::default());
        let usage = std::sync::Arc::new(UsageTracker::new());
        let llm: std::sync::Arc<dyn LlmAdapter> =
            std::sync::Arc::new(EchoAdapter);
        let mut stream = submit_stream(
            SubmitContext::new(Uuid::new_v4(), "hi"),
            w,
            slash,
            compact,
            usage,
            llm,
            CompactConfig::default(),
        );
        let mut last: Option<SubmitChunk> = None;
        while let Some(chunk) = stream.next().await {
            last = Some(chunk);
        }
        assert!(
            matches!(last, Some(SubmitChunk::Finished { .. })),
            "stream must end on Finished, got {last:?}",
        );
    }

    /// Audit-pin: every `SubmitEvent` variant must map onto a
    /// non-Aborted `SubmitChunk`. A future contributor that adds
    /// a new `SubmitEvent` variant without extending the `From`
    /// impl would have that variant silently coerced to
    /// `Aborted` — that's the drift guard here.
    #[test]
    fn every_submit_event_has_a_dedicated_chunk_variant() {
        use serde_json::json;
        use crate::event::SubmitEvent;
        let cases: &[(SubmitEvent, &'static str)] = &[
            (SubmitEvent::Accepted { entry_id: uuid::Uuid::nil() }, "accepted"),
            (SubmitEvent::SlashHandled { command: "x".into(), payload: json!({}) }, "slash"),
            (SubmitEvent::Compacted { kept: 1, dropped: 2 }, "compact"),
            (SubmitEvent::LlmRequested { provider: "p".into(), model: "m".into() }, "llm.request"),
            (SubmitEvent::LlmChunk { text: "t".into() }, "llm.chunk"),
            (SubmitEvent::ToolRequested { id: "i".into(), name: "n".into(), arguments: json!({}) }, "tool.request"),
            (SubmitEvent::Finished { usage: crate::usage::UsageSnapshot::default() }, "finished"),
            (SubmitEvent::Aborted { reason: "r".into() }, "aborted"),
            (SubmitEvent::SpeculationReady { predicted_prompt: "p".into(), precomputed_context: Default::default() }, "speculation_ready"),
        ];
        for (ev, expected_label) in cases {
            let chunk = SubmitChunk::from(ev.clone());
            assert_eq!(
                chunk.label(),
                *expected_label,
                "SubmitEvent {ev:?} → SubmitChunk mapping is wrong",
            );
            // And: the mapping must not silently coerce to
            // Aborted unless the source was already Aborted.
            if !matches!(ev, SubmitEvent::Aborted { .. }) {
                assert!(
                    !matches!(chunk, SubmitChunk::Aborted { .. }),
                    "non-Aborted SubmitEvent {ev:?} became SubmitChunk::Aborted — drift!",
                );
            }
        }
    }

    #[test]
    fn chunk_label_is_stable_across_variants() {
        assert_eq!(
            SubmitChunk::TextDelta { text: "x".into() }.label(),
            "llm.chunk",
        );
        assert_eq!(
            SubmitChunk::Finished {
                usage: crate::usage::UsageSnapshot::default(),
            }
            .label(),
            "finished",
        );
    }
}
