//! Streaming response utilities.
//!
//! Includes [`ToolCallAssembler`], which buffers partial tool-call deltas
//! (id/name announcements + argument fragments) and emits a single
//! [`StreamChunk::ToolCallComplete`] once the tool-use block terminates.
//! High-level consumers should receive only `Text`, `ToolCallComplete`,
//! `Done`, and `Error` — never half-parsed JSON.

use crate::provider::{StreamChunk, ToolCall};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// A partial tool call being assembled from streaming deltas.
#[derive(Debug, Clone, Default)]
pub struct PartialToolCall {
    pub id: String,
    pub name: Option<String>,
    pub arguments_json: String,
}

impl PartialToolCall {
    fn finalize(self) -> ToolCall {
        let name = self.name.unwrap_or_default();
        let arguments = if self.arguments_json.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&self.arguments_json)
                .unwrap_or_else(|_| serde_json::json!({ "raw": self.arguments_json }))
        };
        ToolCall {
            id: self.id,
            name,
            arguments,
        }
    }
}

/// Buffers streaming tool-call fragments until a terminator fires.
///
/// Providers feed `ToolCallStart`/`ToolCallDelta` events through the assembler;
/// when the tool-use block ends (explicit terminator or stream `Done`), the
/// assembler drains finished partials into fully parsed [`ToolCall`]s.
#[derive(Debug, Default)]
pub struct ToolCallAssembler {
    buffers: HashMap<String, PartialToolCall>,
    /// Preserves the order tool calls were first announced, so the downstream
    /// sequence is deterministic when we drain on stream end.
    order: Vec<String>,
}

impl ToolCallAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a tool-call start. Safe to call repeatedly for the same id
    /// (name is only overwritten when a non-empty name arrives).
    pub fn start(&mut self, id: &str, name: &str) {
        let entry = self.buffers.entry(id.to_string()).or_insert_with(|| {
            self.order.push(id.to_string());
            PartialToolCall {
                id: id.to_string(),
                ..PartialToolCall::default()
            }
        });
        if !name.is_empty() {
            entry.name = Some(name.to_string());
        }
    }

    /// Append an argument JSON fragment for the given tool id.
    /// If no prior `start` was observed, we lazily register the id so
    /// providers that skip explicit start events still work.
    pub fn push_delta(&mut self, id: &str, args_delta: &str) {
        let entry = self.buffers.entry(id.to_string()).or_insert_with(|| {
            self.order.push(id.to_string());
            PartialToolCall {
                id: id.to_string(),
                ..PartialToolCall::default()
            }
        });
        entry.arguments_json.push_str(args_delta);
    }

    /// Finalize one tool call by id (e.g. on a `ContentBlockStop`) and return
    /// the completed [`ToolCall`]. Returns `None` if the id was never seen.
    pub fn finalize(&mut self, id: &str) -> Option<ToolCall> {
        let partial = self.buffers.remove(id)?;
        self.order.retain(|existing| existing != id);
        Some(partial.finalize())
    }

    /// Finalize every outstanding tool call in announcement order. Use this on
    /// stream end when the provider doesn't emit per-block terminators.
    pub fn finalize_all(&mut self) -> Vec<ToolCall> {
        let ids = std::mem::take(&mut self.order);
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(partial) = self.buffers.remove(&id) {
                out.push(partial.finalize());
            }
        }
        self.buffers.clear();
        out
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

/// Collect text from stream, returning Result to surface errors.
pub async fn collect_text_checked(mut rx: mpsc::Receiver<StreamChunk>) -> Result<String, String> {
    let mut text = String::new();
    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Text(t) => text.push_str(&t),
            StreamChunk::Done { .. } => break,
            StreamChunk::Error(e) => return Err(e),
            _ => {}
        }
    }
    Ok(text)
}

