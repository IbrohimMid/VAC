//! Generic OpenAI-compatible provider.
//!
//! Covers any backend that speaks the OpenAI `/v1/chat/completions` wire format
//! with a configurable base URL — e.g. Ollama, LM Studio, vLLM, LiteLLM, TGI
//! (openai-compat mode), Together, Groq, DeepInfra, Anyscale, etc.
//!
//! Env vars:
//! - `OPENAI_COMPAT_BASE_URL` (required) — e.g. `http://localhost:11434/v1`
//! - `OPENAI_COMPAT_API_KEY`  (optional) — many local backends don't need one
//! - `OPENAI_COMPAT_MODEL`    (required-ish) — model id the backend expects
//!
//! This module also exposes the `OpenAi*` wire types as `pub(crate)` so sibling
//! OpenAI-wire providers (xAI, Mistral) can reuse them without duplication.

use crate::error::{LlmError, LlmResult};
use crate::provider::{
    FinishReason, LlmProvider, LlmRequest, LlmResponse, Message, Role, StreamChunk, TokenUsage,
    ToolCall, ToolDefinition,
};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const CHAT_PATH: &str = "/chat/completions";

pub struct OpenAiCompatProvider {
    provider_name: String,
    api_key: String,
    /// If set, an empty `api_key` triggers `LlmError::ApiKeyMissing` with this
    /// env var name. If `None`, the key is treated as optional (local backends).
    required_api_key_env: Option<String>,
    base_url: String,
    base_url_env: String,
    model: String,
    max_context: u32,
    supports_vision: bool,
    supports_tools: bool,
    http_client: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("OPENAI_COMPAT_BASE_URL").unwrap_or_default();
        let api_key = std::env::var("OPENAI_COMPAT_API_KEY").unwrap_or_default();
        let model =
            std::env::var("OPENAI_COMPAT_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

        Self {
            provider_name: "openai_compat".to_string(),
            api_key,
            required_api_key_env: None,
            base_url,
            base_url_env: "OPENAI_COMPAT_BASE_URL".to_string(),
            model,
            max_context: 128_000,
            supports_vision: false,
            supports_tools: true,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn with_api_key(mut self, api_key: &str) -> Self {
        self.api_key = api_key.to_string();
        self
    }

    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.trim_end_matches('/').to_string();
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.provider_name = name.to_string();
        self
    }

    pub fn with_capabilities(mut self, vision: bool, tools: bool, max_context: u32) -> Self {
        self.supports_vision = vision;
        self.supports_tools = tools;
        self.max_context = max_context;
        self
    }

    /// Mark the API key as required; missing/empty keys produce
    /// [`LlmError::ApiKeyMissing`] referencing `env_var`.
    pub fn require_api_key(mut self, env_var: &str) -> Self {
        self.required_api_key_env = Some(env_var.to_string());
        self
    }

    /// Override the env-var name surfaced in "base URL not configured" errors
    /// (default: `OPENAI_COMPAT_BASE_URL`).
    pub fn with_base_url_env(mut self, env_var: &str) -> Self {
        self.base_url_env = env_var.to_string();
        self
    }

    fn build_headers(&self) -> LlmResult<HeaderMap> {
        if self.api_key.is_empty() {
            if let Some(env) = &self.required_api_key_env {
                return Err(LlmError::ApiKeyMissing(
                    self.provider_name.clone(),
                    env.clone(),
                ));
            }
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !self.api_key.is_empty() {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", self.api_key)).map_err(|e| {
                    LlmError::Provider {
                        provider: self.provider_name.clone(),
                        message: format!("Invalid API key: {}", e),
                    }
                })?,
            );
        }
        Ok(headers)
    }

    fn endpoint(&self) -> LlmResult<String> {
        if self.base_url.is_empty() {
            return Err(LlmError::Provider {
                provider: self.provider_name.clone(),
                message: format!("{} not configured", self.base_url_env),
            });
        }
        Ok(format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            CHAT_PATH
        ))
    }

    async fn do_complete(&self, request: &OpenAiChatRequest) -> LlmResult<OpenAiChatResponse> {
        let body = serde_json::to_string(request)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;

        debug!(model = %request.model, provider = %self.provider_name, "Sending OpenAI-compat request");

        let response = self
            .http_client
            .post(self.endpoint()?)
            .headers(self.build_headers()?)
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, provider = %self.provider_name, "OpenAI-compat API error");
            return Err(LlmError::Provider {
                provider: self.provider_name.clone(),
                message: format!("API error {}: {}", status, body),
            });
        }

        response.json().await.map_err(Into::into)
    }
}

