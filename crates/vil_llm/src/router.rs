//! LLM Router — manages multiple providers with fallback chain.

use crate::config::LlmConfig;
use crate::error::{LlmError, LlmResult};
use crate::provider::{LlmProvider, LlmRequest, LlmResponse, StreamChunk};
use crate::providers::build_provider_from_config;
use crate::rulebook_hook::RulebookContext;
use crate::sanitize;
use crate::token_budget::TokenBudget;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn};

fn is_retryable(e: &LlmError) -> bool {
    match e {
        LlmError::RateLimited(_, _) => true,
        LlmError::Provider {
            status: Some(s), ..
        } => matches!(s, 429 | 502 | 503 | 504),
        LlmError::Provider { status: None, .. } => false,
        LlmError::Request(re) => re
            .status()
            .is_some_and(|s| matches!(s.as_u16(), 429 | 502 | 503 | 504)),
        _ => false,
    }
}

pub struct SimpleRateLimiter {
    pub requests_per_minute: u32,
    pub request_timestamps: std::collections::VecDeque<tokio::time::Instant>,
}

impl SimpleRateLimiter {
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            requests_per_minute,
            request_timestamps: std::collections::VecDeque::new(),
        }
    }

    pub async fn acquire(limiter: &Arc<tokio::sync::Mutex<Self>>) {
        loop {
            let wait_time = {
                let mut guard = limiter.lock().await;
                if guard.requests_per_minute == 0 {
                    return;
                }
                let now = tokio::time::Instant::now();
                let one_min_ago = now - std::time::Duration::from_secs(60);
                while let Some(&ts) = guard.request_timestamps.front() {
                    if ts < one_min_ago {
                        guard.request_timestamps.pop_front();
                    } else {
                        break;
                    }
                }
                if guard.request_timestamps.len() < guard.requests_per_minute as usize {
                    guard.request_timestamps.push_back(now);
                    None
                } else if let Some(&oldest) = guard.request_timestamps.front() {
                    Some(oldest + std::time::Duration::from_secs(60) - now)
                } else {
                    // Invariant: deque should not be empty when len >= rpm.
                    // Defensive recovery: treat as non-saturated.
                    debug_assert!(
                        false,
                        "request_timestamps empty but len >= requests_per_minute"
                    );
                    guard.request_timestamps.push_back(now);
                    None
                }
            };
            if let Some(w) = wait_time {
                tokio::time::sleep(w).await;
            } else {
                return;
            }
        }
    }
}

pub struct LlmRouter {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    provider_defaults: HashMap<String, ProviderDefaults>,
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
    rate_limiter: Arc<tokio::sync::Mutex<SimpleRateLimiter>>,
}

#[derive(Debug, Clone, Copy)]
struct ProviderDefaults {
    max_tokens: u32,
    temperature: f32,
}

impl LlmRouter {
    pub fn new(default_provider: &str, budget_limit: u64) -> Self {
        Self {
            providers: HashMap::new(),
            provider_defaults: HashMap::new(),
            default_provider: default_provider.to_string(),
            fallback_chain: vec![],
            tool_routing_map: HashMap::new(),
            rulebook: RulebookContext::empty(),
            budget: Arc::new(RwLock::new(TokenBudget::new(budget_limit))),
            retry_config: crate::retry::RetryConfig::default(),
            rate_limiter: Arc::new(tokio::sync::Mutex::new(SimpleRateLimiter::new(0))),
        }
    }

    pub fn add_provider_named(&mut self, name: impl Into<String>, provider: Arc<dyn LlmProvider>) {
        self.providers.insert(name.into(), provider);
    }

    pub fn with_rate_limit(mut self, requests_per_minute: u32) -> Self {
        match Arc::get_mut(&mut self.rate_limiter) {
            Some(inner) => inner.get_mut().requests_per_minute = requests_per_minute,
            None => {
                // Arc has been cloned; create a fresh rate limiter instead of panicking.
                self.rate_limiter = Arc::new(tokio::sync::Mutex::new(SimpleRateLimiter::new(
                    requests_per_minute,
                )));
            }
        }
        self
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
        self.add_provider_named(provider.name().to_string(), provider);
    }

