//! `.vac/config.toml` `[llm]` section parsing and runtime resolution.
//!
//! This module is the *vil_llm*-local view of the LLM configuration (independent
//! from the workspace-wide `vac_core::config::LlmConfig`). It is designed to:
//!
//! 1. Parse the `[llm]` section from `<project_root>/.vac/config.toml` with
//!    schema tolerant of missing fields (defaults fill in gaps).
//! 2. Apply environment-variable overrides (env wins over TOML).
//! 3. Never panic on malformed TOML — return an error instead.
//! 4. Expose a factory hook (`LlmRouter::from_config`) so downstream callers
//!    don't re-encode the provider-registration logic.
//!
//! Schema:
//! ```toml
//! [llm]
//! default_provider = "anthropic"
//! fallback_chain = ["anthropic", "openai"]
//! budget_tokens = 2_000_000
//!
//! [llm.routing]
//! grep = "cheap"
//! glob = "cheap"
//! vil_audit = "reasoning"
//!
//! [llm.providers.anthropic]
//! api_key_env = "ANTHROPIC_API_KEY"
//! model = "claude-opus-4"
//! base_url = "https://api.anthropic.com"
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Default token budget per run when neither TOML nor env override supplies one.
pub const DEFAULT_BUDGET_TOKENS: u64 = 2_000_000;

/// Default provider name when no `[llm].default_provider` is configured.
pub const DEFAULT_PROVIDER: &str = "anthropic";

/// Env var that overrides `[llm].default_provider`.
pub const ENV_DEFAULT_PROVIDER: &str = "VAC_LLM_DEFAULT_PROVIDER";

/// Env var that overrides `[llm].budget_tokens`.
pub const ENV_BUDGET_TOKENS: &str = "VAC_LLM_BUDGET_TOKENS";

/// Resolved LLM configuration used by `LlmRouter::from_config` and `vac doctor`.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub default_provider: String,
    pub fallback_chain: Vec<String>,
    pub budget_tokens: u64,
    /// Maximum LLM requests per minute. 0 means unlimited.
    pub requests_per_minute: u32,
    /// Tool name → provider-or-alias (e.g. `"grep" -> "cheap"`).
    pub routing: HashMap<String, String>,
    pub providers: HashMap<String, ProviderConfig>,
}

/// Per-provider config block.
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub api_key_env: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            default_provider: DEFAULT_PROVIDER.to_string(),
            fallback_chain: vec![DEFAULT_PROVIDER.to_string()],
            budget_tokens: DEFAULT_BUDGET_TOKENS,
            requests_per_minute: 0,
            routing: HashMap::new(),
            providers: HashMap::new(),
        }
    }
}

/// Wire-format TOML shapes (deserialized from `.vac/config.toml`).
#[derive(Debug, Deserialize, Default)]
struct RawRoot {
    #[serde(default)]
    llm: Option<RawLlm>,
}

#[derive(Debug, Deserialize, Default)]
struct RawLlm {
    #[serde(default)]
    default_provider: Option<String>,
    #[serde(default)]
    fallback_chain: Option<Vec<String>>,
    #[serde(default)]
    pub budget_tokens: Option<u64>,
    #[serde(default)]
    requests_per_minute: Option<u32>,
    #[serde(default)]
    routing: Option<HashMap<String, String>>,
    #[serde(default)]
    providers: Option<HashMap<String, RawProvider>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawProvider {
    #[serde(default)]
    api_key_env: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
}

impl LlmConfig {
    /// Load `<project_root>/.vac/config.toml`, apply env overrides, and return
    /// a fully-resolved `LlmConfig`. Missing file → defaults. Malformed TOML →
    /// `Err(...)`.
    pub fn load(project_root: &Path) -> Result<Self, LlmConfigError> {
        let path = project_root.join(".vac").join("config.toml");
        let raw = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| LlmConfigError::Io(path.display().to_string(), e.to_string()))?;
            toml::from_str::<RawRoot>(&text)
                .map_err(|e| LlmConfigError::Parse(path.display().to_string(), e.to_string()))?
        } else {
            RawRoot::default()
        };

