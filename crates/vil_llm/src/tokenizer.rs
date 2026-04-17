//! Provider-aware tokenizer abstraction.
//!
//! The `Tokenizer` trait lets the context budgeter and other subsystems ask a
//! provider-specific tokenizer for message/text token counts without hardcoding
//! a BPE implementation. Two adapters ship today:
//!
//! * [`TiktokenAdapter`] — real BPE via `tiktoken_rs::cl100k_base()`. This is
//!   the default used everywhere previously and preserves the existing
//!   `+8 per-message` overhead + `1.05` safety factor logic from the Stakpak
//!   donor in `vil_swarm::context_budget`.
//! * [`HeuristicAdapter`] — cheap char/4 estimate used as a last-resort
//!   fallback and for unit tests that must compile without the tiktoken
//!   dependency on hot paths.
//!
//! A third, stubbed [`AnthropicApiTokenizer`] sits behind the
//! `anthropic-count-tokens` feature flag for the upcoming
//! `POST /v1/messages/count_tokens` integration. It is intentionally a TODO
//! skeleton so the trait surface is in place without wiring an HTTP call
//! that isn't ready to be production-correct.

use crate::provider::Message;
use once_cell::sync::Lazy;

/// Multiplier applied to raw BPE counts to leave headroom for provider-side
/// serialization overhead (role tags, JSON envelope, etc.).
pub const SAFETY_FACTOR: f64 = 1.05;
/// Per-message overhead Anthropic/OpenAI add around every turn — counted on
/// top of the raw content tokens.
pub const PER_MESSAGE_OVERHEAD: u64 = 8;

/// Provider-aware token counting. Implementations must be cheap (called per
/// message during every context-budget pass) and deterministic.
pub trait Tokenizer: Send + Sync {
    /// Token count for a single message's content + tool-call payload +
    /// per-message overhead. Callers typically sum this across a slice.
    fn count_message(&self, msg: &Message) -> usize;

    /// Raw token count for free-form text (no per-message overhead).
    fn count_text(&self, text: &str) -> usize;

    /// Human-readable name for diagnostics.
    fn name(&self) -> &'static str {
        "tokenizer"
    }
}

// --- Tiktoken adapter --------------------------------------------------------

static CL100K: Lazy<tiktoken_rs::CoreBPE> =
    Lazy::new(|| tiktoken_rs::cl100k_base().expect("cl100k_base BPE initialization failed"));

/// `cl100k_base` BPE tokenizer — matches Claude/GPT-4 tokenization closely
/// enough to be the default for every provider that doesn't ship a dedicated
/// counter.
#[derive(Debug, Default, Clone, Copy)]
pub struct TiktokenAdapter;

impl TiktokenAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Tokenizer for TiktokenAdapter {
    fn count_message(&self, msg: &Message) -> usize {
        let mut text = msg.content.clone();
        for tc in &msg.tool_calls {
            text.push_str(&tc.name);
            text.push_str(&tc.arguments.to_string());
        }
        if let Some(id) = &msg.tool_call_id {
            text.push_str(id);
        }
        if let Some(n) = &msg.name {
            text.push_str(n);
        }
        CL100K.encode_ordinary(&text).len() + PER_MESSAGE_OVERHEAD as usize
    }

    fn count_text(&self, text: &str) -> usize {
        CL100K.encode_ordinary(text).len()
    }

    fn name(&self) -> &'static str {
        "tiktoken:cl100k_base"
    }
}

// --- Heuristic adapter -------------------------------------------------------

/// Fallback tokenizer that uses the classic `chars / 4` heuristic. Useful in
/// environments where pulling in the full BPE table is undesirable (tests,
/// lightweight TUI estimates) and as a parity fixture.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicAdapter;

impl HeuristicAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Tokenizer for HeuristicAdapter {
    fn count_message(&self, msg: &Message) -> usize {
        let mut n = self.count_text(&msg.content);
        for tc in &msg.tool_calls {
            n += self.count_text(&tc.name) + self.count_text(&tc.arguments.to_string());
        }
        if let Some(id) = &msg.tool_call_id {
            n += self.count_text(id);
        }
        if let Some(name) = &msg.name {
            n += self.count_text(name);
        }
        n + PER_MESSAGE_OVERHEAD as usize
    }

    fn count_text(&self, text: &str) -> usize {
        // Char-based — byte length would double-count multi-byte UTF-8.
        text.chars().count().div_ceil(4)
    }

    fn name(&self) -> &'static str {
        "heuristic:chars/4"
    }
}

// --- Anthropic API tokenizer (feature-gated stub) ---------------------------

/// Skeleton for a tokenizer backed by Anthropic's
/// `POST /v1/messages/count_tokens` endpoint. The HTTP call itself is a TODO —
/// flipping the `anthropic-count-tokens` feature on makes the type available
/// so code can reference it, but `count_message`/`count_text` currently fall
/// back to the tiktoken adapter. Replace the bodies with a real blocking HTTP
/// call (or async + `block_on`) once the endpoint contract is finalized.
#[cfg(feature = "anthropic-count-tokens")]
pub struct AnthropicApiTokenizer {
    api_key: String,
    base_url: String,
    model: String,
    fallback: TiktokenAdapter,
}