    pub fn with_anthropic(&mut self) -> &mut Self {
        self.add_provider_named(
            "anthropic",
            Arc::new(crate::providers::anthropic::AnthropicProvider::new()),
        );
        self
    }

    /// Register the Kilo Gateway via the OpenAI-compatible preset (recommended).
    /// Uses `https://api.kilo.ai/api/gateway` and expects `KILO_API_KEY`.
    pub fn with_kilo(&mut self) -> &mut Self {
        self.add_provider_named(
            "kilo",
            Arc::new(crate::providers::openai_compat::OpenAiCompatProvider::new_kilo()),
        );
        self
    }

    /// Legacy Kilo Gateway registration that uses the (misnamed) Anthropic
    /// provider struct. Prefer [`with_kilo`] for new code — this helper is
    /// kept for compatibility with existing callers.
    pub fn with_kilo_gateway(&mut self) -> &mut Self {
        self.add_provider_named(
            "kilo_gateway",
            Arc::new(crate::providers::anthropic::AnthropicProvider::new()),
        );
        self
    }

    /// Register Groq (`https://api.groq.com/openai/v1`, `GROQ_API_KEY`).
    pub fn with_groq(&mut self) -> &mut Self {
        self.add_provider_named(
            "groq",
            Arc::new(crate::providers::openai_compat::OpenAiCompatProvider::new_groq()),
        );
        self
    }

    /// Register OpenRouter (`https://openrouter.ai/api/v1`, `OPENROUTER_API_KEY`).
    pub fn with_openrouter(&mut self) -> &mut Self {
        self.add_provider_named(
            "openrouter",
            Arc::new(crate::providers::openai_compat::OpenAiCompatProvider::new_openrouter()),
        );
        self
    }

    /// Register DeepSeek (`https://api.deepseek.com/v1`, `DEEPSEEK_API_KEY`).
    pub fn with_deepseek(&mut self) -> &mut Self {
        self.add_provider_named(
            "deepseek",
            Arc::new(crate::providers::openai_compat::OpenAiCompatProvider::new_deepseek()),
        );
        self
    }

    /// Register Together AI (`https://api.together.xyz/v1`, `TOGETHER_API_KEY`).
    pub fn with_together(&mut self) -> &mut Self {
        self.add_provider_named(
            "together",
            Arc::new(crate::providers::openai_compat::OpenAiCompatProvider::new_together()),
        );
        self
    }

    pub fn with_openai(&mut self) -> &mut Self {
        self.add_provider_named(
            "openai",
            Arc::new(crate::providers::openai::OpenAiProvider::new()),
        );
        self
    }

    pub fn with_gemini(&mut self) -> &mut Self {
        self.add_provider_named(
            "gemini",
            Arc::new(crate::providers::gemini::GeminiProvider::new()),
        );
        self
    }

    pub fn with_xai(&mut self) -> &mut Self {
        self.add_provider_named("xai", Arc::new(crate::providers::xai::new()));
        self
    }

    pub fn with_mistral(&mut self) -> &mut Self {
        self.add_provider_named("mistral", Arc::new(crate::providers::mistral::new()));
        self
    }

    pub fn with_openai_compat(&mut self) -> &mut Self {
        self.add_provider_named(
            "openai_compat",
            Arc::new(crate::providers::openai_compat::OpenAiCompatProvider::new()),
        );
        self
    }

    pub fn set_fallback_chain(&mut self, chain: Vec<String>) {
        self.fallback_chain = chain;
    }

    /// Build a router from a resolved `LlmConfig`. Known provider names are
    /// registered via their respective builders; unknown names are skipped with
    /// a warning (forward-compatible: newly defined providers in a future
    /// config do not fail the loader). Tool routing and provider-specific
    /// request defaults are also imported from config.
    pub fn from_config(cfg: &LlmConfig) -> Self {
        let mut router = Self::new(&cfg.default_provider, cfg.budget_tokens);
        router.set_fallback_chain(cfg.fallback_chain.clone());
        router.set_tool_routing(cfg.routing.clone());

        for (name, provider_cfg) in &cfg.providers {
            if let Some(provider) = build_provider_from_config(name, provider_cfg) {
                router.provider_defaults.insert(
                    name.clone(),
                    ProviderDefaults {
                        max_tokens: provider_cfg.max_tokens,
                        temperature: provider_cfg.temperature,
                    },
                );
                router.add_provider_named(name.clone(), provider);
            }
        }

        router
    }

