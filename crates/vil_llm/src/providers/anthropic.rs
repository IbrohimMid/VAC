//! Anthropic Claude LLM provider implementation.

use crate::error::{LlmError, LlmResult};
use crate::provider::{
    FinishReason, LlmProvider, LlmRequest, LlmResponse, Role, StreamChunk, TokenUsage, ToolCall,
};
use async_trait::async_trait;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
const API_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    http_client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .expect("ANTHROPIC_API_KEY environment variable must be set");

        Self {
            api_key,
            model: DEFAULT_MODEL.to_string(),
            http_client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    fn role_to_anthropic(role: Role) -> &'static str {
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "user",
        }
    }

    async fn build_request(&self, llm_request: &LlmRequest) -> LlmResult<AnthropicRequest> {
        let messages: Vec<AnthropicMessage> = llm_request
            .messages
            .iter()
            .map(|msg| AnthropicMessage {
                role: Self::role_to_anthropic(msg.role).to_string(),
                content: msg.content.clone(),
            })
            .collect();

        Ok(AnthropicRequest {
            model: llm_request
                .model
                .as_deref()
                .unwrap_or(&self.model)
                .to_string(),
            messages,
            max_tokens: llm_request.max_tokens.unwrap_or(4096),
            stream: false,
        })
    }

    async fn do_complete(&self, request: &AnthropicRequest) -> LlmResult<AnthropicResponse> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_bytes(self.api_key.as_bytes()).map_err(|e| LlmError::Provider {
                provider: "anthropic".to_string(),
                message: format!("Invalid API key: {}", e),
            })?,
        );
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

        let body = serde_json::to_string(request)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;

        debug!(model = %request.model, "Sending request to Anthropic");

        let response = self
            .http_client
            .post(API_ENDPOINT)
            .headers(headers)
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Anthropic API error");
            return Err(LlmError::Provider {
                provider: "anthropic".to_string(),
                message: format!("API error {}: {}", status, body),
            });
        }

        let anthropic_response: AnthropicResponse = response.json().await?;

        Ok(anthropic_response)
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
        let anthropic_request = self.build_request(request).await?;
        let response = self.do_complete(&anthropic_request).await?;

        let content = response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_default();

        let tool_calls: Vec<ToolCall> = response
            .content
            .iter()
            .filter_map(|c| {
                c.tool_use.as_ref().map(|tool_use| ToolCall {
                    id: tool_use.id.clone(),
                    name: tool_use.input.name.clone().unwrap_or_default(),
                    arguments: tool_use.input.arguments.clone(),
                })
            })
            .collect();

        let finish_reason = match response.stop_reason.as_deref() {
            Some("end_turn") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::MaxTokens,
            Some("tool_use") => FinishReason::ToolUse,
            _ => FinishReason::Stop,
        };

        info!(
            model = %response.model,
            input_tokens = response.usage.input_tokens,
            output_tokens = response.usage.output_tokens,
            "Anthropic request completed"
        );

        Ok(LlmResponse {
            content,
            model: response.model,
            finish_reason,
            usage: TokenUsage {
                prompt_tokens: response.usage.input_tokens as u64,
                completion_tokens: response.usage.output_tokens as u64,
                total_tokens: (response.usage.input_tokens + response.usage.output_tokens) as u64,
            },
            tool_calls,
        })
    }

    async fn stream(&self, request: &LlmRequest) -> LlmResult<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(100);

        let anthropic_request = AnthropicStreamRequest {
            model: request.model.as_deref().unwrap_or(&self.model).to_string(),
            messages: request
                .messages
                .iter()
                .map(|msg| AnthropicMessage {
                    role: Self::role_to_anthropic(msg.role).to_string(),
                    content: msg.content.clone(),
                })
                .collect(),
            max_tokens: request.max_tokens.unwrap_or(4096),
            stream: true,
        };

        let api_key = self.api_key.clone();
        let http_client = self.http_client.clone();

        tokio::spawn(async move {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            if let Ok(key) = HeaderValue::from_bytes(api_key.as_bytes()) {
                headers.insert("x-api-key", key);
            }
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

            let body = match serde_json::to_string(&anthropic_request) {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx
                        .send(StreamChunk::Error(format!("Serialization: {}", e)))
                        .await;
                    return;
                }
            };

            let response = match http_client
                .post(API_ENDPOINT)
                .headers(headers)
                .body(body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(StreamChunk::Error(format!("Request: {}", e))).await;
                    return;
                }
            };

            if !response.status().is_success() {
                let _ = tx
                    .send(StreamChunk::Error(format!(
                        "API error: {}",
                        response.status()
                    )))
                    .await;
                return;
            }

            let mut stream = response.bytes_stream();

            use futures::stream::StreamExt;

            let mut buffer = String::new();
            let mut current_tool_id: Option<String> = None;
            #[allow(unused_variables)]
            let current_tool_name: Option<String> = None;
            let mut current_args = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            buffer.push_str(&text);
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(StreamChunk::Error(format!("Read: {}", e))).await;
                        break;
                    }
                }

                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer.drain(..newline_pos + 1).collect::<String>();
                    let line = line.trim();

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            let _ = tx.send(StreamChunk::Done(TokenUsage::default())).await;
                            break;
                        }

                        if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                            match event.type_.as_deref() {
                                Some("content_block_delta") => {
                                    if let Some(delta) = event.delta {
                                        if let Some(text) = delta.text {
                                            let _ = tx.send(StreamChunk::Text(text)).await;
                                        } else if let Some(input_json) = delta.input_json {
                                            current_args.push_str(&input_json);

                                            if let Ok(args) =
                                                serde_json::Value::from_str(&current_args)
                                            {
                                                let _ = tx
                                                    .send(StreamChunk::ToolCallDelta {
                                                        id: current_tool_id
                                                            .as_ref()
                                                            .cloned()
                                                            .unwrap_or_default(),
                                                        arguments_delta: args.to_string(),
                                                    })
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                Some("content_block_start") => {
                                    if let Some(block) = event.block {
                                        if let Some(tool_use) = block.tool_use {
                                            current_tool_id = Some(tool_use.id.clone());
                                            let _ = tx
                                                .send(StreamChunk::ToolCallStart {
                                                    id: tool_use.id,
                                                    name: tool_use.name,
                                                })
                                                .await;
                                        }
                                    }
                                }
                                Some("message_delta") => {
                                    if let Some(usage) = event.usage {
                                        let _ = tx
                                            .send(StreamChunk::Done(TokenUsage {
                                                prompt_tokens: 0,
                                                completion_tokens: usage.output_tokens,
                                                total_tokens: usage.output_tokens,
                                            }))
                                            .await;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(rename = "max_tokens")]
    max_tokens: u32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicStreamRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(rename = "max_tokens")]
    max_tokens: u32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    type_: Option<String>,
    role: String,
    content: Vec<AnthropicContent>,
    model: String,
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicContent {
    #[serde(rename = "type")]
    type_: Option<String>,
    text: Option<String>,
    #[serde(rename = "tool_use")]
    tool_use: Option<AnthropicToolUse>,
}

#[derive(Debug, Deserialize)]
struct AnthropicToolUse {
    id: String,
    name: String,
    input: AnthropicToolInput,
}

#[derive(Debug, Deserialize)]
struct AnthropicToolInput {
    name: Option<String>,
    arguments: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(rename = "input_tokens")]
    input_tokens: u64,
    #[serde(rename = "output_tokens")]
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    type_: Option<String>,
    delta: Option<AnthropicStreamDelta>,
    block: Option<AnthropicStreamBlock>,
    usage: Option<AnthropicStreamUsage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    type_: Option<String>,
    text: Option<String>,
    #[serde(rename = "input_json")]
    input_json: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicStreamBlock {
    #[serde(rename = "type")]
    type_: Option<String>,
    #[serde(rename = "tool_use")]
    tool_use: Option<AnthropicToolUse>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamUsage {
    #[serde(rename = "output_tokens")]
    output_tokens: u64,
}
