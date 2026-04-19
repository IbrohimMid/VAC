//! xAI (Grok) provider — thin factory over [`super::openai_compat`].
//!
//! xAI exposes an OpenAI-compatible `/v1/chat/completions` endpoint at
//! `https://api.x.ai`, so we simply configure an [`OpenAiCompatProvider`] with
//! the right name, defaults, and capability matrix.
//!
//! Env vars:
//! - `XAI_API_KEY`  (required)
//! - `XAI_BASE_URL` (optional, defaults to `https://api.x.ai`)
//! - `XAI_MODEL`    (optional, defaults to `grok-beta`)

use crate::providers::openai_compat::OpenAiCompatProvider;

const DEFAULT_MODEL: &str = "grok-beta";
const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";
const PROVIDER_NAME: &str = "xai";
const API_KEY_ENV: &str = "XAI_API_KEY";

/// Build an [`OpenAiCompatProvider`] preconfigured for xAI Grok.
pub fn new() -> OpenAiCompatProvider {
    let base_url = std::env::var("XAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    let api_key = std::env::var(API_KEY_ENV).unwrap_or_default();
    let model = std::env::var("XAI_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    OpenAiCompatProvider::new()
        .with_name(PROVIDER_NAME)
        .with_base_url(&base_url)
        .with_base_url_env("XAI_BASE_URL")
        .with_api_key(&api_key)
        .require_api_key(API_KEY_ENV)
        .with_model(&model)
        // grok-beta: text-only, tool-calling, 131_072 context.
        .with_capabilities(false, true, 131_072)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::error::LlmError;
    use crate::provider::{LlmProvider, LlmRequest, Message};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn xai_complete_happy_path() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "id": "xai-1",
            "object": "chat.completion",
            "model": "grok-beta",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "hi from grok"}
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer xai-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let provider = new()
            .with_api_key("xai-test")
            .with_base_url(&format!("{}/v1", server.uri()))
            .with_model("grok-beta");

        let req = LlmRequest::new(vec![Message::user("hello")]);
        let resp = provider.complete(&req).await.expect("complete");
        assert_eq!(resp.content, "hi from grok");
        assert_eq!(resp.model, "grok-beta");
        assert_eq!(resp.usage.total_tokens, 8);
        assert_eq!(provider.name(), "xai");
        assert_eq!(provider.max_context_tokens(), 131_072);
    }

    #[tokio::test]
    async fn xai_errors_without_api_key() {
        let provider = new().with_api_key("");
        let req = LlmRequest::new(vec![Message::user("ping")]);
        let err = provider.complete(&req).await.unwrap_err();
        assert!(
            matches!(err, LlmError::ApiKeyMissing(ref p, ref env) if p == "xai" && env == "XAI_API_KEY"),
            "got: {err}"
        );
    }
}
