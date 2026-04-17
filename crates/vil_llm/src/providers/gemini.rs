//! Google Gemini (Generative Language API) provider.
//!
//! Wire format: `POST /v1beta/models/{model}:generateContent?key={API_KEY}`
//! with a body of `{ "contents": [...], "systemInstruction": {...},
//! "tools": [...], "generationConfig": {...} }`.
//! Streaming uses `:streamGenerateContent?alt=sse`.
//!
//! Env vars:
//! - `GEMINI_API_KEY`  (required)
//! - `GEMINI_BASE_URL` (optional, defaults to
//!   `https://generativelanguage.googleapis.com`)
//! - `GEMINI_MODEL`    (optional, defaults to `gemini-1.5-flash`)

use crate::error::{LlmError, LlmResult};
use crate::provider::{
    FinishReason, LlmProvider, LlmRequest, LlmResponse, Role, StreamChunk, TokenUsage, ToolCall,
    ToolDefinition,
};
use async_trait::async_trait;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

const DEFAULT_MODEL: &str = "gemini-1.5-flash";
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const PROVIDER_NAME: &str = "gemini";

pub struct GeminiProvider {
    api_key: String,
    base_url: String,
    model: String,
    http_client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new() -> Self {
        let base_url =
            std::env::var("GEMINI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

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

    fn require_api_key(&self) -> LlmResult<&str> {
        if self.api_key.is_empty() {
            Err(LlmError::ApiKeyMissing(
                PROVIDER_NAME.to_string(),
                "GEMINI_API_KEY".to_string(),
            ))
        } else {
            Ok(&self.api_key)
        }
    }

    fn build_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
    }

    fn endpoint(&self, model: &str, stream: bool) -> LlmResult<String> {
        let op = if stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let key = self.require_api_key()?;
        let mut url = format!(
            "{}/v1beta/models/{}:{}?key={}",
            self.base_url.trim_end_matches('/'),
            model,
            op,
            key
        );
        if stream {
            url.push_str("&alt=sse");
        }
        Ok(url)
    }

    fn resolve_model<'a>(&'a self, request: &'a LlmRequest) -> &'a str {
        request.model.as_deref().unwrap_or(&self.model)
    }

    fn build_request(request: &LlmRequest) -> GeminiRequest {
        let mut system_parts: Vec<GeminiPart> = Vec::new();
        let mut contents: Vec<GeminiContent> = Vec::new();

        for msg in &request.messages {
            match msg.role {
                Role::System => {
                    if !msg.content.is_empty() {
                        system_parts.push(GeminiPart::Text {
                            text: msg.content.clone(),
                        });
                    }
                }
                Role::User => {
                    let mut parts: Vec<GeminiPart> = Vec::new();
                    if !msg.content.is_empty() {
                        parts.push(GeminiPart::Text {
                            text: msg.content.clone(),
                        });
                    }
                    for img in &msg.image_parts {
                        parts.push(GeminiPart::InlineData {
                            inline_data: GeminiInlineData {
                                mime_type: img.media_type.clone(),
                                data: img.data.clone(),
                            },
                        });
                    }
                    if !parts.is_empty() {
                        contents.push(GeminiContent {
                            role: "user".to_string(),
                            parts,
                        });
                    }
                }
                Role::Assistant => {
                    let mut parts: Vec<GeminiPart> = Vec::new();
                    if !msg.content.is_empty() {
                        parts.push(GeminiPart::Text {
                            text: msg.content.clone(),
                        });
                    }
                    for tc in &msg.tool_calls {
                        parts.push(GeminiPart::FunctionCall {
                            function_call: GeminiFunctionCall {
                                name: tc.name.clone(),
                                args: tc.arguments.clone(),
                            },
                        });
                    }
                    if !parts.is_empty() {
                        contents.push(GeminiContent {
                            role: "model".to_string(),
                            parts,
                        });
                    }
                }
                Role::Tool => {
                    // Gemini function responses are role="user" with a
                    // functionResponse part referencing the tool by name.
                    let tool_name = msg.name.clone().unwrap_or_default();
                    let response_val: serde_json::Value = serde_json::from_str(&msg.content)
                        .unwrap_or_else(|_| serde_json::json!({"result": msg.content}));
                    contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![GeminiPart::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name: tool_name,
                                response: response_val,
                            },
                        }],
                    });
                }
            }
        }

        let system_instruction = if system_parts.is_empty() {
            None
        } else {
            Some(GeminiContent {
                role: "system".to_string(),
                parts: system_parts,
            })
        };

        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(vec![GeminiToolBlock {
                function_declarations: map_tools(&request.tools),
            }])
        };

        let generation_config = GeminiGenerationConfig {
            max_output_tokens: request.max_tokens,
            temperature: request.temperature,
            stop_sequences: if request.stop_sequences.is_empty() {
                None
            } else {
                Some(request.stop_sequences.clone())
            },
        };

        GeminiRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(generation_config),
        }
    }

    fn parse_response(model: String, response: GeminiResponse) -> LlmResponse {
        let mut content_text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut finish_reason_str: Option<String> = None;

        if let Some(candidate) = response.candidates.into_iter().next() {
            finish_reason_str = candidate.finish_reason;
            if let Some(parts) = candidate.content.map(|c| c.parts) {
                for part in parts {
                    match part {
                        GeminiResponsePart::Text { text } => content_text.push_str(&text),
                        GeminiResponsePart::FunctionCall { function_call } => {
                            tool_calls.push(ToolCall {
                                // Gemini does not return a call id — synthesize one
                                // from the function name + index.
                                id: format!("{}-{}", function_call.name, tool_calls.len()),
                                name: function_call.name,
                                arguments: function_call.args,
                            });
                        }
                        GeminiResponsePart::Unknown => {}
                    }
                }
            }
        }

        let usage = response.usage_metadata.unwrap_or_default();
        let finish_reason = map_finish_reason(finish_reason_str.as_deref(), !tool_calls.is_empty());

        LlmResponse {
            content: content_text,
            model,
            finish_reason,
            usage: TokenUsage {
                prompt_tokens: usage.prompt_token_count,
                completion_tokens: usage.candidates_token_count,
                total_tokens: usage.total_token_count,
                cached_tokens: 0,
                cache_creation_tokens: 0,
                cache_hit_rate: 0.0,
            },
            tool_calls,
        }
    }
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    fn supports_vision(&self) -> bool {
        // gemini-1.5-flash / pro and 2.x families are multimodal.
        true
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn max_context_tokens(&self) -> u32 {
        // 1.5-flash advertises 1M; 1.5-pro advertises 2M. Be conservative.
        1_000_000
    }

    async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse> {
        let model = self.resolve_model(request).to_string();
        let wire = Self::build_request(request);
        let body = serde_json::to_string(&wire)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;

        debug!(model = %model, "Sending Gemini request");

        let endpoint = self.endpoint(&model, false)?;
        let response = self
            .http_client
            .post(&endpoint)
            .headers(Self::build_headers())
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Gemini API error");
            return Err(LlmError::Provider {
                provider: PROVIDER_NAME.to_string(),
                message: format!("API error {}: {}", status, body),
            });
        }

        let parsed: GeminiResponse = response.json().await?;
        let llm = Self::parse_response(model, parsed);
        info!(
            model = %llm.model,
            prompt_tokens = llm.usage.prompt_tokens,
            completion_tokens = llm.usage.completion_tokens,
            "Gemini request completed"
        );
        Ok(llm)
    }

    async fn stream(&self, request: &LlmRequest) -> LlmResult<mpsc::Receiver<StreamChunk>> {
        let model = self.resolve_model(request).to_string();
        let wire = Self::build_request(request);
        let body = serde_json::to_string(&wire)
            .map_err(|e| LlmError::Other(anyhow::anyhow!("Serialize: {}", e)))?;
        let endpoint = self.endpoint(&model, true)?;

        let response = self
            .http_client
            .post(&endpoint)
            .headers(Self::build_headers())
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Provider {
                provider: PROVIDER_NAME.to_string(),
                message: format!("API error {}: {}", status, body),
            });
        }

        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = response.bytes_stream();
            let mut buf = String::new();
            let mut usage = GeminiUsageMetadata::default();
            let mut finish_reason = FinishReason::Stop;
            let mut saw_tool_call = false;
            let mut tool_call_counter: usize = 0;

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
                    if line.is_empty() {
                        continue;
                    }
                    let data = line.strip_prefix("data: ").unwrap_or(&line);
                    if data == "[DONE]" {
                        continue;
                    }
                    let Ok(val) = serde_json::from_str::<GeminiResponse>(data) else {
                        continue;
                    };

                    if let Some(u) = val.usage_metadata {
                        usage = u;
                    }
                    if let Some(candidate) = val.candidates.into_iter().next() {
                        let candidate_finish_reason = candidate.finish_reason;
                        if let Some(content) = candidate.content {
                            for part in content.parts {
                                match part {
                                    GeminiResponsePart::Text { text } => {
                                        if !text.is_empty() {
                                            let _ = tx.send(StreamChunk::Text(text)).await;
                                        }
                                    }
                                    GeminiResponsePart::FunctionCall { function_call } => {
                                        saw_tool_call = true;
                                        let id =
                                            format!("{}-{}", function_call.name, tool_call_counter);
                                        tool_call_counter += 1;
                                        let _ = tx
                                            .send(StreamChunk::ToolCallStart {
                                                id: id.clone(),
                                                name: function_call.name,
                                            })
                                            .await;
                                        let args_str = serde_json::to_string(&function_call.args)
                                            .unwrap_or_else(|_| "{}".to_string());
                                        let _ = tx
                                            .send(StreamChunk::ToolCallDelta {
                                                id,
                                                arguments_delta: args_str,
                                            })
                                            .await;
                                    }
                                    GeminiResponsePart::Unknown => {}
                                }
                            }
                        }
                        if let Some(fr) = candidate_finish_reason.as_deref() {
                            finish_reason = map_finish_reason(Some(fr), saw_tool_call);
                        }
                    }
                }
            }

            let _ = tx
                .send(StreamChunk::Done {
                    usage: TokenUsage {
                        prompt_tokens: usage.prompt_token_count,
                        completion_tokens: usage.candidates_token_count,
                        total_tokens: usage.total_token_count,
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

fn map_tools(tools: &[ToolDefinition]) -> Vec<GeminiFunctionDeclaration> {
    tools
        .iter()
        .map(|tool| GeminiFunctionDeclaration {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.input_schema.clone(),
        })
        .collect()
}

fn map_finish_reason(reason: Option<&str>, saw_tool_call: bool) -> FinishReason {
    // Gemini uses SCREAMING_SNAKE_CASE codes.
    if saw_tool_call {
        // If a function call was emitted, treat as tool use regardless of STOP.
        return FinishReason::ToolUse;
    }
    match reason {
        Some("STOP") => FinishReason::Stop,
        Some("MAX_TOKENS") => FinishReason::MaxTokens,
        Some(other)
            if other.eq_ignore_ascii_case("SAFETY") || other.eq_ignore_ascii_case("RECITATION") =>
        {
            FinishReason::Error
        }
        _ => FinishReason::Stop,
    }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiToolBlock>>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineData,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize)]
struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiToolBlock {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "maxOutputTokens", skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(rename = "stopSequences", skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata", default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    #[serde(default)]
    content: Option<GeminiResponseContent>,
    #[serde(rename = "finishReason", default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    #[serde(default)]
    parts: Vec<GeminiResponsePart>,
}

#[derive(Debug)]
enum GeminiResponsePart {
    FunctionCall {
        function_call: GeminiResponseFunctionCall,
    },
    Text {
        text: String,
    },
    /// Catch-all for parts we don't model (e.g., inlineData echoed back).
    Unknown,
}

impl<'de> Deserialize<'de> for GeminiResponsePart {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = serde_json::Value::deserialize(deserializer)?;
        if let Some(text) = raw.get("text").and_then(|v| v.as_str()) {
            return Ok(GeminiResponsePart::Text {
                text: text.to_string(),
            });
        }
        if let Some(fc) = raw.get("functionCall") {
            let call: GeminiResponseFunctionCall =
                serde_json::from_value(fc.clone()).map_err(serde::de::Error::custom)?;
            return Ok(GeminiResponsePart::FunctionCall {
                function_call: call,
            });
        }
        Ok(GeminiResponsePart::Unknown)
    }
}

#[derive(Debug, Deserialize)]
struct GeminiResponseFunctionCall {
    name: String,
    #[serde(default)]
    args: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: u64,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: u64,
    #[serde(rename = "totalTokenCount", default)]
    total_token_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{LlmRequest, Message};
    use wiremock::matchers::{body_partial_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn gemini_complete_happy_path() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "hola"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 6,
                "candidatesTokenCount": 2,
                "totalTokenCount": 8
            }
        });

        // Verify path + query key + wire shape (system -> systemInstruction;
        // user message -> contents[0].parts[0].text).
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-1.5-flash:generateContent"))
            .and(query_param("key", "gem-test"))
            .and(body_partial_json(serde_json::json!({
                "contents": [{
                    "role": "user",
                    "parts": [{"text": "hola"}]
                }],
                "systemInstruction": {
                    "role": "system",
                    "parts": [{"text": "you are friendly"}]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let provider = GeminiProvider::new()
            .with_api_key("gem-test")
            .with_base_url(&server.uri())
            .with_model("gemini-1.5-flash");

        let req = LlmRequest::new(vec![
            Message::system("you are friendly"),
            Message::user("hola"),
        ]);
        let resp = provider.complete(&req).await.expect("complete");
        assert_eq!(resp.content, "hola");
        assert_eq!(resp.usage.total_tokens, 8);
        assert_eq!(provider.name(), "gemini");
        assert!(provider.supports_vision());
    }

    #[tokio::test]
    async fn gemini_errors_without_api_key() {
        let provider = GeminiProvider::new().with_api_key("");
        let req = LlmRequest::new(vec![Message::user("ping")]);
        let err = provider.complete(&req).await.unwrap_err();
        assert!(
            matches!(err, LlmError::ApiKeyMissing(ref p, _) if p == "gemini"),
            "got: {err}"
        );
    }

    #[test]
    fn gemini_maps_tool_result_as_function_response() {
        let tool_msg = Message::tool("my_tool", "call_1", r#"{"ok": true}"#);
        let req = LlmRequest::new(vec![Message::user("go"), tool_msg]);
        let wire = GeminiProvider::build_request(&req);
        // Last content should be a user role with a functionResponse part.
        let last = wire.contents.last().expect("has last");
        assert_eq!(last.role, "user");
        let part = last.parts.first().expect("has part");
        let json = serde_json::to_value(part).expect("serialize");
        assert!(json.get("functionResponse").is_some(), "got: {json}");
        assert_eq!(json["functionResponse"]["name"].as_str(), Some("my_tool"));
    }
}
