use std::time::Duration;

use vil_llm::{
    config::{LlmConfig, ProviderConfig},
    provider::{LlmRequest, Message},
    router::LlmRouter,
};

#[tokio::test]
async fn ollama_smoke_complete_with_local_model() {
    if std::process::Command::new("ollama")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: ollama binary not available");
        return;
    }

    if tokio::time::timeout(
        Duration::from_millis(800),
        tokio::net::TcpStream::connect("127.0.0.1:11434"),
    )
    .await
    .is_err()
    {
        eprintln!("skipping: ollama server not reachable at 127.0.0.1:11434");
        return;
    }

    let mut cfg = LlmConfig::default();
    cfg.default_provider = "ollama".to_string();
    cfg.fallback_chain = vec!["ollama".to_string()];
    cfg.providers.insert(
        "ollama".to_string(),
        ProviderConfig {
            model: Some("qwen2.5-coder:0.5b".to_string()),
            max_tokens: 16,
            temperature: 0.0,
            ..ProviderConfig::default()
        },
    );

    let router = LlmRouter::from_config(&cfg);
    let request = LlmRequest::new(vec![Message::user("Say OK")]);
    let (provider, response) = router
        .complete_with_provider(&request)
        .await
        .expect("ollama completion");

    assert_eq!(provider, "ollama");
    assert!(
        response.content.to_ascii_uppercase().contains("OK"),
        "unexpected response: {}",
        response.content
    );
}