impl Default for OpenAiCompatProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    fn supports_vision(&self) -> bool {
        self.supports_vision
    }

    fn supports_tools(&self) -> bool {
        self.supports_tools
    }

    fn max_context_tokens(&self) -> u32 {
        self.max_context
    }

    async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse> {
        let wire_request = build_openai_request(request, &self.model, false, self.supports_vision);
        let response = self.do_complete(&wire_request).await?;
        openai_response_to_llm(response, &self.provider_name)
    }

    async fn stream(&self, request: &LlmRequest) -> LlmResult<mpsc::Receiver<StreamChunk>> {
        let wire_request = build_openai_request(request, &self.model, true, self.supports_vision);
        openai_stream(
            &self.http_client,
            &self.endpoint()?,
            self.build_headers()?,
            &wire_request,
            self.provider_name.clone(),
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// Shared OpenAI-wire helpers (used by xAI, Mistral, and this module).
// ---------------------------------------------------------------------------

pub(crate) fn build_openai_request(
    llm_request: &LlmRequest,
    default_model: &str,
    stream: bool,
    multimodal: bool,
) -> OpenAiChatRequest {
    OpenAiChatRequest {
        model: llm_request
            .model
            .as_deref()
            .unwrap_or(default_model)
            .to_string(),
        messages: llm_request
            .messages
            .iter()
            .map(|m| map_message(m, multimodal))
            .collect(),
        max_tokens: llm_request.max_tokens.unwrap_or(4096),
        stream,
        temperature: llm_request.temperature,
        stop: if llm_request.stop_sequences.is_empty() {
            None
        } else {
            Some(llm_request.stop_sequences.clone())
        },
        tools: if llm_request.tools.is_empty() {
            None
        } else {
            Some(map_tools(&llm_request.tools))
        },
    }
}

fn map_message(msg: &Message, multimodal: bool) -> OpenAiMessage {
    // Determine content: if images present and multimodal allowed, use blocks array.
    let content_value = if multimodal && !msg.image_parts.is_empty() && msg.role == Role::User {
        let mut blocks = Vec::new();
        if !msg.content.is_empty() {
            blocks.push(serde_json::json!({"type": "text", "text": msg.content}));
        }
        for img in &msg.image_parts {
            blocks.push(serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:{};base64,{}", img.media_type, img.data)
                }
            }));
        }
        Some(serde_json::Value::Array(blocks))
    } else if matches!(msg.role, Role::Assistant) && !msg.tool_calls.is_empty() {
        if msg.content.is_empty() {
            None
        } else {
            Some(serde_json::Value::String(msg.content.clone()))
        }
    } else {
        Some(serde_json::Value::String(msg.content.clone()))
    };

    let mut mapped = OpenAiMessage {
        role: match msg.role {
            Role::System => "system".to_string(),
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
            Role::Tool => "tool".to_string(),
        },
        content: content_value,
        name: msg.name.clone(),
        tool_call_id: msg.tool_call_id.clone(),
        tool_calls: None,
    };

    if matches!(msg.role, Role::Assistant) && !msg.tool_calls.is_empty() {
        mapped.tool_calls = Some(
            msg.tool_calls
                .iter()
                .map(|call| OpenAiToolCallMessage {
                    id: call.id.clone(),
                    type_: "function".to_string(),
                    function: OpenAiToolFunctionCall {
                        name: call.name.clone(),
                        arguments: serde_json::to_string(&call.arguments)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                })
                .collect(),
        );
    }

    mapped
}

fn map_tools(tools: &[ToolDefinition]) -> Vec<OpenAiToolDefinition> {
    tools
        .iter()
        .map(|tool| OpenAiToolDefinition {
            type_: "function".to_string(),
            function: OpenAiFunctionDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.input_schema.clone(),
            },
        })
        .collect()
}

pub(crate) fn openai_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("stop") => FinishReason::Stop,
        Some("length") => FinishReason::MaxTokens,
        Some("tool_calls") => FinishReason::ToolUse,
        _ => FinishReason::Stop,
    }
}

pub(crate) fn openai_response_to_llm(
    response: OpenAiChatResponse,
    provider_name: &str,
) -> LlmResult<LlmResponse> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::Provider {
            provider: provider_name.to_string(),
            message: "No choices returned by provider".to_string(),
        })?;

    let tool_calls = choice
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|call| {
            let arguments = serde_json::from_str(&call.function.arguments)
                .unwrap_or_else(|_| serde_json::json!({"raw": call.function.arguments}));
            ToolCall {
                id: call.id,
                name: call.function.name,
                arguments,
            }
        })
        .collect::<Vec<_>>();

    let usage = response.usage.unwrap_or_default();
    let finish_reason = openai_finish_reason(choice.finish_reason.as_deref());

    info!(
        provider = provider_name,
        model = %response.model,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        "OpenAI-compat request completed"
    );

    Ok(LlmResponse {
        content: choice.message.content.unwrap_or_default(),
        model: response.model,
        finish_reason,
        usage: TokenUsage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        },
        tool_calls,
    })
}

