use std::time::Duration;

use vil_llm::provider::{LlmProvider, LlmRequest, Message, StreamChunk};
use vil_llm::providers::anthropic::AnthropicProvider;
use vil_llm::providers::gemini::GeminiProvider;
use vil_llm::providers::mistral;
use vil_llm::providers::openai::OpenAiProvider;
use vil_llm::providers::openai_compat::OpenAiCompatProvider;
use vil_llm::providers::xai;

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn smoke_provider<P: LlmProvider>(label: &str, provider: P) {
    let request = LlmRequest::new(vec![Message::user("Say hi in one short sentence.")]);
    let response = tokio::time::timeout(Duration::from_secs(120), provider.complete(&request))
        .await
        .unwrap_or_else(|_| panic!("{label} smoke test timed out"))
        .unwrap_or_else(|err| panic!("{label} smoke test failed: {err}"));

    assert!(
        !response.content.trim().is_empty(),
        "{label} returned an empty completion"
    );
    assert!(
        response.usage.total_tokens > 0,
        "{label} reported zero token usage"
    );
}

async fn smoke_stream_parity<P: LlmProvider>(label: &str, provider: P) {
    let mut request = LlmRequest::new(vec![Message::user(
        "What is 5 + 7? Use the 'add' tool to find out. Please only use the tool.",
    )]);
    request.tools = vec![vil_llm::provider::Tool {
        name: "add".to_string(),
        description: "Add two numbers".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "a": { "type": "number" },
                "b": { "type": "number" }
            },
            "required": ["a", "b"]
        }),
    }];

    let mut rx = tokio::time::timeout(Duration::from_secs(30), provider.stream(&request))
        .await
        .unwrap_or_else(|_| panic!("{label} stream open timed out"))
        .unwrap_or_else(|err| panic!("{label} stream open failed: {err}"));

    let mut tool_called = false;
    let mut chunks = Vec::new();

    let timeout_future = tokio::time::timeout(Duration::from_secs(120), async {
        while let Some(chunk) = rx.recv().await {
            let is_terminal = matches!(chunk, StreamChunk::Done { .. } | StreamChunk::Error(_));
            chunks.push(chunk.clone());
            if let StreamChunk::ToolCallStart { name, .. } = chunk {
                if name == "add" {
                    tool_called = true;
                }
            }
            if is_terminal {
                break;
            }
        }
    });

    timeout_future
        .await
        .unwrap_or_else(|_| panic!("{label} stream receive timed out"));

    assert!(
        tool_called,
        "{label} stream did not emit ToolCallStart for 'add'. Chunks: {:?}",
        chunks
    );
}

#[tokio::test]
async fn smoke_anthropic() {
    let Some(api_key) = env_value("ANTHROPIC_API_KEY").or_else(|| env_value("KILO_API_KEY")) else {
        eprintln!("skipping anthropic smoke: missing ANTHROPIC_API_KEY or KILO_API_KEY");
        return;
    };

    let mut provider = AnthropicProvider::new().with_api_key(&api_key);
    if let Some(base_url) = env_value("KILO_GATEWAY_URL") {
        provider = provider.with_base_url(&base_url);
    }
    if let Some(model) = env_value("KILO_MODEL") {
        provider = provider.with_model(&model);
    }

    smoke_provider("anthropic", provider.clone()).await;
    smoke_stream_parity("anthropic_stream", provider).await;
}

#[tokio::test]
async fn smoke_openai() {
    let Some(api_key) = env_value("OPENAI_API_KEY") else {
        eprintln!("skipping openai smoke: missing OPENAI_API_KEY");
        return;
    };

    let mut provider = OpenAiProvider::new().with_api_key(&api_key);
    if let Some(base_url) = env_value("OPENAI_BASE_URL") {
        provider = provider.with_base_url(&base_url);
    }
    if let Some(model) = env_value("OPENAI_MODEL") {
        provider = provider.with_model(&model);
    }

    smoke_provider("openai", provider.clone()).await;
    smoke_stream_parity("openai_stream", provider).await;
}

#[tokio::test]
async fn smoke_gemini() {
    let Some(api_key) = env_value("GEMINI_API_KEY") else {
        eprintln!("skipping gemini smoke: missing GEMINI_API_KEY");
        return;
    };

    let mut provider = GeminiProvider::new().with_api_key(&api_key);
    if let Some(base_url) = env_value("GEMINI_BASE_URL") {
        provider = provider.with_base_url(&base_url);
    }
    if let Some(model) = env_value("GEMINI_MODEL") {
        provider = provider.with_model(&model);
    }

    smoke_provider("gemini", provider.clone()).await;
    smoke_stream_parity("gemini_stream", provider).await;
}

#[tokio::test]
async fn smoke_xai() {
    let Some(api_key) = env_value("XAI_API_KEY") else {
        eprintln!("skipping xai smoke: missing XAI_API_KEY");
        return;
    };

    let mut provider = xai::new().with_api_key(&api_key);
    if let Some(base_url) = env_value("XAI_BASE_URL") {
        provider = provider.with_base_url(&base_url);
    }
    if let Some(model) = env_value("XAI_MODEL") {
        provider = provider.with_model(&model);
    }

    smoke_provider("xai", provider.clone()).await;
    smoke_stream_parity("xai_stream", provider).await;
}

#[tokio::test]
async fn smoke_mistral() {
    let Some(api_key) = env_value("MISTRAL_API_KEY") else {
        eprintln!("skipping mistral smoke: missing MISTRAL_API_KEY");
        return;
    };

    let mut provider = mistral::new().with_api_key(&api_key);
    if let Some(base_url) = env_value("MISTRAL_BASE_URL") {
        provider = provider.with_base_url(&base_url);
    }
    if let Some(model) = env_value("MISTRAL_MODEL") {
        provider = provider.with_model(&model);
    }

    smoke_provider("mistral", provider.clone()).await;
    smoke_stream_parity("mistral_stream", provider).await;
}

#[tokio::test]
async fn smoke_openai_compat() {
    let Some(base_url) = env_value("OPENAI_COMPAT_BASE_URL") else {
        eprintln!("skipping openai_compat smoke: missing OPENAI_COMPAT_BASE_URL");
        return;
    };

    let mut provider = OpenAiCompatProvider::new().with_base_url(&base_url);
    if let Some(api_key) = env_value("OPENAI_COMPAT_API_KEY") {
        provider = provider.with_api_key(&api_key);
    }
    if let Some(model) = env_value("OPENAI_COMPAT_MODEL") {
        provider = provider.with_model(&model);
    }

    smoke_provider("openai_compat", provider.clone()).await;
    smoke_stream_parity("openai_compat_stream", provider).await;
}
