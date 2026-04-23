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
}

#[async_trait]
pub trait LlmAdapter: Send + Sync {
    /// Run one LLM round-trip. The engine fires `LlmRequested` before
    /// calling this and `Finished` (or `Aborted`) after.
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse>;
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
