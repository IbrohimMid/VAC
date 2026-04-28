use crate::config::ProviderConfig;
use crate::provider::LlmProvider;
use std::sync::Arc;
use tracing::warn;

use super::anthropic::AnthropicProvider;
use super::gemini::GeminiProvider;
use super::mistral;
use super::openai::OpenAiProvider;
use super::openai_compat::OpenAiCompatProvider;
use super::xai;

fn trim_to_none(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.trim().is_empty())
}

fn resolve_env_value(env_name: &str) -> Option<String> {
    std::env::var(env_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_api_key(preferred_env: Option<&str>, fallback_envs: &[&str]) -> Option<String> {
    preferred_env
        .and_then(resolve_env_value)
        .or_else(|| fallback_envs.iter().find_map(|env| resolve_env_value(env)))
}

fn provider_missing(key: &str, reason: &str) {
    warn!(
        provider = key,
        reason = reason,
        "skipping llm provider registration"
    );
}

fn build_anthropic_provider(cfg: &ProviderConfig) -> Option<Arc<dyn LlmProvider>> {
    let api_key = resolve_api_key(
        cfg.api_key_env.as_deref(),
        &["KILO_API_KEY", "ANTHROPIC_API_KEY"],
    )?;
    let mut provider = AnthropicProvider::new().with_api_key(&api_key);

    if let Some(base_url) = trim_to_none(cfg.base_url.as_deref()) {
        provider = provider.with_base_url(base_url);
    }
    if let Some(model) = trim_to_none(cfg.model.as_deref()) {
        provider = provider.with_model(model);
    }

    Some(Arc::new(provider))
}

fn build_openai_provider(cfg: &ProviderConfig) -> Option<Arc<dyn LlmProvider>> {
    let api_key = resolve_api_key(cfg.api_key_env.as_deref(), &["OPENAI_API_KEY"])?;
    let mut provider = OpenAiProvider::new().with_api_key(&api_key);

    if let Some(base_url) = trim_to_none(cfg.base_url.as_deref()) {
        provider = provider.with_base_url(base_url);
    }
    if let Some(model) = trim_to_none(cfg.model.as_deref()) {
        provider = provider.with_model(model);
    }

    Some(Arc::new(provider))
}

fn build_gemini_provider(cfg: &ProviderConfig) -> Option<Arc<dyn LlmProvider>> {
    let api_key = resolve_api_key(cfg.api_key_env.as_deref(), &["GEMINI_API_KEY"])?;
    let mut provider = GeminiProvider::new().with_api_key(&api_key);

    if let Some(base_url) = trim_to_none(cfg.base_url.as_deref()) {
        provider = provider.with_base_url(base_url);
    }
    if let Some(model) = trim_to_none(cfg.model.as_deref()) {
        provider = provider.with_model(model);
    }

    Some(Arc::new(provider))
}

fn build_openai_compat_provider(
    provider_key: &str,
    cfg: &ProviderConfig,
) -> Option<Arc<dyn LlmProvider>> {
    let has_base_url = trim_to_none(cfg.base_url.as_deref()).is_some()
        || resolve_env_value("OPENAI_COMPAT_BASE_URL").is_some();
    if !has_base_url {
        provider_missing(
            provider_key,
            "missing OPENAI_COMPAT_BASE_URL or config base_url",
        );
        return None;
    }

    let mut provider = OpenAiCompatProvider::new().with_name(provider_key);
    if let Some(base_url) = trim_to_none(cfg.base_url.as_deref()) {
        provider = provider.with_base_url(base_url);
    }
    if let Some(model) = trim_to_none(cfg.model.as_deref()) {
        provider = provider.with_model(model);
    }
    if let Some(api_key) = resolve_api_key(cfg.api_key_env.as_deref(), &["OPENAI_COMPAT_API_KEY"]) {
        provider = provider.with_api_key(&api_key);
    } else if cfg.api_key_env.is_some() {
        provider_missing(provider_key, "missing configured API key");
        return None;
    }

    Some(Arc::new(provider))
}

/// Build an OpenAI-compatible provider from a named preset (kilo, groq,
/// openrouter, deepseek, together). Config values for `base_url`, `model`,
/// and `api_key_env` still take precedence over preset defaults when present.
///
/// Unlike [`build_openai_compat_provider`], this path does not require
/// `OPENAI_COMPAT_BASE_URL` — the preset supplies a canonical base URL.
fn build_preset_openai_compat(
    provider_key: &str,
    cfg: &ProviderConfig,
) -> Option<Arc<dyn LlmProvider>> {
    let (mut provider, default_env) = match provider_key {
        "kilo" | "kilo_gateway" => (OpenAiCompatProvider::new_kilo(), "KILO_API_KEY"),
        "groq" => (OpenAiCompatProvider::new_groq(), "GROQ_API_KEY"),
        "openrouter" => (OpenAiCompatProvider::new_openrouter(), "OPENROUTER_API_KEY"),
        "deepseek" => (OpenAiCompatProvider::new_deepseek(), "DEEPSEEK_API_KEY"),
        "together" => (OpenAiCompatProvider::new_together(), "TOGETHER_API_KEY"),
        _ => return None,
    };

    // Config overrides win over preset defaults.
    if let Some(base_url) = trim_to_none(cfg.base_url.as_deref()) {
        provider = provider.with_base_url(base_url);
    }
    if let Some(model) = trim_to_none(cfg.model.as_deref()) {
        provider = provider.with_model(model);
    }

    // API key resolution: preferred env (from config) -> vendor default env.
    let api_key = resolve_api_key(cfg.api_key_env.as_deref(), &[default_env]);
    if let Some(key) = api_key {
        provider = provider.with_api_key(&key);
    } else {
        provider_missing(
            provider_key,
            &format!(
                "missing {} (or configured api_key_env)",
                cfg.api_key_env.as_deref().unwrap_or(default_env)
            ),
        );
        return None;
    }

    Some(Arc::new(provider))
}

fn build_ollama_provider(cfg: &ProviderConfig) -> Option<Arc<dyn LlmProvider>> {
    let mut provider = OpenAiCompatProvider::new_ollama();

    if let Some(base_url) = trim_to_none(cfg.base_url.as_deref()) {
        provider = provider.with_base_url(base_url);
    }
    if let Some(model) = trim_to_none(cfg.model.as_deref()) {
        provider = provider.with_model(model);
    }

    if let Some(api_key) = resolve_api_key(
        cfg.api_key_env.as_deref(),
        &["OLLAMA_API_KEY", "OPENAI_COMPAT_API_KEY"],
    ) {
        provider = provider.with_api_key(&api_key);
    } else if cfg.api_key_env.is_some() {
        provider_missing("ollama", "missing configured API key");
        return None;
    }

    Some(Arc::new(provider))
}

fn build_xai_provider(cfg: &ProviderConfig) -> Option<Arc<dyn LlmProvider>> {
    let api_key = resolve_api_key(cfg.api_key_env.as_deref(), &["XAI_API_KEY"])?;
    let mut provider = xai::new().with_api_key(&api_key);

    if let Some(base_url) = trim_to_none(cfg.base_url.as_deref()) {
        provider = provider.with_base_url(base_url);
    }
    if let Some(model) = trim_to_none(cfg.model.as_deref()) {
        provider = provider.with_model(model);
    }

    Some(Arc::new(provider))
}

fn build_mistral_provider(cfg: &ProviderConfig) -> Option<Arc<dyn LlmProvider>> {
    let api_key = resolve_api_key(cfg.api_key_env.as_deref(), &["MISTRAL_API_KEY"])?;
    let mut provider = mistral::new().with_api_key(&api_key);

    if let Some(base_url) = trim_to_none(cfg.base_url.as_deref()) {
        provider = provider.with_base_url(base_url);
    }
    if let Some(model) = trim_to_none(cfg.model.as_deref()) {
        provider = provider.with_model(model);
    }

    Some(Arc::new(provider))
}

pub(crate) fn build_provider_from_config(
    provider_key: &str,
    cfg: &ProviderConfig,
) -> Option<Arc<dyn LlmProvider>> {
    match provider_key {
        // Legacy path: the struct is named `AnthropicProvider` but actually
        // targets Kilo Gateway via the OpenAI-compatible endpoint. Kept for
        // backward compat until `anthropic.rs` is renamed. See
        // `docs/PROVIDER_PARITY.md` for the rename plan.
        "anthropic" => build_anthropic_provider(cfg),
        // New path: route `kilo` / `kilo_gateway` through the OpenAI-compat
        // preset so the wire format matches Kilo's documented API and the
        // misnomer does not spread to new call sites.
        "kilo" | "kilo_gateway" | "groq" | "openrouter" | "deepseek" | "together" => {
            build_preset_openai_compat(provider_key, cfg)
        }
        "ollama" => build_ollama_provider(cfg),
        "openai" => build_openai_provider(cfg),
        "gemini" => build_gemini_provider(cfg),
        "openai_compat" => build_openai_compat_provider(provider_key, cfg),
        "xai" => build_xai_provider(cfg),
        "mistral" => build_mistral_provider(cfg),
        other => {
            provider_missing(other, "no provider factory is registered for this key");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_builds_ollama_without_explicit_base_url() {
        let cfg = ProviderConfig::default();
        let Some(provider) = build_provider_from_config("ollama", &cfg) else {
            panic!("provider")
        };
        assert_eq!(provider.name(), "ollama");
    }
}
