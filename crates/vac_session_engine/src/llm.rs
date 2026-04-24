//! LLM adapter trait — abstracts over concrete provider impls so the
//! engine doesn't depend on `vil_llm` (which pulls heavy transport
//! deps). Drivers plug in the concrete adapter at `submit_one` call
//! time.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::EngineResult;

/// Minimal LLM request shape the engine passes down. Drivers expand
/// with their own provider-specific arguments before calling the
/// actual API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub prompt: String,
    /// Additional context lines the driver wants to inject (VIL
    /// project profile, rulebook, recent memdir excerpts). The adapter
    /// decides how these compose with the prompt.
    #[serde(default)]
    pub context: Vec<String>,
}

/// What the adapter returns after completing an LLM turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// Provider id ("anthropic", "openai", "ollama", ...).
    pub provider: String,
    /// Model id.
    pub model: String,
    /// Assembled assistant text.
    pub content: String,
    /// Approximate input tokens consumed.
    #[serde(default)]
    pub input_tokens: u64,
    /// Approximate output tokens emitted.
    #[serde(default)]
    pub output_tokens: u64,
    /// A.3 — tool-use blocks the assistant emitted. Empty when
    /// the response is pure text (today's EchoAdapter stays on
    /// this default); non-empty when the LLM wants the engine to
    /// dispatch tools. Each block is gated through the composite
    /// gate before dispatch, then emits a `ToolResult` event.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRequest>,
}

/// Single tool-use block from the LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    /// Operator-readable reason the model wants the call —
    /// surfaced in the approval prompt and in activity rows.
    #[serde(default)]
    pub reason: Option<String>,
    /// Token estimate the PolicyGate uses to preempt overflow.
    #[serde(default)]
    pub estimated_tokens: u64,
}

#[async_trait]
pub trait LlmAdapter: Send + Sync {
    /// Run one LLM round-trip. The engine fires `LlmRequested` before
    /// calling this and `Finished` (or `Aborted`) after.
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse>;
}

/// A.3 — dispatches tool-use blocks the LLM requested. The engine
/// holds no opinion on *how* a tool runs (vac_tools owns dispatch);
/// the dispatcher closes the loop. Drivers plug their concrete
/// dispatcher at `submit_one` / `submit_stream` call time.
#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    /// Run a single tool call. Returns the result envelope the
    /// engine records in the transcript + emits as a
    /// `ToolResult` event.
    async fn dispatch(
        &self,
        call: &ToolCallRequest,
    ) -> EngineResult<vac_tool_core::ToolResultEnvelope>;
}

/// Always-fail dispatcher. Default slot when no dispatcher is
/// attached — lets the engine keep the streaming shape without
/// requiring every caller to provide one today.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedDispatcher;

#[async_trait]
impl ToolDispatcher for UnsupportedDispatcher {
    async fn dispatch(
        &self,
        call: &ToolCallRequest,
    ) -> EngineResult<vac_tool_core::ToolResultEnvelope> {
        Err(crate::error::EngineError::Other(format!(
            "no ToolDispatcher attached — cannot run tool '{}'",
            call.name,
        )))
    }
}

/// NS.5 — deterministic playback adapter. Loads a JSON cassette
/// mapping `prompt_hash → LlmResponse` and returns the recorded
/// response on match. Unmatched prompts surface a clear error so
/// tests fail loudly on drift rather than silently replaying stale
/// responses.
///
/// Cassette schema (on disk):
/// ```json
/// {
///   "provider": "anthropic",
///   "model": "claude-sonnet-4-6",
///   "entries": [
///     { "prompt": "<exact text>", "response": { ... LlmResponse ... } }
///   ]
/// }
/// ```
/// Matching is exact-string on `prompt` today; a follow-up can
/// swap to request-hash when non-prompt fields (context, tool
/// results) need to participate.
#[derive(Debug, Clone)]
pub struct CassetteAdapter {
    entries: std::collections::HashMap<String, LlmResponse>,
    provider: String,
    model: String,
}

#[derive(Debug, serde::Deserialize)]
struct CassetteFile {
    provider: String,
    model: String,
    entries: Vec<CassetteEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct CassetteEntry {
    prompt: String,
    response: LlmResponse,
}

impl CassetteAdapter {
    /// Load a cassette from a JSON file.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> EngineResult<Self> {
        let raw = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            crate::error::EngineError::Other(format!(
                "cassette read {}: {e}",
                path.as_ref().display(),
            ))
        })?;
        Self::from_str(&raw)
    }

    /// Parse a cassette from an in-memory string.
    pub fn from_str(raw: &str) -> EngineResult<Self> {
        let file: CassetteFile = serde_json::from_str(raw).map_err(|e| {
            crate::error::EngineError::Other(format!("cassette parse: {e}"))
        })?;
        let entries = file
            .entries
            .into_iter()
            .map(|e| (e.prompt, e.response))
            .collect();
        Ok(Self {
            entries,
            provider: file.provider,
            model: file.model,
        })
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[async_trait]
impl LlmAdapter for CassetteAdapter {
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse> {
        self.entries.get(&req.prompt).cloned().ok_or_else(|| {
            crate::error::EngineError::Other(format!(
                "cassette miss: no entry for prompt {:?} (cassette has {} entries for {}/{})",
                &req.prompt,
                self.entries.len(),
                self.provider,
                self.model,
            ))
        })
    }
}

/// Mock adapter for tests + `--minimal` runs. Echoes the prompt with a
/// deterministic prefix so assertions can match exactly.
#[derive(Debug, Default)]
pub struct EchoAdapter;

#[async_trait]
impl LlmAdapter for EchoAdapter {
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "echo".to_string(),
            model: "echo-1".to_string(),
            content: format!("echo: {}", req.prompt),
            input_tokens: req.prompt.split_whitespace().count() as u64,
            output_tokens: 5,
            tool_calls: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_prefixes_prompt() {
        let a = EchoAdapter;
        let r = a
            .complete(LlmRequest {
                prompt: "hi there".into(),
                context: vec![],
            })
            .await
            .unwrap();
        assert_eq!(r.content, "echo: hi there");
        assert_eq!(r.provider, "echo");
        assert_eq!(r.input_tokens, 2);
    }

    #[test]
    fn llm_request_roundtrips_through_json() {
        let req = LlmRequest {
            prompt: "p".into(),
            context: vec!["c1".into(), "c2".into()],
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: LlmRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.context, req.context);
    }
}