        let mut cfg = merge_raw_with_defaults(raw.llm.unwrap_or_default());
        apply_env_overrides(&mut cfg);
        Ok(cfg)
    }

    /// Parse config from raw TOML text (used by tests and callers that already
    /// have the bytes in memory).
    pub fn from_toml_str(text: &str) -> Result<Self, LlmConfigError> {
        let raw: RawRoot = toml::from_str(text)
            .map_err(|e| LlmConfigError::Parse("<memory>".to_string(), e.to_string()))?;
        let mut cfg = merge_raw_with_defaults(raw.llm.unwrap_or_default());
        apply_env_overrides(&mut cfg);
        Ok(cfg)
    }

    /// Return the resolved API-key env value for a named provider, if any.
    /// `None` means "no env var configured" (or provider not present).
    pub fn api_key_env_name(&self, provider: &str) -> Option<&str> {
        self.providers.get(provider)?.api_key_env.as_deref()
    }

    /// Returns `true` when the provider's configured API-key env var is set in
    /// the process environment. Providers without `api_key_env` are treated as
    /// "always ready" (e.g. local providers needing no key).
    pub fn provider_ready(&self, provider: &str) -> bool {
        match self.api_key_env_name(provider) {
            Some(var) => std::env::var(var).is_ok(),
            None => self.providers.contains_key(provider),
        }
    }
}

fn merge_raw_with_defaults(raw: RawLlm) -> LlmConfig {
    let defaults = LlmConfig::default();
    let default_provider = raw.default_provider.unwrap_or(defaults.default_provider);
    let fallback_chain = raw.fallback_chain.unwrap_or(defaults.fallback_chain);
    let budget_tokens = raw.budget_tokens.unwrap_or(defaults.budget_tokens);
    let requests_per_minute = raw
        .requests_per_minute
        .unwrap_or(defaults.requests_per_minute);
    let routing = raw.routing.unwrap_or_default();
    let providers = raw
        .providers
        .unwrap_or_default()
        .into_iter()
        .map(|(name, rp)| {
            (
                name,
                ProviderConfig {
                    api_key_env: rp.api_key_env,
                    model: rp.model,
                    base_url: rp.base_url,
                },
            )
        })
        .collect();

    LlmConfig {
        default_provider,
        fallback_chain,
        budget_tokens,
        requests_per_minute,
        routing,
        providers,
    }
}

fn apply_env_overrides(cfg: &mut LlmConfig) {
    if let Ok(v) = std::env::var(ENV_DEFAULT_PROVIDER) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            cfg.default_provider = trimmed.to_string();
        }
    }
    if let Ok(v) = std::env::var(ENV_BUDGET_TOKENS)
        && let Ok(n) = v.trim().parse::<u64>()
    {
        cfg.budget_tokens = n;
    }
}

/// Errors produced while loading / parsing an LlmConfig.
#[derive(Debug, thiserror::Error)]
pub enum LlmConfigError {
    #[error("failed to read {0}: {1}")]
    Io(String, String),
    #[error("failed to parse {0}: {1}")]
    Parse(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env-dependent tests so a parallel test can't flip vars mid-run.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_env() {
        // SAFETY: std::env::remove_var is unsafe on edition 2024 — we hold the
        // ENV_LOCK guard, so no other thread in this process mutates env here.
        unsafe {
            std::env::remove_var(ENV_DEFAULT_PROVIDER);
            std::env::remove_var(ENV_BUDGET_TOKENS);
        }
    }

    #[test]
    fn parses_full_valid_config() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();

        let toml = r#"
[llm]
default_provider = "anthropic"
fallback_chain = ["anthropic", "openai"]
budget_tokens = 3000000

[llm.routing]
grep = "cheap"
glob = "cheap"
vil_audit = "reasoning"

[llm.providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-opus-4"
base_url = "https://api.anthropic.com"

[llm.providers.openai]
api_key_env = "OPENAI_API_KEY"
model = "gpt-4o"
"#;
        let cfg = LlmConfig::from_toml_str(toml).expect("parse ok");
        assert_eq!(cfg.default_provider, "anthropic");
        assert_eq!(cfg.fallback_chain, vec!["anthropic", "openai"]);
        assert_eq!(cfg.budget_tokens, 3_000_000);
        assert_eq!(cfg.routing.get("grep").map(String::as_str), Some("cheap"));
        assert_eq!(
            cfg.routing.get("vil_audit").map(String::as_str),
            Some("reasoning")
        );
        let anthropic = cfg.providers.get("anthropic").expect("anthropic present");
        assert_eq!(anthropic.api_key_env.as_deref(), Some("ANTHROPIC_API_KEY"));
        assert_eq!(anthropic.model.as_deref(), Some("claude-opus-4"));
        assert_eq!(
            anthropic.base_url.as_deref(),
            Some("https://api.anthropic.com")
        );
        let openai = cfg.providers.get("openai").expect("openai present");
        assert_eq!(openai.api_key_env.as_deref(), Some("OPENAI_API_KEY"));
        assert!(openai.base_url.is_none());
    }