#[cfg(feature = "anthropic-count-tokens")]
impl AnthropicApiTokenizer {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            model: model.into(),
            fallback: TiktokenAdapter::new(),
        }
    }
}

#[cfg(feature = "anthropic-count-tokens")]
impl Tokenizer for AnthropicApiTokenizer {
    fn count_message(&self, msg: &Message) -> usize {
        // TODO: POST to {base_url}/v1/messages/count_tokens with {api_key}
        // and {model}. For now delegate to the local estimator to keep
        // callers correct under load-shedding / offline conditions.
        let _ = (&self.api_key, &self.base_url, &self.model);
        self.fallback.count_message(msg)
    }

    fn count_text(&self, text: &str) -> usize {
        self.fallback.count_text(text)
    }

    fn name(&self) -> &'static str {
        "anthropic-count-tokens:stub"
    }
}

// --- Helpers -----------------------------------------------------------------

/// Sum tokens across a slice of messages, applying the documented safety
/// factor. Equivalent to the old `estimate_tokens` free function in
/// `vil_swarm::context_budget`.
pub fn estimate_tokens<T: Tokenizer + ?Sized>(tokenizer: &T, messages: &[Message]) -> u64 {
    let raw: usize = messages.iter().map(|m| tokenizer.count_message(m)).sum();
    (raw as f64 * SAFETY_FACTOR).ceil() as u64
}

/// Default adapter used when callers don't specify one — tiktoken.
pub fn default_tokenizer() -> TiktokenAdapter {
    TiktokenAdapter::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Message, Role, ToolCall};
    use serde_json::json;

    fn mk(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: vec![],
            image_parts: vec![],
        }
    }

    #[test]
    fn tiktoken_counts_monotonic_in_content() {
        let t = TiktokenAdapter::new();
        let short = mk(Role::User, "hi");
        let long = mk(Role::User, &"word ".repeat(100));
        assert!(t.count_message(&long) > t.count_message(&short));
    }

    #[test]
    fn heuristic_counts_monotonic_in_content() {
        let h = HeuristicAdapter::new();
        let short = mk(Role::User, "hi");
        let long = mk(Role::User, &"word ".repeat(100));
        assert!(h.count_message(&long) > h.count_message(&short));
    }

    #[test]
    fn tool_calls_add_tokens() {
        let t = TiktokenAdapter::new();
        let plain = mk(Role::Assistant, "ok");
        let mut with_tool = plain.clone();
        with_tool.tool_calls.push(ToolCall {
            id: "tc1".into(),
            name: "file_read".into(),
            arguments: json!({"path": "src/main.rs"}),
        });
        assert!(t.count_message(&with_tool) > t.count_message(&plain));
    }

    #[test]
    fn heuristic_and_tiktoken_disagree_but_correlate() {
        // Parity test: order is preserved between the two tokenizers on a
        // set of fixture strings even though absolute counts differ.
        let fixtures = [
            "hello",
            "the quick brown fox jumps over the lazy dog",
            &"word ".repeat(500),
        ];
        let t = TiktokenAdapter::new();
        let h = HeuristicAdapter::new();
        let mut t_counts: Vec<usize> = fixtures.iter().map(|s| t.count_text(s)).collect();
        let mut h_counts: Vec<usize> = fixtures.iter().map(|s| h.count_text(s)).collect();
        let t_order: Vec<usize> = {
            let mut idx: Vec<usize> = (0..t_counts.len()).collect();
            idx.sort_by_key(|&i| t_counts[i]);
            idx
        };
        let h_order: Vec<usize> = {
            let mut idx: Vec<usize> = (0..h_counts.len()).collect();
            idx.sort_by_key(|&i| h_counts[i]);
            idx
        };
        assert_eq!(t_order, h_order, "relative ordering should match");
        // Count buckets should agree within an order of magnitude on plain
        // English — guards against wild drift in either implementation.
        t_counts.sort();
        h_counts.sort();
        for (tc, hc) in t_counts.iter().zip(h_counts.iter()) {
            let ratio = (*tc.max(hc) as f64) / (*tc.min(hc).max(&1) as f64);
            assert!(ratio < 3.0, "tc={} hc={} ratio={}", tc, hc, ratio);
        }
    }

    #[test]
    fn estimate_applies_safety_factor() {
        let t = TiktokenAdapter::new();
        let msgs = vec![mk(Role::User, "hello world")];
        let raw = t.count_message(&msgs[0]) as f64;
        let est = estimate_tokens(&t, &msgs);
        assert!(est as f64 >= raw);
        assert!(est as f64 <= (raw * SAFETY_FACTOR).ceil());
    }

    #[test]
    fn dyn_tokenizer_is_object_safe() {
        let t: Box<dyn Tokenizer> = Box::new(TiktokenAdapter::new());
        let _ = t.count_text("hello");
        let h: Box<dyn Tokenizer> = Box::new(HeuristicAdapter::new());
        let _ = h.count_text("hello");
    }
}
