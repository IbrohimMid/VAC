//! OpenAI Chat Completions provider.
//!
//! Implements [`LlmProvider`] against the OpenAI `/v1/chat/completions` API.
//! Uses the flat `tool_calls` array wire format (as opposed to Anthropic's
//! content-block shape).

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

const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const API_ENDPOINT_PATH: &str = "/v1/chat/completions";
const PROVIDER_NAME: &str = "openai";

pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    model: String,
    http_client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new() -> Self {
        let base_url =
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

        Self {
            api_key,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
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

    fn build_headers(&self) -> LlmResult<HeaderMap> {
        if self.api_key.is_empty() {
            return Err(LlmError::ApiKeyMissing(
                PROVIDER_NAME.to_string(),
                "OPENAI_API_KEY".to_string(),
            ));
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key)).map_err(|e| {
                LlmError::Provider {
                    provider: PROVIDER_NAME.to_string(),
                    message: format!("Invalid API key: {}", e),
                }
            })?,
        );
        Ok(headers)
    }

    fn endpoint(&self) -> String {
        format!("{}{}", self.base_url, API_ENDPOINT_PATH)
    }

    fn map_message(msg: &Message) -> OpenAiMessage {
        // For multimodal user messages, encode content as an array of parts.
        let content_value = if !msg.image_parts.is_empty() && msg.role == Role::User {
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
            // OpenAI accepts null/omitted content when an assistant turn is
            // purely a tool-call dispatch.
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
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            }
            .to_string(),
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

    fn build_request(&self, llm_request: &LlmRequest) -> OpenAiChatRequest {
        OpenAiChatRequest {
            model: llm_request
                .model
                .as_deref()
                .unwrap_or(&self.model)
                .to_string(),
            messages: llm_request.messages.iter().map(Self::map_message).collect(),
            max_tokens: llm_request.max_tokens.unwrap_or(4096),
            temperature: llm_request.temperature,
            stream: false,
            tools: if llm_request.tools.is_empty() {
                None
            } else {
                Some(Self::map_tools(&llm_request.tools))
            },
        }
    }

    fn finish_reason(reason: Option<&str>) -> FinishReason {
        match reason {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::MaxTokens,
            Some("tool_calls" | "function_call") => FinishReason::ToolUse,
            _ => FinishReason::Stop,
        }
    }
}

impl Default for OpenAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse> {
        let body_struct = self.build_request(request);
        let body = serde_json::to_string(&body_struct)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;

        debug!(model = %body_struct.model, provider = PROVIDER_NAME, "Sending OpenAI chat request");

        let response = self
            .http_client
            .post(self.endpoint())
            .headers(self.build_headers()?)
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let err_body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %err_body, "OpenAI API error");
            if status.as_u16() == 429 {
                return Err(LlmError::RateLimited(PROVIDER_NAME.to_string(), 0));
            }
            return Err(LlmError::Provider {
                provider: PROVIDER_NAME.to_string(),
                message: format!("API error {}: {}", status, err_body),
            });
        }

        let parsed: OpenAiChatResponse = response.json().await?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Provider {
                provider: PROVIDER_NAME.to_string(),
                message: "No choices returned by provider".to_string(),
            })?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| {
                let arguments = serde_json::from_str(&call.function.arguments)
                    .unwrap_or_else(|_| serde_json::json!({ "raw": call.function.arguments }));
                ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments,
                }
            })
            .collect::<Vec<_>>();

        let usage = parsed.usage.unwrap_or_default();
        let finish_reason = Self::finish_reason(choice.finish_reason.as_deref());

        info!(
            model = %parsed.model,
            prompt_tokens = usage.prompt_tokens,
            completion_tokens = usage.completion_tokens,
            "OpenAI request completed"
        );

        Ok(LlmResponse {
            content: choice.message.content.unwrap_or_default(),
            model: parsed.model,
            finish_reason,
            usage: TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                cached_tokens: 0,
                cache_creation_tokens: 0,
                cache_hit_rate: 0.0,
            },
            tool_calls,
        })
    }

    async fn stream(&self, request: &LlmRequest) -> LlmResult<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(256);

        let mut stream_request = self.build_request(request);
        stream_request.stream = true;

        let body = serde_json::to_string(&stream_request)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;

        debug!(model = %stream_request.model, provider = PROVIDER_NAME, "Opening OpenAI SSE stream");

        let response = self
            .http_client
            .post(self.endpoint())
            .headers(self.build_headers()?)
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let err_body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %err_body, "OpenAI streaming error");
            if status.as_u16() == 429 {
                return Err(LlmError::RateLimited(PROVIDER_NAME.to_string(), 0));
            }
            return Err(LlmError::Provider {
                provider: PROVIDER_NAME.to_string(),
                message: format!("API error {}: {}", status, err_body),
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
                    buf.drain(..=pos);

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
                        finish_reason = Self::finish_reason(Some(fr));
                    }

                    let Some(delta) = choice.get("delta") else {
                        continue;
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
                        cached_tokens: 0,
                        cache_creation_tokens: 0,
                        cache_hit_rate: 0.0,
                    },
                    finish_reason,
                })
                .await;
        });

        Ok(rx)
    }
}

