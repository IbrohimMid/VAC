#![allow(clippy::unwrap_used, clippy::expect_used)]

use vac_core::{VacEngine, config::VacConfig};

#[tokio::test]
async fn test_dynamic_reload_without_restart() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    let vac_dir = project_root.join(".vac");
    std::fs::create_dir_all(&vac_dir).unwrap();

    let config_path = vac_dir.join("config.toml");

    let mut initial_config = VacConfig::default();
    initial_config.llm.default_provider = "anthropic".to_string();
    initial_config.vil_lsp.enable = false;
    initial_config.trace.enable = false;
    std::fs::write(&config_path, toml::to_string(&initial_config).unwrap()).unwrap();

    let mut engine = VacEngine::new(project_root.clone()).await.unwrap();
    engine.init().await.unwrap();

    // Validate initial state
    let mut available = engine.available_models();
    assert!(available.iter().any(|(p, _)| p == "anthropic"));

    // Now update config
    let mut updated_config = VacConfig::default();
    updated_config.llm.default_provider = "openai".to_string();
    updated_config.llm.providers.retain(|k, _| k == "openai"); // Remove anthropic
    std::fs::write(&config_path, toml::to_string(&updated_config).unwrap()).unwrap();

    // Reload
    engine.reload_config().await.unwrap();

    // Validate updated state
    available = engine.available_models();
    assert!(!available.iter().any(|(p, _)| p == "anthropic"));
    assert!(available.iter().any(|(p, _)| p == "openai"));
}
