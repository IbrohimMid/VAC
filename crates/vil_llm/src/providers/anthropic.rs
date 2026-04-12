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
        let (tx, rx) = mpsc::channel(100);
        let response = self.complete(request).await?;

        tokio::spawn(async move {
            if !response.content.is_empty() {
                let _ = tx.send(StreamChunk::Text(response.content.clone())).await;
            }

            for call in &response.tool_calls {
                let _ = tx
                    .send(StreamChunk::ToolCallStart {
                        id: call.id.clone(),
                        name: call.name.clone(),
                    })
                    .await;
                let _ = tx
                    .send(StreamChunk::ToolCallDelta {
                        id: call.id.clone(),
                        arguments_delta: call.arguments.to_string(),
                    })
                    .await;
            }

            let _ = tx.send(StreamChunk::Done(response.usage)).await;
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