// --- Wire types -------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiToolDefinition>>,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCallMessage>>,
}

#[derive(Debug, Serialize)]
struct OpenAiToolDefinition {
    #[serde(rename = "type")]
    type_: String,
    function: OpenAiFunctionDefinition,
}

#[derive(Debug, Serialize)]
struct OpenAiFunctionDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolCallMessage {
    id: String,
    #[serde(rename = "type")]
    type_: String,
    function: OpenAiToolFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    model: String,
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    finish_reason: Option<String>,
    message: OpenAiChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoiceMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCallMessage>>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

// --- Tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{CacheControlHint, Message, ToolDefinition};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_provider(base_url: &str) -> OpenAiProvider {
        OpenAiProvider::new()
            .with_api_key("sk-test-1234")
            .with_base_url(base_url)
            .with_model("gpt-4o")
    }

    #[test]
    fn build_headers_errors_without_api_key() {
        let p = OpenAiProvider {
            api_key: String::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            http_client: reqwest::Client::new(),
        };
        let err = p.build_headers().unwrap_err();
        match err {
            LlmError::ApiKeyMissing(provider, env) => {
                assert_eq!(provider, "openai");
                assert_eq!(env, "OPENAI_API_KEY");
            }
            other => panic!("expected ApiKeyMissing, got {:?}", other),
        }
    }

    #[test]
    fn map_message_assistant_tool_call_flat_format() {
        let call = ToolCall {
            id: "call_abc".to_string(),
            name: "get_weather".to_string(),
            arguments: serde_json::json!({"city": "SF"}),
        };
        let msg = Message::assistant_with_tool_calls("", vec![call]);
        let mapped = OpenAiProvider::map_message(&msg);
        assert_eq!(mapped.role, "assistant");
        assert!(mapped.content.is_none(), "content should be omitted");
        let tcs = mapped.tool_calls.expect("tool_calls must be Some");
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "call_abc");
        assert_eq!(tcs[0].type_, "function");
        assert_eq!(tcs[0].function.name, "get_weather");
        // OpenAI expects arguments as JSON-encoded STRING, not an object.
        assert_eq!(tcs[0].function.arguments, r#"{"city":"SF"}"#);
    }

    #[test]
    fn map_message_tool_role_roundtrip() {
        let msg = Message::tool("get_weather", "call_abc", "72F, sunny");
        let mapped = OpenAiProvider::map_message(&msg);
        assert_eq!(mapped.role, "tool");
        assert_eq!(mapped.tool_call_id.as_deref(), Some("call_abc"));
        assert_eq!(mapped.name.as_deref(), Some("get_weather"));
    }

    #[tokio::test]
    async fn complete_simple_text_response() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hello there" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test-1234"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let req = LlmRequest::new(vec![Message::user("Hi")]);
        let resp = provider.complete(&req).await.expect("complete ok");

        assert_eq!(resp.content, "Hello there");
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.usage.total_tokens, 7);
        assert!(resp.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn complete_request_body_shape_matches_openai_schema() {
        let server = MockServer::start().await;

        // Capture body via a mock that only matches if shape is right.
        use wiremock::matchers::body_json;
        let expected = serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "Hi"}
            ],
            "max_tokens": 256,
            "temperature": 0.5,
            "stream": false,
            "tools": [{
                "type": "function",
                "function": {
                    "name": "add",
                    "description": "add two numbers",
                    "parameters": {"type": "object", "properties": {"a": {"type": "number"}}}
                }
            }]
        });

        let response_body = serde_json::json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "ok" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_json(&expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let req = LlmRequest {
            messages: vec![Message::system("You are helpful"), Message::user("Hi")],
            model: Some("gpt-4o".to_string()),
            max_tokens: Some(256),
            temperature: Some(0.5),
            stop_sequences: vec![],
            tools: vec![ToolDefinition {
                name: "add".to_string(),
                description: "add two numbers".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {"a": {"type": "number"}}
                }),
            }],
            cache_control_blocks: vec![],
            cache_control_hint: CacheControlHint::Ephemeral,
        };

        let resp = provider.complete(&req).await.expect("body matched");
        assert_eq!(resp.content, "ok");
    }

    #[tokio::test]
    async fn complete_parses_tool_calls_flat_array() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "id": "chatcmpl-2",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\":\"SF\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let req = LlmRequest::new(vec![Message::user("Weather in SF?")]);
        let resp = provider.complete(&req).await.expect("tool_calls ok");

        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
        assert_eq!(resp.tool_calls.len(), 1);
        let tc = &resp.tool_calls[0];
        assert_eq!(tc.id, "call_1");
        assert_eq!(tc.name, "get_weather");
        assert_eq!(tc.arguments, serde_json::json!({"city": "SF"}));
    }

    #[tokio::test]
    async fn complete_rate_limit_returns_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let req = LlmRequest::new(vec![Message::user("Hi")]);
        let err = provider.complete(&req).await.unwrap_err();
        assert!(
            matches!(err, LlmError::RateLimited(ref p, _) if p == "openai"),
            "expected RateLimited, got {:?}",
            err
        );
    }

    #[tokio::test]
    async fn complete_error_body_does_not_leak_api_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server boom"))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let req = LlmRequest::new(vec![Message::user("Hi")]);
        let err = provider.complete(&req).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains("sk-test-1234"),
            "error message must not contain api key: {msg}"
        );
    }

    #[tokio::test]
    async fn stream_assembles_text_and_tool_call_deltas() {
        let server = MockServer::start().await;

        // Two text deltas, then a tool-call delta pair, then finish_reason,
        // then [DONE].
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_1\",\"function\":{\"name\":\"t\",\"arguments\":\"{\\\"x\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_1\",\"function\":{\"arguments\":\"1}\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4,\"total_tokens\":7}}\n\n",
            "data: [DONE]\n\n",
        );

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let req = LlmRequest::new(vec![Message::user("Hi")]);
        let mut rx = provider.stream(&req).await.expect("stream opened");

        let mut text = String::new();
        let mut tool_call_started = false;
        let mut args = String::new();
        let mut done = None;
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Text(t) => text.push_str(&t),
                StreamChunk::ToolCallStart { name, id } => {
                    assert_eq!(id, "call_1");
                    assert_eq!(name, "t");
                    tool_call_started = true;
                }
                StreamChunk::ToolCallDelta {
                    id,
                    arguments_delta,
                } => {
                    assert_eq!(id, "call_1");
                    args.push_str(&arguments_delta);
                }
                StreamChunk::Done {
                    usage,
                    finish_reason,
                } => {
                    done = Some((usage, finish_reason));
                    break;
                }
                StreamChunk::Error(e) => panic!("stream error: {e}"),
                StreamChunk::ToolCallComplete(_) => { /* aggregated by assembler; ignored in test */ }
            }
        }

        assert_eq!(text, "Hello");
        assert!(tool_call_started);
        assert_eq!(args, r#"{"x":1}"#);
        let (usage, finish) = done.expect("stream ended with Done");
        assert_eq!(finish, FinishReason::ToolUse);
        assert_eq!(usage.total_tokens, 7);
    }
}