    pub fn default_provider(&self) -> &str {
        &self.default_provider
    }

    pub fn provider_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.providers.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    fn apply_provider_defaults(&self, provider_name: &str, request: &LlmRequest) -> LlmRequest {
        let mut merged = request.clone();
        if let Some(defaults) = self.provider_defaults.get(provider_name) {
            if merged.max_tokens.is_none() {
                merged.max_tokens = Some(defaults.max_tokens);
            }
            if merged.temperature.is_none() {
                merged.temperature = Some(defaults.temperature);
            }
        }
        merged
    }

    fn provider_chain(&self) -> Vec<String> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();

        for provider_name in std::iter::once(self.default_provider.as_str())
            .chain(self.fallback_chain.iter().map(String::as_str))
        {
            if seen.insert(provider_name) {
                chain.push(provider_name.to_string());
            }
        }

        chain
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

        let chain = self.provider_chain();

        let mut last_error = None;

        for provider_name in &chain {
            if let Some(provider) = self.providers.get(provider_name) {
                let request = self.apply_provider_defaults(provider_name, request);
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
                    cache_control_blocks: request.cache_control_blocks.clone(),
                    cache_control_hint: request.cache_control_hint,
                };

                let mut attempt = 0usize;
                loop {
                    attempt += 1;
                    SimpleRateLimiter::acquire(&self.rate_limiter).await;
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
                        Err(e) if is_retryable(&e) => {
                            let mut headers = std::collections::HashMap::new();
                            if let LlmError::RateLimited(_, retry_after_secs) = &e {
                                headers.insert(
                                    "retry-after".to_string(),
                                    retry_after_secs.to_string(),
                                );
                            }
                            match crate::retry::next_retry_decision(
                                &headers,
                                &self.retry_config,
                                attempt,
                                chrono::Utc::now(),
                            ) {
                                crate::retry::RetryDecision::Retry(mut delay) => {
                                    delay.delay_ms =
                                        delay.delay_ms.min(self.retry_config.max_backoff_ms);
                                    warn!(provider = provider_name, attempt, delay_ms = delay.delay_ms, error = %e, "Retrying after delay");
                                    tokio::time::sleep(std::time::Duration::from_millis(
                                        delay.delay_ms,
                                    ))
                                    .await;
                                }
                                crate::retry::RetryDecision::GiveUp => {
                                    warn!(provider = provider_name, error = %e, "Provider failed, trying next");
                                    last_error = Some(e);
                                    break;
                                }
                            }
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
        SimpleRateLimiter::acquire(&self.rate_limiter).await;

        let provider =
            self.providers
                .get(&self.default_provider)
                .ok_or_else(|| LlmError::Provider {
                    provider: self.default_provider.clone(),
                    status: None,
                    message: "Default provider not found".into(),
                })?;

        let request = self.apply_provider_defaults(&self.default_provider, request);
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
            cache_control_blocks: request.cache_control_blocks.clone(),
            cache_control_hint: request.cache_control_hint,
        };

        provider.stream(&sanitized_request).await
    }

    pub async fn token_usage(&self) -> (u64, u64) {
        let budget = self.budget.read().await;
        (budget.used(), budget.limit())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::config::LlmConfig;
    use crate::provider::{LlmRequest, Message};
    use std::sync::Mutex;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn router() -> LlmRouter {
        LlmRouter::new("default_prov", 1_000)
    }

    fn clear_env() {
        // SAFETY: ENV_LOCK serializes env mutation within this test module.
        unsafe {
            for key in [
                "ANTHROPIC_API_KEY",
                "OPENAI_API_KEY",
                "GEMINI_API_KEY",
                "XAI_API_KEY",
                "MISTRAL_API_KEY",
                "OPENAI_COMPAT_API_KEY",
                "OPENAI_COMPAT_BASE_URL",
            ] {
                std::env::remove_var(key);
            }
        }
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

    #[tokio::test]
    async fn rate_limiter_blocks_until_window_advances() {
        tokio::time::pause();
        let limiter = Arc::new(tokio::sync::Mutex::new(SimpleRateLimiter::new(1)));
        SimpleRateLimiter::acquire(&limiter).await;

        let handle = tokio::spawn({
            let limiter = limiter.clone();
            async move { SimpleRateLimiter::acquire(&limiter).await }
        });

        tokio::task::yield_now().await;
        assert!(!handle.is_finished());

        tokio::time::advance(std::time::Duration::from_secs(60)).await;
        let _ = handle.await;
    }

    #[tokio::test]
    async fn rate_limiter_zero_rpm_returns_immediately() {
        let limiter = Arc::new(tokio::sync::Mutex::new(SimpleRateLimiter::new(0)));
        // Should return immediately — zero RPM means no rate limiting.
        SimpleRateLimiter::acquire(&limiter).await;
    }

    #[tokio::test]
    async fn rate_limiter_prune_to_empty_does_not_panic() {
        tokio::time::pause();
        let limiter = Arc::new(tokio::sync::Mutex::new(SimpleRateLimiter::new(2)));

        // Fill the window
        SimpleRateLimiter::acquire(&limiter).await;
        SimpleRateLimiter::acquire(&limiter).await;

        // Advance past the window so all timestamps get pruned
        tokio::time::advance(std::time::Duration::from_secs(61)).await;

        // Should succeed without panic — timestamps were pruned to empty
        SimpleRateLimiter::acquire(&limiter).await;
    }

    #[test]
    fn from_config_registers_supported_providers_and_routing() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();

        // SAFETY: ENV_LOCK serializes env mutation within this test module.
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "anthropic-test");
            std::env::set_var("OPENAI_API_KEY", "openai-test");
            std::env::set_var("GEMINI_API_KEY", "gemini-test");
            std::env::set_var("XAI_API_KEY", "xai-test");
            std::env::set_var("MISTRAL_API_KEY", "mistral-test");
        }

        let cfg = LlmConfig::from_toml_str(
            r#"
[llm]
default_provider = "openai"
fallback_chain = ["gemini", "mistral"]
budget_tokens = 4096

[llm.routing]
lint = "openai"
review = "gemini"

[llm.providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-sonnet-4"

[llm.providers.openai]
api_key_env = "OPENAI_API_KEY"
model = "gpt-4o"

[llm.providers.gemini]
api_key_env = "GEMINI_API_KEY"
model = "gemini-2.0-flash"

[llm.providers.xai]
api_key_env = "XAI_API_KEY"
model = "grok-beta"

[llm.providers.mistral]
api_key_env = "MISTRAL_API_KEY"
model = "mistral-large-latest"

[llm.providers.openai_compat]
base_url = "http://127.0.0.1:11434/v1"
model = "llama3.1"
max_tokens = 2048
temperature = 0.25
"#,
        )
        .expect("config parses");

        let router = LlmRouter::from_config(&cfg);
        assert_eq!(router.default_provider(), "openai");
        assert_eq!(router.route_for_tool("lint"), "openai");
        assert_eq!(router.route_for_tool("review"), "gemini");

        let names = router.provider_names();
        assert_eq!(
            names,
            vec![
                "anthropic".to_string(),
                "gemini".to_string(),
                "mistral".to_string(),
                "openai".to_string(),
                "openai_compat".to_string(),
                "xai".to_string(),
            ]
        );
        for name in [
            "anthropic",
            "gemini",
            "mistral",
            "openai",
            "openai_compat",
            "xai",
        ] {
            assert!(router.has_provider(name), "missing provider {name}");
        }

        clear_env();
    }

    #[tokio::test]
    async fn from_config_applies_provider_defaults_to_requests() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_partial_json(serde_json::json!({
                "model": "llama3.1",
                "max_tokens": 2048,
                "temperature": 0.25
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-1",
                "object": "chat.completion",
                "model": "llama3.1",
                "choices": [{
                    "index": 0,
                    "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "hello"}
                }],
                "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
            })))
            .mount(&server)
            .await;

