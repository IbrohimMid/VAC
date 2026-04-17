use vil_llm::provider::{FinishReason, LlmProvider, LlmRequest, Message, StreamChunk};
use vil_llm::providers::anthropic::AnthropicProvider;
use vil_llm::providers::gemini::GeminiProvider;
use vil_llm::providers::mistral;
use vil_llm::providers::openai_compat::OpenAiCompatProvider;
use vil_llm::providers::xai;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn collect_stream(provider: &dyn LlmProvider, request: &LlmRequest) -> Vec<StreamChunk> {
    let mut rx = provider.stream(request).await.expect("stream opened");
    let mut chunks = Vec::new();

    while let Some(chunk) = rx.recv().await {
        let is_terminal = matches!(chunk, StreamChunk::Done { .. } | StreamChunk::Error(_));
        chunks.push(chunk);
        if is_terminal {
            break;
        }
    }

    chunks
}

fn openai_style_sse() -> String {
    concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_1\",\"function\":{\"name\":\"t\",\"arguments\":\"{\\\"x\\\":\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_1\",\"function\":{\"arguments\":\"1}\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4,\"total_tokens\":7}}\n\n",
        "data: [DONE]\n\n",
    )
    .to_string()
}

fn assert_openai_stream_chunks(chunks: &[StreamChunk], expect_complete: bool) {
    assert!(
        chunks
            .iter()
            .any(|chunk| matches!(chunk, StreamChunk::Text(text) if text == "Hel"))
    );
    assert!(
        chunks
            .iter()
            .any(|chunk| matches!(chunk, StreamChunk::Text(text) if text == "lo"))
    );
    assert!(chunks.iter().any(|chunk| matches!(
        chunk,
        StreamChunk::ToolCallStart { id, name } if id == "call_1" && name == "t"
    )));
    let assembled_args = chunks.iter().fold(String::new(), |mut acc, chunk| {
        if let StreamChunk::ToolCallDelta {
            id,
            arguments_delta,
        } = chunk
            && id == "call_1"
        {
            acc.push_str(arguments_delta);
        }
        acc
    });
    assert_eq!(assembled_args, r#"{"x":1}"#);
    if expect_complete {
        assert!(chunks.iter().any(|chunk| matches!(
            chunk,
            StreamChunk::ToolCallComplete(call) if call.id == "call_1" && call.name == "t"
        )));
    }
    let done = chunks
        .iter()
        .find_map(|chunk| match chunk {
            StreamChunk::Done {
                usage,
                finish_reason,
            } => Some((usage.total_tokens, finish_reason)),
            _ => None,
        })
        .expect("stream should end with Done");
    assert_eq!(done.0, 7);
    assert_eq!(*done.1, FinishReason::ToolUse);
}

#[tokio::test]
async fn anthropic_stream_emits_complete_tool_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/gateway/chat/completions"))
        .and(header("authorization", "Bearer anth-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(openai_style_sse()),
        )
        .mount(&server)
        .await;

    let provider = AnthropicProvider::new()
        .with_api_key("anth-key")
        .with_base_url(&server.uri())
        .with_model("kilo-auto/free");
    let request = LlmRequest::new(vec![Message::user("hi")]);
    let chunks = collect_stream(&provider, &request).await;

    assert_openai_stream_chunks(&chunks, true);
}

#[tokio::test]
async fn openai_compat_stream_emits_tool_call_deltas() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(openai_style_sse()),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatProvider::new()
        .with_name("ollama")
        .with_base_url(&format!("{}/v1", server.uri()))
        .with_model("llama3.1");
    let request = LlmRequest::new(vec![Message::user("hi")]);
    let chunks = collect_stream(&provider, &request).await;

    assert_openai_stream_chunks(&chunks, false);
}

#[tokio::test]
async fn xai_stream_emits_tool_call_deltas() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer xai-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(openai_style_sse()),
        )
        .mount(&server)
        .await;

    let provider = xai::new()
        .with_api_key("xai-key")
        .with_base_url(&format!("{}/v1", server.uri()))
        .with_model("grok-beta");
    let request = LlmRequest::new(vec![Message::user("hi")]);
    let chunks = collect_stream(&provider, &request).await;

    assert_openai_stream_chunks(&chunks, false);
}

#[tokio::test]
async fn mistral_stream_emits_tool_call_deltas() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer mistral-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(openai_style_sse()),
        )
        .mount(&server)
        .await;

    let provider = mistral::new()
        .with_api_key("mistral-key")
        .with_base_url(&format!("{}/v1", server.uri()))
        .with_model("mistral-large-latest");
    let request = LlmRequest::new(vec![Message::user("hi")]);
    let chunks = collect_stream(&provider, &request).await;

    assert_openai_stream_chunks(&chunks, false);
}

#[tokio::test]
async fn gemini_stream_emits_tool_call_chunks() {
    let server = MockServer::start().await;
    let response = serde_json::json!({
        "candidates": [{
            "content": {
                "role": "model",
                "parts": [
                    {"text": "Hello"},
                    {"functionCall": {"name": "t", "args": {"x": 1}}}
                ]
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 6,
            "candidatesTokenCount": 2,
            "totalTokenCount": 8
        }
    });
    Mock::given(method("POST"))
        .and(path(
            "/v1beta/models/gemini-1.5-flash:streamGenerateContent",
        ))
        .and(query_param("key", "gem-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(format!("data: {}\n\ndata: [DONE]\n\n", response)),
        )
        .mount(&server)
        .await;

    let provider = GeminiProvider::new()
        .with_api_key("gem-key")
        .with_base_url(&server.uri())
        .with_model("gemini-1.5-flash");
    let request = LlmRequest::new(vec![Message::user("hi")]);
    let chunks = collect_stream(&provider, &request).await;

    assert!(
        chunks
            .iter()
            .any(|chunk| matches!(chunk, StreamChunk::Text(text) if text == "Hello"))
    );
    assert!(chunks.iter().any(|chunk| matches!(
        chunk,
        StreamChunk::ToolCallStart { id, name } if id == "t-0" && name == "t"
    )));
    assert!(
        chunks.iter().any(|chunk| matches!(
            chunk,
            StreamChunk::ToolCallDelta { id, arguments_delta } if id == "t-0" && arguments_delta.contains("\"x\":1")
        ))
    );
    let done = chunks
        .iter()
        .find_map(|chunk| match chunk {
            StreamChunk::Done {
                usage,
                finish_reason,
            } => Some((usage.total_tokens, finish_reason)),
            _ => None,
        })
        .expect("stream should end with Done");
    assert_eq!(done.0, 8);
    assert_eq!(*done.1, FinishReason::ToolUse);
}
