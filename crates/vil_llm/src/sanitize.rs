//! Message sanitization for provider contract hardening.
//!
//! Ensures message sequences conform to provider requirements:
//! - No consecutive same-role messages
//! - No orphan tool results (tool results must follow assistant with tool_calls)
//! - No orphan tool calls (tool calls must have corresponding tool results)
//! - No duplicate tool results for same tool_call_id

use crate::models::LlmMessage;
use crate::provider::{Message, Role};

/// Sanitize messages for provider compatibility.
pub fn sanitize_messages(messages: &[Message], _provider_name: &str) -> Vec<Message> {
    if messages.is_empty() {
        return vec![];
    }

    // Order matters:
    // 1. Dedup tool results first
    // 2. Drop orphan tool results (sequential validation)
    // 3. Drop orphan tool calls (missing results)
    // 4. Merge consecutive same role LAST (to clean up after deletions)
    let mut result = dedup_tool_results(messages);
    result = drop_orphan_tool_results(&result);
    result = drop_orphan_tool_calls(&result);
    result = merge_consecutive_same_role(&result);
    result
}

/// Sanitize LlmMessage format for provider compatibility.
pub fn sanitize_llm_messages(messages: &[LlmMessage], provider_name: &str) -> Vec<LlmMessage> {
    // Convert to Message, sanitize, convert back
    let msgs: Vec<Message> = messages.iter().map(Message::from).collect();
    let sanitized = sanitize_messages(&msgs, provider_name);
    sanitized.iter().map(LlmMessage::from).collect()
}

/// Merge consecutive messages with the same role.
/// Must be called LAST after other sanitization that may delete messages.
/// For Assistant messages with tool_calls: merge tool_calls arrays instead of skipping.
fn merge_consecutive_same_role(messages: &[Message]) -> Vec<Message> {
    if messages.is_empty() {
        return vec![];
    }

    let mut result = Vec::with_capacity(messages.len());
    let mut iter = messages.iter().peekable();

    while let Some(msg) = iter.next() {
        let mut merged = msg.clone();

        // Peek and merge while same role
        while let Some(next) = iter.peek() {
            if next.role != merged.role {
                break;
            }

            // Tool messages cannot be merged (each has unique tool_call_id)
            if merged.role == Role::Tool || next.role == Role::Tool {
                break;
            }

            // For Assistant messages: merge tool_calls arrays
            if merged.role == Role::Assistant
                && (!merged.tool_calls.is_empty() || !next.tool_calls.is_empty())
            {
                // Merge content
                if !merged.content.is_empty() && !next.content.is_empty() {
                    merged.content.push_str("\n\n");
                }
                merged.content.push_str(&next.content);
                // Merge tool_calls
                merged.tool_calls.extend(next.tool_calls.clone());
                iter.next();
                continue;
            }

            // For User/System messages without tool_calls: merge content
            if merged.tool_calls.is_empty() && next.tool_calls.is_empty() {
                if !merged.content.is_empty() && !next.content.is_empty() {
                    merged.content.push_str("\n\n");
                }
                merged.content.push_str(&next.content);
                iter.next();
                continue;
            }

            break;
        }

        result.push(merged);
    }

    result
}

/// Drop tool results that don't immediately follow an assistant with matching tool_call_id.
/// Uses SEQUENTIAL validation (not global HashSet) per Anthropic requirements.
fn drop_orphan_tool_results(messages: &[Message]) -> Vec<Message> {
    if messages.is_empty() {
        return vec![];
    }

    let mut result = Vec::with_capacity(messages.len());
    let mut pending_tool_call_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    for msg in messages {
        match msg.role {
            Role::Assistant => {
                // Collect tool_call_ids from this assistant message
                for tc in &msg.tool_calls {
                    pending_tool_call_ids.insert(tc.id.clone());
                }
                result.push(msg.clone());
            }
            Role::Tool => {
                // Tool result is valid ONLY if it matches a pending tool_call_id
                if let Some(ref id) = msg.tool_call_id {
                    if pending_tool_call_ids.contains(id) {
                        pending_tool_call_ids.remove(id);
                        result.push(msg.clone());
                    }
                    // else: orphan tool result, drop it
                }
                // else: tool message without tool_call_id, drop it
            }
            _ => {
                // User/System messages clear the pending tool calls context
                pending_tool_call_ids.clear();
                result.push(msg.clone());
            }
        }
    }

    result
}

