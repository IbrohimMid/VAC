//! B4 — `EngineAgentDispatcher`: concrete `AgentDispatcher` impl
//! that drives the session-engine subagent machinery and folds
//! the resulting `SubmitStream` into a single `ToolResultEnvelope`
//! the tool-side caller can return directly.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::StreamExt;
use uuid::Uuid;
use vac_session_primitives::{AgentDispatchInput, AgentDispatcher};
use vac_tool_core::{ToolResultEnvelope, ToolResultKind};

use crate::agent_tool::{AgentToolInput, dispatch_agent_tool};
use crate::error::{EngineError, EngineResult};
use crate::stream::SubmitChunk;
use crate::subagent::SubagentDispatchContext;

pub struct EngineAgentDispatcher {
    dispatch_ctx: SubagentDispatchContext,
}

impl EngineAgentDispatcher {
    pub fn new(dispatch_ctx: SubagentDispatchContext) -> Self {
        Self { dispatch_ctx }
    }
}

impl AgentDispatcher for EngineAgentDispatcher {
    fn dispatch<'a>(
        &'a self,
        input: AgentDispatchInput,
        parent_session_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = EngineResult<ToolResultEnvelope>> + Send + 'a>> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            let subagent_type = input.subagent_type.clone();
            let engine_input = AgentToolInput {
                subagent_type: input.subagent_type,
                description: input.description,
                prompt: input.prompt,
                isolation: input.isolation,
            };
            let mut stream =
                dispatch_agent_tool(engine_input, parent_session_id, self.dispatch_ctx.clone())
                    .await?;

            let mut tool_calls: Vec<serde_json::Value> = Vec::new();
            let mut content = String::new();
            let mut aborted: Option<String> = None;
            while let Some(chunk) = stream.next().await {
                match chunk {
                    SubmitChunk::TextDelta { text } => content.push_str(&text),
                    SubmitChunk::ToolResult { id, name, payload } => {
                        tool_calls.push(serde_json::json!({
                            "id": id,
                            "name": name,
                            "payload": payload,
                        }));
                    }
                    SubmitChunk::Aborted { reason } => {
                        aborted = Some(reason);
                    }
                    SubmitChunk::Finished { .. } => break,
                    _ => {}
                }
            }

            let duration_ms = started.elapsed().as_millis() as u64;
            if let Some(reason) = aborted {
                return Ok(ToolResultEnvelope {
                    kind: ToolResultKind::Error,
                    summary: format!("subagent {subagent_type} aborted"),
                    payload: serde_json::json!({
                        "subagent_type": subagent_type,
                        "error": reason,
                        "tool_calls": tool_calls,
                        "content": content,
                    }),
                    duration_ms,
                });
            }

            Ok(ToolResultEnvelope {
                kind: ToolResultKind::Ok,
                summary: format!(
                    "subagent {subagent_type} finished ({} tool calls)",
                    tool_calls.len()
                ),
                payload: serde_json::json!({
                    "subagent_type": subagent_type,
                    "tool_calls": tool_calls,
                    "content": content,
                }),
                duration_ms,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::TrivialCompactBoundary;
    use crate::llm::EchoAdapter;
    use crate::slash::SlashProcessor;
    use crate::subagent::SubagentDispatchContext;
    use crate::transcript::TranscriptWriter;
    use crate::usage::UsageTracker;

    #[tokio::test]
    async fn dispatches_echo_subagent_and_folds_envelope() {
        let tmp = tempfile::tempdir().unwrap();
        let dispatch_ctx = SubagentDispatchContext::new(
            Arc::new(TranscriptWriter::new(tmp.path().to_path_buf())),
            Arc::new(SlashProcessor::new()),
            Arc::new(TrivialCompactBoundary::default()),
            Arc::new(UsageTracker::new()),
            Arc::new(EchoAdapter),
        );
        let dispatcher = EngineAgentDispatcher::new(dispatch_ctx);
        let envelope = dispatcher
            .dispatch(
                AgentDispatchInput {
                    subagent_type: "explore".into(),
                    description: "smoke test".into(),
                    prompt: "hello".into(),
                    isolation: None,
                },
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        assert_eq!(envelope.kind, ToolResultKind::Ok);
        assert_eq!(envelope.payload["subagent_type"], "explore");
        assert!(
            envelope.payload["content"]
                .as_str()
                .unwrap()
                .contains("echo")
        );
    }
}