        let cfg = LlmConfig::from_toml_str(&format!(
            r#"
[llm]
default_provider = "openai_compat"
budget_tokens = 4096

[llm.providers.openai_compat]
base_url = "{}/v1"
model = "llama3.1"
max_tokens = 2048
temperature = 0.25
"#,
            server.uri()
        ))
        .expect("config parses");

        let router = LlmRouter::from_config(&cfg);
        let req = LlmRequest::new(vec![Message::user("hi")]);
        let resp = router.complete(&req).await.expect("complete");

        assert_eq!(resp.content, "hello");
        assert_eq!(resp.model, "llama3.1");
        assert_eq!(resp.usage.total_tokens, 3);
    }

    #[test]
    fn with_rate_limit_does_not_panic() {
        let r = router().with_rate_limit(60);
        // Verify RPM was set by acquiring (synchronous check)
        let limiter = r.rate_limiter.clone();
        let guard = limiter.try_lock().unwrap();
        assert_eq!(guard.requests_per_minute, 60);
    }

    #[test]
    fn provider_chain_deduplicates_default_and_fallback_entries() {
        let mut r = router();
        r.set_fallback_chain(vec![
            "default_prov".to_string(),
            "alt_prov".to_string(),
            "default_prov".to_string(),
            "alt_prov".to_string(),
        ]);

        assert_eq!(
            r.provider_chain(),
            vec!["default_prov".to_string(), "alt_prov".to_string()]
        );
    }

    #[test]
    fn is_retryable_structured_status() {
        // Retryable status codes
        for code in [429, 502, 503, 504] {
            let err = LlmError::Provider {
                provider: "test".into(),
                status: Some(code),
                message: "doesn't matter".into(),
            };
            assert!(is_retryable(&err), "status {code} should be retryable");
        }

        // Non-retryable status codes
        for code in [200, 400, 401, 403, 404, 500] {
            let err = LlmError::Provider {
                provider: "test".into(),
                status: Some(code),
                message: "error 503 429".into(), // misleading message
            };
            assert!(
                !is_retryable(&err),
                "status {code} should NOT be retryable even with misleading message"
            );
        }

        // No status → not retryable
        let err = LlmError::Provider {
            provider: "test".into(),
            status: None,
            message: "error 503".into(),
        };
        assert!(
            !is_retryable(&err),
            "None status should NOT be retryable even with status-like message"
        );

        // RateLimited is always retryable
        assert!(is_retryable(&LlmError::RateLimited("test".into(), 5)));

        // Other error types are not retryable
        assert!(!is_retryable(&LlmError::AllProvidersFailed));
    }

    struct MockRetryProvider {
        attempts: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl crate::provider::LlmProvider for MockRetryProvider {
        fn name(&self) -> &str {
            "mock_retry"
        }
        async fn complete(&self, _req: &LlmRequest) -> LlmResult<crate::provider::LlmResponse> {
            self.attempts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(LlmError::RateLimited("mock".into(), 0))
        }
        async fn stream(
            &self,
            _req: &LlmRequest,
        ) -> LlmResult<tokio::sync::mpsc::Receiver<crate::provider::StreamChunk>> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn router_respects_max_attempts_limit() {
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let provider = Arc::new(MockRetryProvider {
            attempts: attempts.clone(),
        });

        let mut r = LlmRouter::new("mock_retry", 1_000);
        r.add_provider(provider);
        r.retry_config.max_attempts = 2;
        r.retry_config.initial_backoff_ms = 1;
        r.retry_config.max_backoff_ms = 1;

        let req = LlmRequest::new(vec![Message::user("hi")]);
        let result = r.complete(&req).await;

        assert!(result.is_err());
        // max_attempts = 2 allows attempt 1, 2 (which are retried) and attempt 3 (which gives up).
        // Total provider calls = 3.
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    }
}
