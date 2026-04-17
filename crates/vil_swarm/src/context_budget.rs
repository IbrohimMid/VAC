//! Context budget management — two-level message reducer.
//! Adapted from stakpak/libs/agent-core/src/budget_context.rs (Apache-2.0).
//!
//! Tokenization is delegated to [`vil_llm::Tokenizer`] so callers can swap in
//! a provider-aware counter (Anthropic `count_tokens`, custom heuristic, …).
//! The default adapter is `TiktokenAdapter` (cl100k_base) which preserves the
//! original `+8` per-message overhead + `1.05` safety factor that the donor
//! tuned against.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use vil_llm::provider::{Message, Role, ToolCall};
use vil_llm::tokenizer::{SAFETY_FACTOR, TiktokenAdapter, Tokenizer};

const DEFAULT_CONTEXT_WINDOW: u64 = 204_800;
const MAX_OUTPUT_TOKENS: u64 = 4_000;
const TRIM_HEADROOM: f64 = 0.75;
const KEEP_LAST_N_ASSISTANT: usize = 3;
const TRIMMED_PLACEHOLDER: &str = "[trimmed older context]";

/// Process-wide default tokenizer. Exposed so callers that don't care which
/// tokenizer is used (most of them) can just pass `&*DEFAULT_TOKENIZER`.
pub static DEFAULT_TOKENIZER: Lazy<TiktokenAdapter> = Lazy::new(TiktokenAdapter::new);

/// Stores original message content before trimming for potential restoration.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TrimStore(pub HashMap<usize, TrimmedEntry>);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrimmedEntry {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

impl TrimStore {
    /// Save original message content and replace with placeholder.
    /// IMPORTANT: Does NOT clear tool_calls to preserve prompt caching stability.
    pub fn trim_one(&mut self, idx: usize, msg: &mut Message) {
        self.0.entry(idx).or_insert_with(|| TrimmedEntry {
            content: msg.content.clone(),
            tool_calls: msg.tool_calls.clone(),
        });
        msg.content = TRIMMED_PLACEHOLDER.to_string();
        // DO NOT clear tool_calls - preserving them maintains message structure for prompt caching
        // and prevents orphan tool results in sanitize_messages
    }

    /// Restore original message content if it was trimmed.
    pub fn restore(&self, idx: usize, msg: &mut Message) -> bool {
        if let Some(entry) = self.0.get(&idx) {
            msg.content = entry.content.clone();
            msg.tool_calls = entry.tool_calls.clone();
            true
        } else {
            false
        }
    }
}

/// Token count for a single message (content + tool-call payload + per-message
/// overhead) via the supplied tokenizer.
fn message_token_estimate<T: Tokenizer + ?Sized>(tokenizer: &T, msg: &Message) -> u64 {
    tokenizer.count_message(msg) as u64
}

/// Estimate total tokens for a message slice, with safety factor applied.
/// Uses the process-wide default tokenizer — tiktoken cl100k_base.
pub fn estimate_tokens(messages: &[Message]) -> u64 {
    estimate_tokens_with(&*DEFAULT_TOKENIZER, messages)
}

/// Same as [`estimate_tokens`] but with an explicit tokenizer — lets callers
/// swap in a provider-aware counter.
pub fn estimate_tokens_with<T: Tokenizer + ?Sized>(tokenizer: &T, messages: &[Message]) -> u64 {
    let raw: u64 = messages
        .iter()
        .map(|m| message_token_estimate(tokenizer, m))
        .sum();
    (raw as f64 * SAFETY_FACTOR).ceil() as u64
}

/// Two-level context reducer, using the default tokenizer. Equivalent to
/// calling [`reduce_messages_with`] with `&*DEFAULT_TOKENIZER`.
pub fn reduce_messages(
    messages: Vec<Message>,
    trim_boundary: &mut usize,
    store: &mut TrimStore,
) -> Vec<Message> {
    reduce_messages_with(&*DEFAULT_TOKENIZER, messages, trim_boundary, store)
}

