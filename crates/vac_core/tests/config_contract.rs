//! Tests for config contract: vac init template must parse to VacConfig.

use vac_core::VacConfig;

/// The default config template written by `vac init`.
/// This test ensures the template always parses to a valid VacConfig.
const INIT_CONFIG_TEMPLATE: &str = r#"
[knowledge]
# root = "/path/to/llm_knowledge"

[llm]
default_provider = "anthropic"

[llm.providers.anthropic]
api_key_env = "KILO_API_KEY"
model = "kilo/free"
max_tokens = 4000

[tools]
default_policy = "medium"

[tools.allow]
file_read = true
file_write = true
file_edit = true
glob = true
grep = true
search = true
cargo = true
git = true
bash = true
vil_status = true
vil_knowledge = true
task_done = true
todo_write = true

[memory]
persist_path = ".vac/memory"
enable_episodic = true
enable_semantic = true

[context]
enable_shm = true
shm_pool_size_mb = 512

[swarm]
max_concurrent_agents = 4
enable_parallel = true

[trace]
enable = true
output_path = ".vac/traces"
enable_signing = false

[vil_lsp]
enable = true
binary_path = "vil-lsp"
startup_timeout_ms = 3000
max_prompt_items = 8
fail_on_unavailable = false
analyze_on_init = true
analyze_after_edit = true
"#;

#[test]
fn init_config_template_parses_to_vac_config() {
    let result = toml::from_str::<VacConfig>(INIT_CONFIG_TEMPLATE);
    assert!(result.is_ok(), "init config template must parse: {:?}", result.err());
}

#[test]
fn default_vac_config_is_valid() {
    let config = VacConfig::default();
    // Serialize and re-parse to ensure round-trip
    let serialized = toml::to_string(&config).expect("VacConfig must serialize");
    let reparsed = toml::from_str::<VacConfig>(&serialized);
    assert!(reparsed.is_ok(), "VacConfig round-trip must succeed: {:?}", reparsed.err());
}

#[test]
fn vil_lsp_config_defaults_are_sane() {
    let config = VacConfig::default();
    assert!(config.vil_lsp.enable);
    assert!(!config.vil_lsp.fail_on_unavailable);
    assert!(config.vil_lsp.analyze_on_init);
    assert!(config.vil_lsp.analyze_after_edit);
    assert!(config.vil_lsp.max_prompt_items > 0);
}

#[test]
fn rulebook_config_defaults_are_sane() {
    let config = VacConfig::default();
    assert!(config.rulebook.enable);
    assert!(!config.rulebook.fail_on_invalid);
}

#[test]
fn runtime_config_defaults_are_sane() {
    let config = VacConfig::default();
    assert!(!config.runtime.enable, "runtime should be disabled by default");
    assert_eq!(config.runtime.operating_mode, "monitor-only");
}