pub async fn collect_text(mut rx: mpsc::Receiver<StreamChunk>) -> String {
    let mut text = String::new();
    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Text(t) => text.push_str(&t),
            StreamChunk::Done { .. } => break,
            StreamChunk::Error(e) => {
                tracing::error!(error = %e, "Stream error");
                break;
            }
            _ => {}
        }
    }
    text
}

pub async fn print_stream(mut rx: mpsc::Receiver<StreamChunk>) {
    use std::io::Write;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Text(t) => {
                print!("{}", t);
                let _ = std::io::stdout().flush();
            }
            StreamChunk::Done { usage, .. } => {
                println!("\n[Done: {} tokens]", usage.total_tokens);
                break;
            }
            StreamChunk::Error(e) => {
                eprintln!("\n[Error: {}]", e);
                break;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn assembles_single_tool_call_split_across_three_deltas() {
        let mut asm = ToolCallAssembler::new();
        asm.start("call_1", "search");
        asm.push_delta("call_1", "{\"que");
        asm.push_delta("call_1", "ry\":\"hello");
        asm.push_delta("call_1", " world\"}");

        let finalized = asm.finalize("call_1").expect("tool call present");
        assert_eq!(finalized.id, "call_1");
        assert_eq!(finalized.name, "search");
        assert_eq!(finalized.arguments["query"], "hello world");
        assert!(asm.is_empty());
    }

    #[test]
    fn start_without_name_then_later_name_wins() {
        let mut asm = ToolCallAssembler::new();
        asm.start("call_1", ""); // provider sent id first
        asm.push_delta("call_1", "{\"x\":1}");
        asm.start("call_1", "late_name"); // name arrives later

        let finalized = asm.finalize("call_1").unwrap();
        assert_eq!(finalized.name, "late_name");
        assert_eq!(finalized.arguments["x"], 1);
    }

    #[test]
    fn push_delta_before_start_lazily_registers() {
        let mut asm = ToolCallAssembler::new();
        asm.push_delta("call_1", "{\"x\":1}");
        let finalized = asm.finalize("call_1").unwrap();
        assert_eq!(finalized.id, "call_1");
        assert_eq!(finalized.name, ""); // never got a name
    }

    #[test]
    fn finalize_all_preserves_announcement_order() {
        let mut asm = ToolCallAssembler::new();
        asm.start("b", "tool_b");
        asm.start("a", "tool_a");
        asm.push_delta("a", "{}");
        asm.push_delta("b", "{}");

        let all = asm.finalize_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "b");
        assert_eq!(all[1].id, "a");
        assert!(asm.is_empty());
    }

    #[test]
    fn malformed_json_falls_back_to_raw_wrapper() {
        let mut asm = ToolCallAssembler::new();
        asm.start("call_1", "tool");
        asm.push_delta("call_1", "{not json");
        let finalized = asm.finalize("call_1").unwrap();
        assert_eq!(finalized.arguments["raw"], "{not json");
    }

    #[test]
    fn empty_arguments_default_to_empty_object() {
        let mut asm = ToolCallAssembler::new();
        asm.start("call_1", "tool");
        let finalized = asm.finalize("call_1").unwrap();
        assert!(finalized.arguments.is_object());
        assert_eq!(finalized.arguments.as_object().unwrap().len(), 0);
    }

    #[test]
    fn finalize_missing_id_returns_none() {
        let mut asm = ToolCallAssembler::new();
        assert!(asm.finalize("nope").is_none());
    }

    #[test]
    fn assembles_two_concurrent_interleaved_tool_calls() {
        let mut asm = ToolCallAssembler::new();
        asm.start("a", "first");
        asm.start("b", "second");
        asm.push_delta("a", "{\"p\":");
        asm.push_delta("b", "{\"q\":");
        asm.push_delta("a", "1}");
        asm.push_delta("b", "2}");

        let a = asm.finalize("a").unwrap();
        let b = asm.finalize("b").unwrap();
        assert_eq!(a.name, "first");
        assert_eq!(a.arguments["p"], 1);
        assert_eq!(b.name, "second");
        assert_eq!(b.arguments["q"], 2);
    }
}
