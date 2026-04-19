//! Kilo Gateway provider implementation.

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

const DEFAULT_MODEL: &str = "kilo-auto/free";
const API_ENDPOINT_PATH: &str = "/api/gateway/chat/completions";

pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    model: String,
    http_client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        let base_url =
            std::env::var("KILO_GATEWAY_URL").unwrap_or_else(|_| "https://api.kilo.ai".to_string());
        let api_key = std::env::var("KILO_API_KEY")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .unwrap_or_default();
        let model = std::env::var("KILO_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

        Self {
            api_key,
            base_url,
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
                "anthropic".to_string(),
                "KILO_API_KEY".to_string(),
            ));
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key)).map_err(|e| {
                LlmError::Provider {
                    provider: "anthropic".to_string(),
                    status: None,
                    message: format!("Invalid API key: {}", e),
                }
            })?,
        );
        Ok(headers)
    }

    fn map_message(msg: &Message, cache_marked: bool) -> OpenAiMessage {
        // Determine content: if images present or the block must carry
        // cache_control, emit a content-blocks array; otherwise keep the
        // compact string form.
        let content_value = if !msg.image_parts.is_empty() && msg.role == Role::User {
            let mut blocks = Vec::new();
            if !msg.content.is_empty() {
                let mut text_block = serde_json::json!({"type": "text", "text": msg.content});
                if cache_marked {
                    if let Some(obj) = text_block.as_object_mut() {
                        obj.insert(
                            "cache_control".to_string(),
                            serde_json::json!({"type": "ephemeral"}),
                        );
                    }
                }
                blocks.push(text_block);
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
            } else if cache_marked {
                Some(serde_json::Value::Array(vec![serde_json::json!({
                    "type": "text",
                    "text": msg.content,
                    "cache_control": {"type": "ephemeral"},
                })]))
            } else {
                Some(serde_json::Value::String(msg.content.clone()))
            }
        } else if cache_marked {
            Some(serde_json::Value::Array(vec![serde_json::json!({
                "type": "text",
                "text": msg.content,
                "cache_control": {"type": "ephemeral"},
            })]))
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

    fn build_request(&self, llm_request: &LlmRequest) -> OpenAiChatRequest {
        OpenAiChatRequest {
            model: llm_request
                .model
                .as_deref()
                .unwrap_or(&self.model)
                .to_string(),
            messages: llm_request
                .messages
                .iter()
                .enumerate()
                .map(|(i, m)| Self::map_message(m, llm_request.is_cache_marked(i)))
                .collect(),
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

    async fn do_complete(&self, request: &OpenAiChatRequest) -> LlmResult<OpenAiChatResponse> {
        let body = serde_json::to_string(request)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;

        debug!(model = %request.model, "Sending request to Kilo Gateway");

        let endpoint = format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            API_ENDPOINT_PATH
        );
        let response = self
            .http_client
            .post(&endpoint)
            .headers(self.build_headers()?)
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Kilo Gateway API error");
            return Err(LlmError::Provider {
                provider: "anthropic".to_string(),
                status: Some(status.as_u16()),
                message: format!("API error {}: {}", status, body),
            });
        }

        response.json().await.map_err(Into::into)
    }

    fn finish_reason(reason: Option<&str>) -> FinishReason {
        match reason {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::MaxTokens,
            Some("tool_calls") => FinishReason::ToolUse,
            _ => FinishReason::Stop,
        }
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn supports_vision(&self) -> bool {
        // Claude 3/3.5/4 models all support images via Kilo Gateway's OpenAI-compat wire.
        true
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn max_context_tokens(&self) -> u32 {
        // Claude 3.5 / Sonnet 4 family: 200K context.
        200_000
    }

    async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse> {
        let response = self.do_complete(&self.build_request(request)).await?;
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Provider {
                provider: "anthropic".to_string(),
                status: None,
                message: "No choices returned by provider".to_string(),
            })?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| {
                let arguments =
                    serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| {
                        serde_json::json!({
                            "raw": call.function.arguments
                        })
                    });
                ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments,
                }
            })
            .collect::<Vec<_>>();

        let usage = response.usage.unwrap_or_default();
        let finish_reason = Self::finish_reason(choice.finish_reason.as_deref());

        info!(
            model = %response.model,
            prompt_tokens = usage.prompt_tokens,
            completion_tokens = usage.completion_tokens,
            cache_read = usage.cache_read_input_tokens,
            cache_creation = usage.cache_creation_input_tokens,
            "Kilo Gateway request completed"
        );

        let mut token_usage = TokenUsage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_tokens: usage.cache_read_input_tokens,
            cache_creation_tokens: usage.cache_creation_input_tokens,
            cache_hit_rate: 0.0,
        };
        token_usage.recompute_cache_hit_rate();

        Ok(LlmResponse {
            content: choice.message.content.unwrap_or_default(),
            model: response.model,
            finish_reason,
            usage: token_usage,
            tool_calls,
        })
    }

    async fn stream(&self, request: &LlmRequest) -> LlmResult<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(256);

        let mut stream_request = self.build_request(request);
        stream_request.stream = true;

        let body = serde_json::to_string(&stream_request)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;
        let endpoint = format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            API_ENDPOINT_PATH
        );
        let response = self
            .http_client
            .post(&endpoint)
            .headers(self.build_headers()?)
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Provider {
                provider: "anthropic".to_string(),
                status: Some(status.as_u16()),
                message: format!("API error {}: {}", status, body),
            });
        }

        tokio::spawn(async move {
            use crate::streaming::ToolCallAssembler;
            use futures::StreamExt;
            let mut stream = response.bytes_stream();
            let mut buf = String::new();
            let mut usage = OpenAiUsage::default();
            let mut finish_reason = crate::provider::FinishReason::Stop;
            let mut assembler = ToolCallAssembler::new();

            while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete SSE lines
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

                    // Accumulate usage if present
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
                        usage.cache_read_input_tokens = u
                            .get("cache_read_input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(usage.cache_read_input_tokens);
                        usage.cache_creation_input_tokens = u
                            .get("cache_creation_input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(usage.cache_creation_input_tokens);
                    }

                    let Some(choices) = val.get("choices").and_then(|c| c.as_array()) else {
                        continue;
                    };
                    let Some(choice) = choices.first() else {
                        continue;
                    };

                    // Parse finish_reason if present
                    if let Some(fr) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                        finish_reason = Self::finish_reason(Some(fr));
                    }

                    let delta = match choice.get("delta") {
                        Some(d) => d,
                        None => continue,
                    };

                    // Text delta
                    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                        if !text.is_empty() {
                            let _ = tx.send(StreamChunk::Text(text.to_string())).await;
                        }
                    }

                    // Tool call deltas: feed through the assembler so upstream
                    // consumers never see half-parsed JSON. ToolCallStart/Delta
                    // are still emitted for low-level/debug consumers, but the
                    // authoritative result is ToolCallComplete on stream end.
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

                            // Register with the assembler whenever we have an id,
                            // regardless of whether this delta carried a name —
                            // `start` safely skips overwriting with an empty name.
                            if !id.is_empty() {
                                assembler.start(&id, &name);
                            }
                            if !name.is_empty() {
                                let _ = tx
                                    .send(StreamChunk::ToolCallStart {
                                        id: id.clone(),
                                        name,
                                    })
                                    .await;
                            }
                            if !args_delta.is_empty() {
                                assembler.push_delta(&id, &args_delta);
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

            // Drain the assembler: the OpenAI-wire stream has no explicit
            // ContentBlockStop per tool, so finalize all outstanding partials
            // here and emit one ToolCallComplete per tool in announcement order.
            for finalized in assembler.finalize_all() {
                let _ = tx.send(StreamChunk::ToolCallComplete(finalized)).await;
            }

            let mut done_usage = TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                cached_tokens: usage.cache_read_input_tokens,
                cache_creation_tokens: usage.cache_creation_input_tokens,
                cache_hit_rate: 0.0,
            };
            done_usage.recompute_cache_hit_rate();
            let _ = tx
                .send(StreamChunk::Done {
                    usage: done_usage,
                    finish_reason,
                })
                .await;
        });

        Ok(rx)
    }
}

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
    /// Content can be a string or an array of content blocks (for multimodal).
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
    /// Anthropic-native prompt-cache fields. The gateway proxies them through
    /// verbatim when they're present. Both default to `0` on providers that
    /// don't emit them.
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::provider::{LlmRequest, Message};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn cache_marked_message_serializes_with_cache_control() {
        let msg = Message::system("frozen system prompt");
        let mapped = AnthropicProvider::map_message(&msg, true);
        let content = mapped.content.unwrap();
        // Should be an array with a text block carrying cache_control
        let arr = content.as_array().expect("content should be array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn unmarked_message_stays_as_string() {
        let msg = Message::user("hi");
        let mapped = AnthropicProvider::map_message(&msg, false);
        assert!(matches!(mapped.content, Some(serde_json::Value::String(_))));
    }

    #[test]
    fn build_request_applies_cache_control_at_flagged_indices() {
        let provider = AnthropicProvider::new().with_api_key("test-key");
        let req = LlmRequest::new(vec![Message::system("shared"), Message::user("varies")])
            .with_cache_control([0]);

        let built = provider.build_request(&req);
        // Message 0 should be content-blocks with cache_control.
        let m0_content = built.messages[0].content.as_ref().unwrap();
        assert!(m0_content.is_array(), "idx 0 should be blocks");
        assert_eq!(
            m0_content.as_array().unwrap()[0]["cache_control"]["type"],
            "ephemeral"
        );
        // Message 1 should stay as string (unmarked).
        let m1_content = built.messages[1].content.as_ref().unwrap();
        assert!(m1_content.is_string(), "idx 1 should be string");
    }

    #[tokio::test]
    async fn complete_parses_cache_tokens_and_hit_rate() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "model": "kilo-auto/free",
            "choices": [{
                "finish_reason": "stop",
                "message": {"content": "ok"},
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 250,
                "cache_read_input_tokens": 900,
                "cache_creation_input_tokens": 75,
            },
        });
        Mock::given(method("POST"))
            .and(path("/api/gateway/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let provider = AnthropicProvider::new()
            .with_api_key("test-key")
            .with_base_url(&server.uri());
        let req = LlmRequest::new(vec![Message::user("hello")]);
        let resp = provider
            .complete(&req)
            .await
            .expect("request should succeed");

        assert_eq!(resp.usage.prompt_tokens, 100);
        assert_eq!(resp.usage.completion_tokens, 50);
        assert_eq!(resp.usage.cached_tokens, 900);
        assert_eq!(resp.usage.cache_creation_tokens, 75);
        // 900 / (100 + 900) = 0.9
        assert!((resp.usage.cache_hit_rate - 0.9).abs() < 1e-9);
    }

    #[test]
    fn cache_hit_rate_zero_on_empty_usage() {
        let usage = crate::provider::TokenUsage::default();
        assert_eq!(usage.cache_hit_rate, 0.0);
    }

    #[test]
    fn recompute_handles_zero_safely() {
        let mut u = crate::provider::TokenUsage::default();
        u.recompute_cache_hit_rate();
        assert_eq!(u.cache_hit_rate, 0.0);

        u.prompt_tokens = 10;
        u.cached_tokens = 0;
        u.recompute_cache_hit_rate();
        assert_eq!(u.cache_hit_rate, 0.0);

        u.prompt_tokens = 0;
        u.cached_tokens = 10;
        u.recompute_cache_hit_rate();
        assert!((u.cache_hit_rate - 1.0).abs() < 1e-9);
    }
}