/// Drop tool calls from assistant messages that don't have corresponding tool results.
/// Injects dummy "TOOL_CALL_CANCELLED" results for orphan tool calls.
/// IMPORTANT: Keeps tool_calls in assistant message intact to avoid orphan tool results.
fn drop_orphan_tool_calls(messages: &[Message]) -> Vec<Message> {
    if messages.is_empty() {
        return vec![];
    }

    // First pass: collect which tool_call_ids have results
    let mut resolved_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for msg in messages {
        if msg.role == Role::Tool {
            if let Some(ref id) = msg.tool_call_id {
                resolved_ids.insert(id.clone());
            }
        }
    }

    // Second pass: inject dummy results for orphan tool calls
    // DO NOT remove tool_calls from assistant - that would orphan the tool results
    let mut result = Vec::with_capacity(messages.len());
    for msg in messages {
        result.push(msg.clone());

        // If assistant has tool_calls, check for orphans and inject dummy results
        if msg.role == Role::Assistant && !msg.tool_calls.is_empty() {
            for tc in &msg.tool_calls {
                if !resolved_ids.contains(&tc.id) {
                    result.push(Message::tool(&tc.name, &tc.id, "TOOL_CALL_CANCELLED"));
                }
            }
        }
    }

    result
}

/// Deduplicate tool results with the same tool_call_id.
fn dedup_tool_results(messages: &[Message]) -> Vec<Message> {
    if messages.is_empty() {
        return vec![];
    }

    let mut seen_ids = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(messages.len());

    for msg in messages {
        if msg.role == Role::Tool {
            if let Some(ref id) = msg.tool_call_id {
                if seen_ids.contains(id) {
                    continue; // Skip duplicate
                }
                seen_ids.insert(id.clone());
            }
        }
        result.push(msg.clone());
    }

    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::provider::ToolCall;

    fn tool_call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: serde_json::json!({}),
        }
    }

    #[test]
    fn test_merge_consecutive_user() {
        let messages = vec![
            Message::user("Hello"),
            Message::user("World"),
            Message::assistant("Hi"),
        ];

        let result = merge_consecutive_same_role(&messages);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, Role::User);
        assert_eq!(result[0].content, "Hello\n\nWorld");
    }

    #[test]
    fn test_no_merge_different_roles() {
        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi"),
            Message::user("World"),
        ];

        let result = merge_consecutive_same_role(&messages);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_merge_assistant_with_and_without_tool_calls() {
        // Assistant with tool_calls followed by assistant without - should merge
        let messages = vec![
            Message::assistant_with_tool_calls("Using tool", vec![tool_call("1", "test")]),
            Message::assistant("More text"),
        ];

        let result = merge_consecutive_same_role(&messages);
        assert_eq!(result.len(), 1); // Merged into one
        assert_eq!(result[0].content, "Using tool\n\nMore text");
        assert_eq!(result[0].tool_calls.len(), 1); // tool_calls preserved
    }

    #[test]
    fn test_drop_orphan_tool_result() {
        let messages = vec![
            Message::user("Hello"),
            Message::tool("test", "orphan_id", "result"), // No assistant with this tool_call_id
        ];

        let result = drop_orphan_tool_results(&messages);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, Role::User);
    }

    #[test]
    fn test_keep_valid_tool_result() {
        let messages = vec![
            Message::assistant_with_tool_calls("", vec![tool_call("call_123", "test")]),
            Message::tool("test", "call_123", "result"),
        ];

        let result = drop_orphan_tool_results(&messages);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dedup_tool_results() {
        let messages = vec![
            Message::assistant_with_tool_calls("", vec![tool_call("1", "a"), tool_call("2", "b")]),
            Message::tool("a", "1", "result1"),
            Message::tool("a", "1", "duplicate"), // Same tool_call_id
            Message::tool("b", "2", "result2"),
        ];

        let result = dedup_tool_results(&messages);
        assert_eq!(result.len(), 3); // 1 assistant + 2 unique tool results
    }

    #[test]
    fn test_sanitize_full_sequence() {
        let messages = vec![
            Message::user("Hello"),
            Message::user("World"), // Should merge into one
            Message::assistant_with_tool_calls("", vec![tool_call("1", "test")]),
            Message::tool("test", "orphan", "oops"), // Should drop - no matching call
            Message::tool("test", "1", "result"),
            Message::tool("test", "1", "duplicate"), // Should dedup
        ];

        let result = sanitize_messages(&messages, "anthropic");

        // After dedup: user, user, assistant, tool(orphan), tool(1)
        // After drop_orphan: user, user, assistant, tool(1) - orphan dropped
        // After drop_orphan_tool_calls: no change (tool call 1 has result)
        // After merge: user(merged), assistant, tool(1)
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].role, Role::User);
        assert_eq!(result[1].role, Role::Assistant);
        assert_eq!(result[2].role, Role::Tool);
    }

    #[test]
    fn test_sequential_validation_rejects_non_adjacent_tool_result() {
        // Tool result for call_1 appears AFTER a user message breaks the sequence
        let messages = vec![
            Message::assistant_with_tool_calls("", vec![tool_call("call_1", "test")]),
            Message::user("Interrupt"),
            Message::tool("test", "call_1", "late result"), // Should drop - not adjacent
        ];

        let result = drop_orphan_tool_results(&messages);
        assert_eq!(result.len(), 2); // assistant + user, tool result dropped
    }

    #[test]
    fn test_orphan_tool_calls_get_dummy_results() {
        // Assistant has tool call but no corresponding tool result
        let messages = vec![
            Message::assistant_with_tool_calls("", vec![tool_call("orphan_call", "test")]),
            Message::user("Next message"),
        ];

        let result = drop_orphan_tool_calls(&messages);
        // Result: assistant (tool_calls KEPT), dummy tool result, user
        assert_eq!(result.len(), 3);
        // Assistant keeps tool_calls
        assert_eq!(result[0].role, Role::Assistant);
        assert_eq!(result[0].tool_calls.len(), 1);
        // Dummy result injected
        assert_eq!(result[1].role, Role::Tool);
        assert_eq!(result[1].content, "TOOL_CALL_CANCELLED");
        assert_eq!(result[2].role, Role::User);
    }

    #[test]
    fn test_merge_after_deletion() {
        // After dropping orphan tool result, we get consecutive users
        let messages = vec![
            Message::user("First"),
            Message::tool("test", "orphan", "oops"), // Will be dropped
            Message::user("Second"),                 // Now consecutive with First
        ];

        let result = sanitize_messages(&messages, "anthropic");
        assert_eq!(result.len(), 1); // Merged into single user
        assert_eq!(result[0].content, "First\n\nSecond");
    }

    #[test]
    fn test_partial_orphan_tool_calls() {
        // Assistant has 2 tool calls, only 1 has result
        let messages = vec![
            Message::assistant_with_tool_calls(
                "",
                vec![tool_call("resolved", "a"), tool_call("orphan", "b")],
            ),
            Message::tool("a", "resolved", "ok"),
            Message::user("Next"),
        ];

        let result = drop_orphan_tool_calls(&messages);
        // Result: assistant (tool_calls KEPT), tool result, dummy tool result, user
        assert_eq!(result.len(), 4);
        // Assistant keeps ALL tool_calls (not removed)
        assert_eq!(result[0].tool_calls.len(), 2);
        // Check dummy result injected
        let dummy = result
            .iter()
            .find(|m| m.role == Role::Tool && m.tool_call_id.as_deref() == Some("orphan"));
        assert!(dummy.is_some(), "Should have dummy tool result for orphan");
        assert_eq!(dummy.unwrap().content, "TOOL_CALL_CANCELLED");
    }

    #[test]
    fn test_merge_consecutive_assistant_with_tool_calls() {
        // Two consecutive assistant messages with tool_calls should merge
        let messages = vec![
            Message::assistant_with_tool_calls("First", vec![tool_call("1", "a")]),
            Message::assistant_with_tool_calls("Second", vec![tool_call("2", "b")]),
            Message::user("Next"),
        ];

        let result = merge_consecutive_same_role(&messages);
        assert_eq!(result.len(), 2); // merged assistant + user
        assert_eq!(result[0].content, "First\n\nSecond");
        assert_eq!(result[0].tool_calls.len(), 2); // tool_calls merged
    }
}
