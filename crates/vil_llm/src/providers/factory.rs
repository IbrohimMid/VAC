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
        "anthropic" | "kilo" | "kilo_gateway" => build_anthropic_provider(cfg),
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