/// Two-level context reducer with an explicit tokenizer.
///
/// Level A: trim old assistant+tool messages to placeholder, keep last N assistant.
/// Level B: emergency hard cap — trim oldest non-system messages if still over budget.
///
/// `trim_boundary` tracks the highest trimmed index across turns (cache stability).
/// `store` preserves original content for potential restoration.
pub fn reduce_messages_with<T: Tokenizer + ?Sized>(
    tokenizer: &T,
    mut messages: Vec<Message>,
    trim_boundary: &mut usize,
    store: &mut TrimStore,
) -> Vec<Message> {
    let available = DEFAULT_CONTEXT_WINDOW.saturating_sub(MAX_OUTPUT_TOKENS);
    let threshold = (available as f64 * 0.90) as u64;
    let trim_target = (threshold as f64 * TRIM_HEADROOM) as u64;

    // Fast path: already under budget and no prior trimming needed.
    if *trim_boundary == 0 && estimate_tokens_with(tokenizer, &messages) <= threshold {
        return messages;
    }

    let len = messages.len();

    // Find the last N assistant message indices to preserve.
    let preserved_assistant: std::collections::HashSet<usize> = {
        let mut idxs = Vec::new();
        for i in (0..len).rev() {
            if messages[i].role == Role::Assistant {
                idxs.push(i);
                if idxs.len() >= KEEP_LAST_N_ASSISTANT {
                    break;
                }
            }
        }
        idxs.into_iter().collect()
    };

    // Find latest user message index to always preserve.
    let latest_user_idx = (0..len).rev().find(|&i| messages[i].role == Role::User);

    // --- Level A: trim old assistant+tool messages ---
    let prev_clamped = (*trim_boundary).min(len);
    let mut new_boundary = *trim_boundary;

    for (i, msg) in messages.iter_mut().enumerate().take(len).skip(prev_clamped) {
        if matches!(msg.role, Role::Assistant | Role::Tool)
            && !preserved_assistant.contains(&i)
            && Some(i) != latest_user_idx
        {
            store.trim_one(i, msg);
            new_boundary = new_boundary.max(i + 1);
        }
    }
    *trim_boundary = new_boundary;

    if estimate_tokens_with(tokenizer, &messages) <= threshold {
        return messages;
    }

    // --- Level B: emergency hard cap ---
    // Trim oldest non-system messages until under trim_target.
    // Never remove the latest user message.
    let mut i = 0;
    while i < messages.len() && estimate_tokens_with(tokenizer, &messages) > trim_target {
        if messages[i].role != Role::System && Some(i) != latest_user_idx {
            store.trim_one(i, &mut messages[i]);
            // Update trim_boundary to track this emergency trim
            *trim_boundary = (*trim_boundary).max(i + 1);
        }
        i += 1;
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vil_llm::provider::ToolCall;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: vec![],
            image_parts: vec![],
        }
    }

    fn tool_msg(content: &str) -> Message {
        Message {
            role: Role::Tool,
            content: content.into(),
            name: None,
            tool_call_id: Some("tc1".into()),
            tool_calls: vec![],
            image_parts: vec![],
        }
    }

    fn assistant_with_tool_call(content: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: vec![ToolCall {
                id: "tc1".into(),
                name: "file_read".into(),
                arguments: json!({"path": "src/main.rs"}),
            }],
            image_parts: vec![],
        }
    }

    #[test]
    fn estimate_tokens_grows_with_content() {
        let small = vec![msg(Role::User, "hi")];
        let large = vec![msg(Role::User, &"word ".repeat(2_000))];
        assert!(estimate_tokens(&large) > estimate_tokens(&small));
    }

    #[test]
    fn estimate_tokens_includes_tool_calls() {
        let tokenizer = &*DEFAULT_TOKENIZER;
        let plain = msg(Role::Assistant, "ok");
        let with_tool = assistant_with_tool_call("ok");
        assert!(
            message_token_estimate(tokenizer, &with_tool)
                > message_token_estimate(tokenizer, &plain)
        );
    }

    #[test]
    fn reduce_with_heuristic_tokenizer_still_honors_budget() {
        // Parity: swapping in the cheap heuristic still shrinks a blown
        // context below the window.
        let heuristic = vil_llm::tokenizer::HeuristicAdapter::new();
        let big = "word ".repeat(60_000);
        let mut messages = vec![msg(Role::System, "sys")];
        for _ in 0..6 {
            messages.push(msg(Role::Assistant, &big));
            messages.push(tool_msg(&big));
        }
        messages.push(msg(Role::User, "final task"));

        let mut boundary = 0;
        let mut store = TrimStore::default();
        let reduced = reduce_messages_with(&heuristic, messages, &mut boundary, &mut store);
        assert!(estimate_tokens_with(&heuristic, &reduced) < DEFAULT_CONTEXT_WINDOW);
        assert_eq!(reduced.last().unwrap().content, "final task");
    }

    #[test]
    fn reduce_keeps_recent_messages_under_budget() {
        // Under budget — should return unchanged.
        let messages = vec![
            msg(Role::System, "You are a coder."),
            msg(Role::User, "do something"),
            msg(Role::Assistant, "ok"),
        ];
        let mut boundary = 0;
        let mut store = TrimStore::default();
        let reduced = reduce_messages(messages.clone(), &mut boundary, &mut store);
        assert_eq!(reduced.len(), messages.len());
        assert_eq!(boundary, 0);
    }

    #[test]
    fn reduce_keeps_latest_user_message() {
        // Build a large context that forces trimming.
        let big = "word ".repeat(10_000);
        let mut messages = vec![msg(Role::System, "sys")];
        for _ in 0..5 {
            messages.push(msg(Role::Assistant, &big));
            messages.push(tool_msg(&big));
        }
        messages.push(msg(Role::User, "latest user task"));

        let mut boundary = 0;
        let mut store = TrimStore::default();
        let reduced = reduce_messages(messages, &mut boundary, &mut store);

        let latest_user = reduced.iter().rev().find(|m| m.role == Role::User);
        assert!(latest_user.is_some());
        assert_eq!(latest_user.unwrap().content, "latest user task");
    }

    #[test]
    fn reduce_trims_old_assistant_tool_messages() {
        let big = "word ".repeat(20_000);
        let mut messages = vec![msg(Role::System, "sys")];
        for _ in 0..8 {
            messages.push(msg(Role::Assistant, &big));
            messages.push(tool_msg(&big));
        }
        messages.push(msg(Role::User, "new task"));

        let mut boundary = 0;
        let mut store = TrimStore::default();
        let reduced = reduce_messages(messages, &mut boundary, &mut store);

        // At least some old assistant/tool messages should be trimmed.
        let trimmed_count = reduced
            .iter()
            .filter(|m| m.content == TRIMMED_PLACEHOLDER)
            .count();
        assert!(trimmed_count > 0);
    }

    #[test]
    fn trim_boundary_frozen_when_under_budget() {
        let messages = vec![
            msg(Role::System, "sys"),
            msg(Role::User, "task"),
            msg(Role::Assistant, "done"),
        ];
        let mut boundary = 0;
        let mut store = TrimStore::default();
        reduce_messages(messages, &mut boundary, &mut store);
        assert_eq!(boundary, 0, "boundary should not advance when under budget");
    }

    #[test]
    fn reduce_still_shrinks_when_many_user_turns_exist() {
        // Many large user turns — emergency fallback must still reduce.
        let big = "word ".repeat(6_000);
        let mut messages = vec![msg(Role::System, "sys")];
        for _ in 0..10 {
            messages.push(msg(Role::User, &big));
            messages.push(msg(Role::Assistant, &big));
        }
        messages.push(msg(Role::User, "final task"));

        let mut boundary = 0;
        let mut store = TrimStore::default();
        let reduced = reduce_messages(messages, &mut boundary, &mut store);

        assert!(estimate_tokens(&reduced) < 204_800);
        // Latest user message preserved.
        assert_eq!(reduced.last().unwrap().content, "final task");
    }

    #[test]
    fn trim_store_trim_one_and_restore() {
        let mut store = TrimStore::default();
        let mut msg = msg(Role::Assistant, "original content");
        msg.tool_calls.push(ToolCall {
            id: "tc1".into(),
            name: "test".into(),
            arguments: json!({}),
        });

        store.trim_one(5, &mut msg);
        assert_eq!(msg.content, TRIMMED_PLACEHOLDER);
        // tool_calls should NOT be cleared (prompt caching stability)
        assert_eq!(msg.tool_calls.len(), 1);

        let restored = store.restore(5, &mut msg);
        assert!(restored);
        assert_eq!(msg.content, "original content");
        assert_eq!(msg.tool_calls.len(), 1);
    }

    #[test]
    fn trim_store_restore_returns_false_for_unknown_idx() {
        let store = TrimStore::default();
        let mut msg = msg(Role::User, "test");
        assert!(!store.restore(99, &mut msg));
    }
}