    #[test]
    fn env_default_provider_wins_over_toml() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();
        // SAFETY: ENV_LOCK held; we mutate env only for this test.
        unsafe {
            std::env::set_var(ENV_DEFAULT_PROVIDER, "openai");
        }

        let toml = r#"
[llm]
default_provider = "anthropic"
"#;
        let cfg = LlmConfig::from_toml_str(toml).expect("parse ok");
        assert_eq!(cfg.default_provider, "openai");
        clear_env();
    }

    #[test]
    fn env_budget_tokens_wins_over_toml() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();
        // SAFETY: ENV_LOCK held.
        unsafe {
            std::env::set_var(ENV_BUDGET_TOKENS, "500000");
        }

        let toml = r#"
[llm]
budget_tokens = 9000000
"#;
        let cfg = LlmConfig::from_toml_str(toml).expect("parse ok");
        assert_eq!(cfg.budget_tokens, 500_000);
        clear_env();
    }

    #[test]
    fn env_budget_tokens_non_numeric_falls_through() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();
        // SAFETY: ENV_LOCK held.
        unsafe {
            std::env::set_var(ENV_BUDGET_TOKENS, "not-a-number");
        }

        let toml = r#"
[llm]
budget_tokens = 12345
"#;
        let cfg = LlmConfig::from_toml_str(toml).expect("parse ok");
        assert_eq!(cfg.budget_tokens, 12_345);
        clear_env();
    }

    #[test]
    fn missing_config_returns_defaults() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();

        let tmp = tempfile::tempdir().expect("tempdir");
        // Do NOT create .vac/config.toml — path is absent.
        let cfg = LlmConfig::load(tmp.path()).expect("load ok");
        assert_eq!(cfg.default_provider, DEFAULT_PROVIDER);
        assert_eq!(cfg.budget_tokens, DEFAULT_BUDGET_TOKENS);
        assert_eq!(cfg.fallback_chain, vec![DEFAULT_PROVIDER.to_string()]);
        assert!(cfg.routing.is_empty());
        assert!(cfg.providers.is_empty());
    }

    #[test]
    fn malformed_toml_returns_error_not_panic() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();

        let err = LlmConfig::from_toml_str("[llm\ninvalid = = =").unwrap_err();
        // Guarantee it's a Parse error, not a panic / Io error.
        match err {
            LlmConfigError::Parse(_, _) => {}
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn empty_llm_section_uses_defaults() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();

        let cfg = LlmConfig::from_toml_str("[llm]\n").expect("parse ok");
        assert_eq!(cfg.default_provider, DEFAULT_PROVIDER);
        assert_eq!(cfg.fallback_chain, vec![DEFAULT_PROVIDER.to_string()]);
        assert_eq!(cfg.budget_tokens, DEFAULT_BUDGET_TOKENS);
    }

    #[test]
    fn provider_ready_flags_missing_env() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();
        // SAFETY: ENV_LOCK held.
        unsafe {
            std::env::remove_var("TEST_FAKE_KEY_ABC");
            std::env::set_var("TEST_FAKE_KEY_SET", "value");
        }

        let toml = r#"
[llm]
default_provider = "anthropic"

[llm.providers.anthropic]
api_key_env = "TEST_FAKE_KEY_SET"

[llm.providers.openai]
api_key_env = "TEST_FAKE_KEY_ABC"
"#;
        let cfg = LlmConfig::from_toml_str(toml).expect("parse ok");
        assert!(cfg.provider_ready("anthropic"));
        assert!(!cfg.provider_ready("openai"));
        assert!(!cfg.provider_ready("mistral")); // not present at all

        // SAFETY: ENV_LOCK held.
        unsafe {
            std::env::remove_var("TEST_FAKE_KEY_SET");
        }
    }
}
