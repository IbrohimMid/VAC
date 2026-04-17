//! Mistral La Plateforme provider — thin factory over [`super::openai_compat`].
//!
//! Mistral exposes an OpenAI-compatible `/v1/chat/completions` endpoint at
//! `https://api.mistral.ai`, so we simply configure an [`OpenAiCompatProvider`]
//! with the right name, defaults, and capability matrix.
//!
//! Env vars:
//! - `MISTRAL_API_KEY`  (required)
//! - `MISTRAL_BASE_URL` (optional, defaults to `https://api.mistral.ai`)
//! - `MISTRAL_MODEL`    (optional, defaults to `mistral-large-latest`)

use crate::providers::openai_compat::OpenAiCompatProvider;

const DEFAULT_MODEL: &str = "mistral-large-latest";
const DEFAULT_BASE_URL: &str = "https://api.mistral.ai/v1";
const PROVIDER_NAME: &str = "mistral";
const API_KEY_ENV: &str = "MISTRAL_API_KEY";

/// Build an [`OpenAiCompatProvider`] preconfigured for Mistral La Plateforme.
pub fn new() -> OpenAiCompatProvider {
    let base_url =
        std::env::var("MISTRAL_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    let api_key = std::env::var(API_KEY_ENV).unwrap_or_default();
    let model = std::env::var("MISTRAL_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    OpenAiCompatProvider::new()
        .with_name(PROVIDER_NAME)
        .with_base_url(&base_url)
        .with_base_url_env("MISTRAL_BASE_URL")
        .with_api_key(&api_key)
        .require_api_key(API_KEY_ENV)
        .with_model(&model)
        // mistral-large-latest: text-only, tool-calling, 131_072 context.
        .with_capabilities(false, true, 131_072)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LlmError;
    use crate::provider::{LlmProvider, LlmRequest, Message};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn mistral_complete_happy_path() {
        let server = MockServer::start().await;
        let resp_body = serde_json::json!({
            "id": "mistral-1",
            "object": "chat.completion",
            "model": "mistral-large-latest",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "bonjour"}
            }],
            "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer mistral-test"))
            .and(body_partial_json(
                serde_json::json!({"model": "mistral-large-latest"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp_body))
            .mount(&server)
            .await;

        let provider = new()
            .with_api_key("mistral-test")
            .with_base_url(&format!("{}/v1", server.uri()))
            .with_model("mistral-large-latest");

        let req = LlmRequest::new(vec![Message::user("salut")]);
        let resp = provider.complete(&req).await.expect("complete");
        assert_eq!(resp.content, "bonjour");
        assert_eq!(resp.model, "mistral-large-latest");
        assert_eq!(resp.usage.total_tokens, 3);
        assert_eq!(provider.name(), "mistral");
    }

    #[tokio::test]
    async fn mistral_errors_without_api_key() {
        let provider = new().with_api_key("");
        let req = LlmRequest::new(vec![Message::user("ping")]);
        let err = provider.complete(&req).await.unwrap_err();
        assert!(
            matches!(err, LlmError::ApiKeyMissing(ref p, ref env) if p == "mistral" && env == "MISTRAL_API_KEY"),
            "got: {err}"
        );
    }
}
