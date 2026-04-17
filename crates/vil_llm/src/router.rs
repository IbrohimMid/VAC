//! LLM Router — manages multiple providers with fallback chain.

use crate::config::LlmConfig;
use crate::error::{LlmError, LlmResult};
use crate::provider::{LlmProvider, LlmRequest, LlmResponse, StreamChunk};
use crate::rulebook_hook::RulebookContext;
use crate::sanitize;
use crate::token_budget::TokenBudget;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn};

use crate::providers::anthropic::AnthropicProvider;
use crate::providers::gemini::GeminiProvider;
use crate::providers::openai::OpenAiProvider;
use crate::providers::openai_compat::OpenAiCompatProvider;
use crate::providers::{mistral, xai};

fn is_retryable(e: &LlmError) -> bool {
    match e {
        LlmError::RateLimited(_, _) => true,
        LlmError::Provider { message, .. } => {
            message.contains("429") || message.contains("503") || message.contains("502")
        }
        LlmError::Request(re) => re
            .status()
            .is_some_and(|s| s.as_u16() == 429 || s.as_u16() == 503 || s.as_u16() == 502),
        _ => false,
    }
}

pub struct LlmRouter {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    default_provider: String,
    fallback_chain: Vec<String>,
    /// Per-tool provider overrides: tool name -> provider name.
    /// Consulted after the rulebook, before falling back to `default_provider`.
    tool_routing_map: HashMap<String, String>,
    /// Rulebook hook consulted before `tool_routing_map`. Defaults to a stub
    /// that always returns `None` (no override).
    rulebook: RulebookContext,
    budget: Arc<RwLock<TokenBudget>>,
    retry_config: crate::retry::RetryConfig,
}

impl LlmRouter {
    pub fn new(default_provider: &str, budget_limit: u64) -> Self {
        Self {
            providers: HashMap::new(),
            default_provider: default_provider.to_string(),
            fallback_chain: vec![],
            tool_routing_map: HashMap::new(),
            rulebook: RulebookContext::empty(),
            budget: Arc::new(RwLock::new(TokenBudget::new(budget_limit))),
            retry_config: crate::retry::RetryConfig::default(),
        }
    }