pub(crate) async fn openai_stream(
    client: &reqwest::Client,
    endpoint: &str,
    headers: HeaderMap,
    wire_request: &OpenAiChatRequest,
    provider_name: String,
) -> LlmResult<mpsc::Receiver<StreamChunk>> {
    let (tx, rx) = mpsc::channel(256);

    let body = serde_json::to_string(wire_request)
        .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;

    let response = client
        .post(endpoint)
        .headers(headers)
        .body(body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(LlmError::Provider {
            provider: provider_name,
            message: format!("API error {}: {}", status, body),
        });
    }

    tokio::spawn(async move {
        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        let mut usage = OpenAiUsage::default();
        let mut finish_reason = FinishReason::Stop;

        while let Some(chunk) = stream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                    return;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf = buf[pos + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" {
                    continue;
                }
                let data = line.strip_prefix("data: ").unwrap_or(&line);
                let Ok(val) = serde_json::from_str::<serde_json::Value>(data) else {
                    continue;
                };

                if let Some(u) = val.get("usage") {
                    usage.prompt_tokens = u
                        .get("prompt_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(usage.prompt_tokens);
                    usage.completion_tokens = u
                        .get("completion_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(usage.completion_tokens);
                    usage.total_tokens = u
                        .get("total_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(usage.total_tokens);
                }

                let Some(choices) = val.get("choices").and_then(|c| c.as_array()) else {
                    continue;
                };
                let Some(choice) = choices.first() else {
                    continue;
                };

                if let Some(fr) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                    finish_reason = openai_finish_reason(Some(fr));
                }

                let delta = match choice.get("delta") {
                    Some(d) => d,
                    None => continue,
                };

                if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        let _ = tx.send(StreamChunk::Text(text.to_string())).await;
                    }
                }

                if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let id = tc
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args_delta = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        if !name.is_empty() {
                            let _ = tx
                                .send(StreamChunk::ToolCallStart {
                                    id: id.clone(),
                                    name,
                                })
                                .await;
                        }
                        if !args_delta.is_empty() {
                            let _ = tx
                                .send(StreamChunk::ToolCallDelta {
                                    id,
                                    arguments_delta: args_delta,
                                })
                                .await;
                        }
                    }
                }
            }
        }

        let _ = tx
            .send(StreamChunk::Done {
                usage: TokenUsage {
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    total_tokens: usage.total_tokens,
                },
                finish_reason,
            })
            .await;
    });

    Ok(rx)
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub(crate) struct OpenAiChatRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    pub max_tokens: u32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiToolDefinition>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OpenAiMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCallMessage>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OpenAiToolDefinition {
    #[serde(rename = "type")]
    pub type_: String,
    pub function: OpenAiFunctionDefinition,
}

#[derive(Debug, Serialize)]
pub(crate) struct OpenAiFunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OpenAiToolCallMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: OpenAiToolFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OpenAiToolFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiChatResponse {
    pub model: String,
    pub choices: Vec<OpenAiChoice>,
    #[serde(default)]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiChoice {
    #[serde(default)]
    pub finish_reason: Option<String>,
    pub message: OpenAiChoiceMessage,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiChoiceMessage {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAiToolCallMessage>>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct OpenAiUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{LlmRequest, Message};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn openai_compat_complete_happy_path() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "id": "chatcmpl-xyz",
            "object": "chat.completion",
            "model": "llama3.1",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "hello from local"}
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7}
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let provider = OpenAiCompatProvider::new()
            .with_base_url(&format!("{}/v1", server.uri()))
            .with_model("llama3.1")
            .with_name("ollama");

        let req = LlmRequest::new(vec![Message::user("ping")]);
        let resp = provider.complete(&req).await.expect("complete");
        assert_eq!(resp.content, "hello from local");
        assert_eq!(resp.model, "llama3.1");
        assert_eq!(resp.usage.total_tokens, 7);
        assert_eq!(provider.name(), "ollama");
    }

    #[tokio::test]
    async fn openai_compat_errors_without_base_url() {
        let provider = OpenAiCompatProvider::new().with_base_url("");
        let req = LlmRequest::new(vec![Message::user("ping")]);
        let err = provider.complete(&req).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("OPENAI_COMPAT_BASE_URL"), "got: {msg}");
    }
}
