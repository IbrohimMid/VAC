//! D8E — tool-use transcript replay helper.
//!
//! Read a session transcript JSONL and pair every `tool_call`
//! row with its matching `tool_result` row, in transcript
//! order. Tolerant of:
//!
//! * old transcripts that have no tool rows (returns empty).
//! * partial transcripts where a `tool_result` is missing
//!   (the corresponding view's `result` field stays `None`).
//! * unknown row kinds (skipped, no error).
//!
//! Pairing is by `id`. The engine writes `tool_call` and
//! `tool_result` with matching ids per call (D7E contract).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use vac_tool_core::ToolResultEnvelope;

/// One paired (call, optional result) view extracted from a
/// transcript file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolUseTranscriptView {
    /// The id the LLM assigned to the tool call.
    pub id: String,
    /// Tool name as the LLM emitted it.
    pub name: String,
    /// Raw arguments JSON the LLM emitted.
    pub arguments: serde_json::Value,
    /// Optional reason / rationale (currently always `None`
    /// from the D7D bridge but read back as-is).
    pub reason: Option<String>,
    /// Token estimate (D7D bridge writes 0).
    pub estimated_tokens: u64,
    /// `None` when the transcript records no `tool_result` row
    /// for this call id (truncated session, mid-flight crash,
    /// or a transcript predating D7E that contained calls but
    /// not results).
    pub result: Option<ToolResultEnvelope>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolUseReplayError {
    #[error("transcript io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("transcript parse error at {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

/// Read every `tool_call` / `tool_result` row from
/// `transcript_path` and return them paired in transcript
/// order. Missing transcript file is `Ok(vec![])` so callers
/// can stream this over older sessions without branching.
pub fn read_tool_use_rows(
    transcript_path: impl AsRef<Path>,
) -> Result<Vec<ToolUseTranscriptView>, ToolUseReplayError> {
    let path = transcript_path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(ToolUseReplayError::Io {
                path: path.display().to_string(),
                source: e,
            });
        }
    };
    let text = String::from_utf8_lossy(&bytes);

    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, ToolUseTranscriptView> = HashMap::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: serde_json::Value =
            serde_json::from_str(line).map_err(|e| ToolUseReplayError::Parse {
                path: path.display().to_string(),
                source: e,
            })?;
        let kind = row.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let content = match row.get("content") {
            Some(c) => c,
            None => continue,
        };
        match kind {
            "tool_call" => {
                let id = content
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                let view = ToolUseTranscriptView {
                    id: id.clone(),
                    name: content
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    arguments: content
                        .get("arguments")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                    reason: content
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    estimated_tokens: content
                        .get("estimated_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    result: None,
                };
                if !by_id.contains_key(&id) {
                    order.push(id.clone());
                }
                by_id.insert(id, view);
            }
            "tool_result" => {
                let id = content
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                let envelope: Option<ToolResultEnvelope> = content
                    .get("envelope")
                    .cloned()
                    .and_then(|e| serde_json::from_value(e).ok());
                if let Some(view) = by_id.get_mut(&id) {
                    view.result = envelope;
                }
            }
            _ => {}
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect())
}