    pub fn with_retry_config(mut self, config: crate::retry::RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Configure the per-tool provider routing map. Overwrites any existing map.
    pub fn set_tool_routing(&mut self, map: HashMap<String, String>) {
        self.tool_routing_map = map;
    }

    /// Install a rulebook context (e.g. wired from the future rulebook unit).
    pub fn set_rulebook(&mut self, rulebook: RulebookContext) {
        self.rulebook = rulebook;
    }

    /// Resolve the provider name for a given tool. Priority order:
    /// 1. Rulebook preference (if any)
    /// 2. Static `tool_routing_map` entry
    /// 3. `default_provider`
    ///
    /// This only *names* the provider; it does not verify the provider is
    /// registered. Callers that need a registered provider should check
    /// `self.providers.contains_key(&name)` and fall back accordingly.
    pub fn route_for_tool(&self, tool_name: &str) -> String {
        if let Some(model) = self.rulebook.preferred_model(tool_name) {
            return model;
        }
        if let Some(provider) = self.tool_routing_map.get(tool_name) {
            return provider.clone();
        }
        self.default_provider.clone()
    }

    pub fn add_provider(&mut self, provider: Arc<dyn LlmProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    pub fn with_anthropic(&mut self) -> &mut Self {
        self.add_provider(Arc::new(AnthropicProvider::new()));
        self
    }

    pub fn with_kilo_gateway(&mut self) -> &mut Self {
        self.add_provider(Arc::new(AnthropicProvider::new()));
        self
    }

    pub fn with_openai(&mut self) -> &mut Self {
        self.add_provider(Arc::new(OpenAiProvider::new()));
        self
    }

    pub fn with_gemini(&mut self) -> &mut Self {
        self.add_provider(Arc::new(GeminiProvider::new()));
        self
    }

    pub fn with_xai(&mut self) -> &mut Self {
        self.add_provider(Arc::new(xai::new()));
        self
    }

    pub fn with_mistral(&mut self) -> &mut Self {
        self.add_provider(Arc::new(mistral::new()));
        self
    }

    pub fn with_openai_compat(&mut self) -> &mut Self {
        self.add_provider(Arc::new(OpenAiCompatProvider::new()));
        self
    }

    pub fn set_fallback_chain(&mut self, chain: Vec<String>) {
        self.fallback_chain = chain;
    }

    /// Build a router from a resolved `LlmConfig`. Known provider names are
    /// registered via their respective builders; unknown names are skipped with
    /// a warning (forward-compatible: newly defined providers in a future
    /// config do not fail the loader).
    pub fn from_config(cfg: &LlmConfig) -> Self {
        let mut router = Self::new(&cfg.default_provider, cfg.budget_tokens);
        router.set_fallback_chain(cfg.fallback_chain.clone());

        for name in cfg.providers.keys() {
            match name.as_str() {
                "anthropic" | "kilo" | "kilo_gateway" => {
                    router.add_provider(Arc::new(AnthropicProvider::new()));
                }
                other => {
                    warn!(
                        provider = other,
                        "provider listed in [llm.providers] but no builder registered yet — \
                         skipping (pending Unit 1/2 merges)"
                    );
                }
            }
        }

        router
    }

    pub async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse> {
        {
            let budget = self.budget.read().await;
            if budget.is_exceeded() {
                return Err(LlmError::BudgetExceeded {
                    used: budget.used(),
                    limit: budget.limit(),
                });
            }
        }

        let mut chain = vec![self.default_provider.clone()];
        chain.extend(self.fallback_chain.iter().cloned());

        let mut last_error = None;

        for provider_name in &chain {
            if let Some(provider) = self.providers.get(provider_name) {
                // Sanitize messages per-provider (each provider may have different contract rules)
                let sanitized_messages =
                    sanitize::sanitize_messages(&request.messages, provider_name);
                let sanitized_request = LlmRequest {
                    messages: sanitized_messages,
                    model: request.model.clone(),
                    max_tokens: request.max_tokens,
                    temperature: request.temperature,
                    stop_sequences: request.stop_sequences.clone(),
                    tools: request.tools.clone(),
                };

                let mut attempt = 0usize;
                loop {
                    attempt += 1;
                    match provider.complete(&sanitized_request).await {
                        Ok(response) => {
                            let mut budget = self.budget.write().await;
                            budget.add_usage(response.usage.total_tokens);
                            info!(
                                provider = provider_name,
                                tokens = response.usage.total_tokens,
                                "LLM request completed"
                            );
                            return Ok(response);
                        }
                        Err(e) if is_retryable(&e) && attempt < self.retry_config.max_attempts => {
                            let delay = if let LlmError::RateLimited(_, retry_after_secs) = &e {
                                // Use retry-after from error
                                crate::retry::RetryDelay {
                                    delay_ms: retry_after_secs * 1000,
                                    source: crate::retry::RetryDelaySource::RetryAfterHeader,
                                }
                            } else {
                                // Fallback to exponential backoff
                                crate::retry::resolve_retry_delay_ms(
                                    &std::collections::HashMap::new(),
                                    &self.retry_config,
                                    attempt,
                                    chrono::Utc::now(),
                                )
                            };
                            warn!(provider = provider_name, attempt, delay_ms = delay.delay_ms, error = %e, "Retrying after delay");
                            tokio::time::sleep(std::time::Duration::from_millis(delay.delay_ms))
                                .await;
                        }
                        Err(e) => {
                            warn!(provider = provider_name, error = %e, "Provider failed, trying next");
                            last_error = Some(e);
                            break;
                        }
                    }
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::AllProvidersFailed))
    }

    pub async fn stream(&self, request: &LlmRequest) -> LlmResult<mpsc::Receiver<StreamChunk>> {
        let provider =
            self.providers
                .get(&self.default_provider)
                .ok_or_else(|| LlmError::Provider {
                    provider: self.default_provider.clone(),
                    message: "Default provider not found".into(),
                })?;

        // Sanitize messages for this specific provider
        let sanitized_messages =
            sanitize::sanitize_messages(&request.messages, &self.default_provider);
        let sanitized_request = LlmRequest {
            messages: sanitized_messages,
            model: request.model.clone(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stop_sequences: request.stop_sequences.clone(),
            tools: request.tools.clone(),
        };

        provider.stream(&sanitized_request).await
    }

    pub async fn token_usage(&self) -> (u64, u64) {
        let budget = self.budget.read().await;
        (budget.used(), budget.limit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> LlmRouter {
        LlmRouter::new("default_prov", 1_000)
    }

    #[test]
    fn route_for_tool_falls_back_to_default_when_no_map_no_rulebook() {
        let r = router();
        assert_eq!(r.route_for_tool("Grep"), "default_prov");
    }

    #[test]
    fn route_for_tool_uses_map_when_set() {
        let mut r = router();
        let mut map = HashMap::new();
        map.insert("Grep".to_string(), "cheap_prov".to_string());
        map.insert("Plan".to_string(), "reasoning_prov".to_string());
        r.set_tool_routing(map);

        assert_eq!(r.route_for_tool("Grep"), "cheap_prov");
        assert_eq!(r.route_for_tool("Plan"), "reasoning_prov");
        // Miss → default
        assert_eq!(r.route_for_tool("Unknown"), "default_prov");
    }

    #[test]
    fn route_for_tool_rulebook_wins_over_map() {
        let mut r = router();
        let mut map = HashMap::new();
        map.insert("Grep".to_string(), "cheap_prov".to_string());
        r.set_tool_routing(map);

        // Rulebook says Grep → rulebook_prov; should win over map entry.
        r.set_rulebook(RulebookContext::new(|tool| {
            if tool == "Grep" {
                Some("rulebook_prov".to_string())
            } else {
                None
            }
        }));

        assert_eq!(r.route_for_tool("Grep"), "rulebook_prov");
    }

    #[test]
    fn route_for_tool_rulebook_miss_falls_through_to_map() {
        let mut r = router();
        let mut map = HashMap::new();
        map.insert("Plan".to_string(), "reasoning_prov".to_string());
        r.set_tool_routing(map);
        r.set_rulebook(RulebookContext::new(|_tool| None));

        assert_eq!(r.route_for_tool("Plan"), "reasoning_prov");
        assert_eq!(r.route_for_tool("Other"), "default_prov");
    }

    #[test]
    fn route_for_tool_rulebook_miss_and_map_miss_falls_through_to_default() {
        let mut r = router();
        r.set_rulebook(RulebookContext::new(|_tool| None));
        assert_eq!(r.route_for_tool("Anything"), "default_prov");
    }
}
