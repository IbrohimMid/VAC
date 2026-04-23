//! Compact boundary — policy hook that decides whether the engine
//! should compact (summarise/drop) messages before an LLM call.
//!
//! The engine doesn't own the compaction algorithm — different drivers
//! plug in different strategies (drop-oldest, semantic merge, external
//! summariser). This module provides the trait + a trivial reference
//! impl.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::EngineResult;

/// Recommendation returned by a [`CompactBoundary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactHint {
    /// No action needed; pass messages through.
    Keep,
    /// Drop the oldest `n` messages before the LLM call.
    DropOldest { n: usize },
    /// Driver should replace oldest `n` with a summary message.
    /// `summary` is the replacement text.
    Summarise { n: usize, summary: String },
}

/// Input to the boundary decision. Intentionally small — drivers can
/// extend with their own state if they need richer signals.
#[derive(Debug, Clone)]
pub struct CompactInput {
    /// Current conversation length in messages (user + assistant).
    pub message_count: usize,
    /// Approximate token count of the current conversation.
    pub approx_tokens: u64,
    /// Model's context-window size in tokens.
    pub context_window_tokens: u64,
}

#[async_trait]
pub trait CompactBoundary: Send + Sync {
    /// Decide whether + how to compact. Return `Keep` if no action
    /// needed. Pure: must not mutate anything.
    async fn decide(&self, input: &CompactInput) -> EngineResult<CompactHint>;
}

/// Reference implementation: drops the oldest half of messages when
/// `approx_tokens` exceeds 80% of the context window. Good enough for
/// CLI/headless; TUI may install something smarter.
#[derive(Debug, Default)]
pub struct TrivialCompactBoundary {
    /// Threshold fraction in [0.0, 1.0]. Defaults to 0.8.
    pub threshold: f32,
}

impl TrivialCompactBoundary {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold: threshold.clamp(0.05, 0.99),
        }
    }
}

#[async_trait]
impl CompactBoundary for TrivialCompactBoundary {
    async fn decide(&self, input: &CompactInput) -> EngineResult<CompactHint> {
        let threshold = if self.threshold <= 0.0 {
            0.8
        } else {
            self.threshold
        };
        let budget = (input.context_window_tokens as f32 * threshold) as u64;
        if input.approx_tokens <= budget {
            return Ok(CompactHint::Keep);
        }
        // Drop half, minimum 1. Keep recent.
        let n = (input.message_count / 2).max(1);
        Ok(CompactHint::DropOldest { n })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(msgs: usize, tokens: u64, window: u64) -> CompactInput {
        CompactInput {
            message_count: msgs,
            approx_tokens: tokens,
            context_window_tokens: window,
        }
    }

    #[tokio::test]
    async fn trivial_keeps_when_under_threshold() {
        let b = TrivialCompactBoundary::default();
        assert_eq!(b.decide(&ctx(10, 1000, 8000)).await.unwrap(), CompactHint::Keep);
    }

    #[tokio::test]
    async fn trivial_drops_half_when_over_threshold() {
        let b = TrivialCompactBoundary::default();
        // 8000 * 0.8 = 6400; 7000 > 6400 → drop.
        let hint = b.decide(&ctx(20, 7000, 8000)).await.unwrap();
        match hint {
            CompactHint::DropOldest { n } => assert_eq!(n, 10),
            other => panic!("expected DropOldest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn trivial_threshold_clamps() {
        let b = TrivialCompactBoundary::new(2.0);
        assert!(b.threshold <= 0.99);
        let b = TrivialCompactBoundary::new(-1.0);
        assert!(b.threshold >= 0.05);
    }

    #[tokio::test]
    async fn trivial_drops_at_least_one() {
        let b = TrivialCompactBoundary::default();
        // 1 message, tiny window. Should still recommend drop with n >= 1.
        let hint = b.decide(&ctx(1, 1000, 100)).await.unwrap();
        match hint {
            CompactHint::DropOldest { n } => assert!(n >= 1),
            other => panic!("expected DropOldest, got {other:?}"),
        }
    }

    #[test]
    fn compact_hint_roundtrips_through_json() {
        let h = CompactHint::Summarise {
            n: 5,
            summary: "prev work done".into(),
        };
        let s = serde_json::to_string(&h).unwrap();
        let back: CompactHint = serde_json::from_str(&s).unwrap();
        assert_eq!(back, h);
    }
}
