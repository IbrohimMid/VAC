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
//! The `anthropic-count-tokens` feature flag is **disabled** via
//! `compile_error!`. It previously held a stub that silently delegated to
//! the tiktoken adapter, which was misleading. Enable the flag only after
//! wiring the real `POST /v1/messages/count_tokens` HTTP call.

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

#[allow(clippy::expect_used)] // BPE init is infallible with bundled data; panic at init is intentional.
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

/// Fallback tokenizer that uses the classic `chars / 4` heuristic.
///
/// **Fallback-only.** Use [`TiktokenAdapter`] for any budget, billing, or
/// truncation decisions. The heuristic's documented error band vs tiktoken
/// is roughly 0.5×–3× depending on input corpus:
/// * English prose: typically 0.7–1.3× true count
/// * Source code: often over-counts (more punctuation per token)
/// * CJK / emoji: under-counts (one tiktoken token ≈ 2–4 chars)
/// * Whitespace-heavy: over-counts
///
/// The `heuristic_error_rate_across_corpus` test in this module asserts the
/// max relative error stays within a 5× band across a representative corpus.
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

// --- Anthropic API tokenizer (feature-gated — BLOCKED) ----------------------

// The `anthropic-count-tokens` feature previously held a stub that silently
// delegated to TiktokenAdapter. This was misleading: callers believed they
// were getting Anthropic-native counts, but received local BPE estimates.
// The feature is now blocked at compile time. Re-enable it only after
// implementing the real HTTP call to POST /v1/messages/count_tokens.
#[cfg(feature = "anthropic-count-tokens")]
compile_error!(
    "The `anthropic-count-tokens` feature is a stub. \
     Do not enable until the HTTP call to /v1/messages/count_tokens is implemented. \
     See crates/vil_llm/src/tokenizer.rs for context."
);

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
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
    fn heuristic_error_rate_across_corpus() {
        // Measures HeuristicAdapter accuracy vs TiktokenAdapter across a
        // representative corpus. The heuristic is fallback-only; this test
        // documents and bounds its error band so consumers know what to expect.
        let repetitive = "word ".repeat(200);
        let corpus: Vec<(&str, &str)> = vec![
            ("english", "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs."),
            ("rust_code", "fn main() { let x: Vec<u32> = (0..10).collect(); println!(\"{:?}\", x); }"),
            ("json", r#"{"name":"vac","version":"0.1.0","deps":["tokio","serde"]}"#),
            ("cjk", "こんにちは世界。これはトークナイザのテストです。你好世界，这是分词器测试。"),
            ("emoji", "🚀🎉💡🔥✨ deploy shipped 🎊🥳🎈"),
            ("mixed", "Hello 世界! 🌍 fn greet() -> &'static str { \"hi\" }"),
            ("whitespace", "   \t\n   spaced   out   \n\n   content   "),
            ("repetitive", &repetitive),
        ];

        let t = TiktokenAdapter::new();
        let h = HeuristicAdapter::new();

        let mut max_ratio: f64 = 1.0;
        for (label, text) in &corpus {
            let tc = t.count_text(text).max(1);
            let hc = h.count_text(text).max(1);
            let ratio = (tc.max(hc) as f64) / (tc.min(hc) as f64);
            eprintln!("{label:>12}: tiktoken={tc:>4} heuristic={hc:>4} ratio={ratio:.2}");
            max_ratio = max_ratio.max(ratio);
        }

        // Documented error band: heuristic stays within 5× of tiktoken across
        // our representative corpus. Emoji and CJK are the weak spots (one
        // tiktoken token can span 4+ chars). Tightening this requires a real
        // BPE-aware estimator.
        assert!(
            max_ratio < 5.0,
            "heuristic error exceeds 5x bound: max_ratio={max_ratio:.2}"
        );
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
