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
                    message: format!("Invalid API key: {}", e),
                }
            })?,
        );
        Ok(headers)
    }

    fn map_message(msg: &Message) -> OpenAiMessage {
        let mut mapped = OpenAiMessage {
            role: match msg.role {
                Role::System => "system".to_string(),
                Role::User => "user".to_string(),
                Role::Assistant => "assistant".to_string(),
                Role::Tool => "tool".to_string(),
            },
            content: if matches!(msg.role, Role::Assistant) && !msg.tool_calls.is_empty() {
                if msg.content.is_empty() {
                    None
                } else {
                    Some(msg.content.clone())
                }
            } else {
                Some(msg.content.clone())
            },
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
        tools.iter()
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

        let endpoint = format!("{}{}", self.base_url.trim_end_matches('/'), API_ENDPOINT_PATH);
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

    async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse> {
        let response = self.do_complete(&self.build_request(request)).await?;
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Provider {
                provider: "anthropic".to_string(),
                message: "No choices returned by provider".to_string(),
            })?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| {
                let arguments = serde_json::from_str(&call.function.arguments).unwrap_or_else(
                    |_| serde_json::json!({
                        "raw": call.function.arguments
                    }),
                );
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
            "Kilo Gateway request completed"
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

    async fn stream(&self, request: &LlmRequest) -> LlmResult<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(256);

        let mut stream_request = self.build_request(request);
        stream_request.stream = true;

        let body = serde_json::to_string(&stream_request)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;
        let endpoint = format!("{}{}", self.base_url.trim_end_matches('/'), API_ENDPOINT_PATH);
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
                message: format!("API error {}: {}", status, body),
            });
        }

        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = response.bytes_stream();
            let mut buf = String::new();
            let mut usage = OpenAiUsage::default();

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
                    let Ok(val) = serde_json::from_str::<serde_json::Value>(data) else { continue };

                    // Accumulate usage if present
                    if let Some(u) = val.get("usage") {
                        usage.prompt_tokens = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(usage.prompt_tokens);
                        usage.completion_tokens = u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(usage.completion_tokens);
                        usage.total_tokens = u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(usage.total_tokens);
                    }

                    let Some(choices) = val.get("choices").and_then(|c| c.as_array()) else { continue };
                    let Some(choice) = choices.first() else { continue };
                    let delta = match choice.get("delta") { Some(d) => d, None => continue };

                    // Text delta
                    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                        if !text.is_empty() {
                            let _ = tx.send(StreamChunk::Text(text.to_string())).await;
                        }
                    }

                    // Tool call deltas
                    if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tcs {
                            let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let name = tc.get("function").and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let args_delta = tc.get("function").and_then(|f| f.get("arguments")).and_then(|v| v.as_str()).unwrap_or("").to_string();

                            if !name.is_empty() {
                                let _ = tx.send(StreamChunk::ToolCallStart { id: id.clone(), name }).await;
                            }
                            if !args_delta.is_empty() {
                                let _ = tx.send(StreamChunk::ToolCallDelta { id, arguments_delta: args_delta }).await;
                            }
                        }
                    }
                }
            }

            let _ = tx.send(StreamChunk::Done(TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            })).await;
        });

        Ok(rx)
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiToolDefinition>>,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
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
