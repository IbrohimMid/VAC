//! Stream processor — reconstructs LlmResponse from SSE stream chunks.
//! Extracted from orchestrator.rs for pipeline clarity.

use tokio::sync::mpsc;
use vil_llm::provider::{FinishReason, LlmResponse, StreamChunk, TokenUsage, ToolCall};

use crate::error::{SwarmError, SwarmResult};
use crate::orchestrator::AgentLoopEvent;

/// Result of processing a complete SSE stream.
pub struct StreamResult {
    pub response: LlmResponse,
}

/// Drain an SSE stream receiver, reassemble text + tool calls, and produce a single LlmResponse.
///
/// Emits `AssistantChunk` events in real-time as text deltas arrive.
pub async fn process_stream(
    mut rx: mpsc::Receiver<StreamChunk>,
    updates: &Option<mpsc::UnboundedSender<AgentLoopEvent>>,
) -> SwarmResult<StreamResult> {
    let mut full_content = String::new();
    let mut tool_args_buf: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    let mut stream_usage = TokenUsage::default();
    let mut stream_finish = FinishReason::Stop;

    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Text(text) => {
                full_content.push_str(&text);
                if let Some(tx) = updates {
                    let _ = tx.send(AgentLoopEvent::AssistantChunk(text));
                }
            }
            StreamChunk::ToolCallStart { id, name } => {
                tool_args_buf.insert(id, (name, String::new()));
            }
            StreamChunk::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                if let Some((_, args)) = tool_args_buf.get_mut(&id) {
                    args.push_str(&arguments_delta);
                }
            }
            StreamChunk::Done {
                usage,
                finish_reason,
            } => {
                stream_usage = usage;
                stream_finish = finish_reason;
            }
            StreamChunk::Error(e) => {
                return Err(SwarmError::Orchestration(format!(
                    "LLM stream error: {}",
                    e
                )));
            }
            StreamChunk::ToolCallComplete(tool_call) => {
                // Already-assembled tool call from the streaming assembler
                tool_args_buf.insert(
                    tool_call.id.clone(),
                    (tool_call.name, tool_call.arguments.to_string()),
                );
            }
        }
    }

    // Reconstruct tool calls from buffered argument deltas
    let mut stream_tool_calls: Vec<ToolCall> = Vec::new();
    for (id, (name, args_str)) in tool_args_buf {
        let arguments = serde_json::from_str(&args_str)
            .unwrap_or_else(|_| serde_json::json!({"_parse_error": true}));
        stream_tool_calls.push(ToolCall {
            id,
            name,
            arguments,
        });
    }

    // Fallback: if SSE didn't provide finish_reason, infer from tool_calls presence
    if stream_finish == FinishReason::Stop && !stream_tool_calls.is_empty() {
        stream_finish = FinishReason::ToolUse;
    }

    let response = LlmResponse {
        content: full_content,
        model: std::env::var("KILO_MODEL").unwrap_or_else(|_| "kilo-auto/free".to_string()),
        finish_reason: stream_finish,
        usage: stream_usage,
        tool_calls: stream_tool_calls,
    };

    Ok(StreamResult { response })
}

/// Collect text from stream with error checking.
/// Returns error if stream contains StreamChunk::Error.
pub async fn collect_text_checked_wrapper(rx: mpsc::Receiver<StreamChunk>) -> SwarmResult<String> {
    use vil_llm::streaming::collect_text_checked;
    collect_text_checked(rx)
        .await
        .map_err(SwarmError::Orchestration)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use vil_llm::provider::{FinishReason, StreamChunk, TokenUsage};

    #[tokio::test]
    async fn stream_processor_assembles_text() {
        let (tx, rx) = mpsc::channel(10);

        tx.send(StreamChunk::Text("Hello ".to_string()))
            .await
            .unwrap();
        tx.send(StreamChunk::Text("world".to_string()))
            .await
            .unwrap();
        tx.send(StreamChunk::Done {
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                ..Default::default()
            },
            finish_reason: FinishReason::Stop,
        })
        .await
        .unwrap();
        drop(tx);

        let result = process_stream(rx, &None).await.unwrap();
        assert_eq!(result.response.content, "Hello world");
        assert_eq!(result.response.finish_reason, FinishReason::Stop);
    }

    #[tokio::test]
    async fn stream_processor_assembles_tool_calls() {
        let (tx, rx) = mpsc::channel(10);

        tx.send(StreamChunk::ToolCallStart {
            id: "1".into(),
            name: "test".into(),
        })
        .await
        .unwrap();
        tx.send(StreamChunk::ToolCallDelta {
            id: "1".into(),
            arguments_delta: "{\"a\":".into(),
        })
        .await
        .unwrap();
        tx.send(StreamChunk::ToolCallDelta {
            id: "1".into(),
            arguments_delta: "1}".into(),
        })
        .await
        .unwrap();
        tx.send(StreamChunk::Done {
            usage: TokenUsage::default(),
            finish_reason: FinishReason::ToolUse,
        })
        .await
        .unwrap();
        drop(tx);

        let result = process_stream(rx, &None).await.unwrap();
        assert_eq!(result.response.tool_calls.len(), 1);
        assert_eq!(result.response.tool_calls[0].name, "test");
    }
}
