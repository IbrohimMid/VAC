//! LLM Router — manages multiple providers with fallback chain.

use crate::error::{LlmError, LlmResult};
use crate::provider::{LlmProvider, LlmRequest, LlmResponse, StreamChunk};
use crate::token_budget::TokenBudget;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn};

use crate::providers::anthropic::AnthropicProvider;

fn is_retryable(e: &LlmError) -> bool {
    match e {
        LlmError::RateLimited(_, _) => true,
        LlmError::Provider { message, .. } => {
            message.contains("429") || message.contains("503") || message.contains("502")
        }
        LlmError::Request(re) => re.status().map_or(false, |s| {
            s.as_u16() == 429 || s.as_u16() == 503 || s.as_u16() == 502
        }),
        _ => false,
    }
}

pub struct LlmRouter {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    default_provider: String,
    fallback_chain: Vec<String>,
    budget: Arc<RwLock<TokenBudget>>,
    retry_config: crate::retry::RetryConfig,
}

impl LlmRouter {
    pub fn new(default_provider: &str, budget_limit: u64) -> Self {
        Self {
            providers: HashMap::new(),
            default_provider: default_provider.to_string(),
            fallback_chain: vec![],
            budget: Arc::new(RwLock::new(TokenBudget::new(budget_limit))),
            retry_config: crate::retry::RetryConfig::default(),
        }
    }

    pub fn with_retry_config(mut self, config: crate::retry::RetryConfig) -> Self {
        self.retry_config = config;
        self
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

    pub fn set_fallback_chain(&mut self, chain: Vec<String>) {
        self.fallback_chain = chain;
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
                let mut attempt = 0usize;
                loop {
                    attempt += 1;
                    match provider.complete(request).await {
                        Ok(response) => {
                            let mut budget = self.budget.write().await;
                            budget.add_usage(response.usage.total_tokens);
                            info!(provider = provider_name, tokens = response.usage.total_tokens, "LLM request completed");
                            return Ok(response);
                        }
                        Err(e) if is_retryable(&e) && attempt < self.retry_config.max_attempts => {
                            let delay = crate::retry::resolve_retry_delay_ms(
                                &std::collections::HashMap::new(),
                                &self.retry_config,
                                attempt,
                                chrono::Utc::now(),
                            );
                            warn!(provider = provider_name, attempt, delay_ms = delay.delay_ms, error = %e, "Retrying after delay");
                            tokio::time::sleep(std::time::Duration::from_millis(delay.delay_ms)).await;
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

        provider.stream(request).await
    }

    pub async fn token_usage(&self) -> (u64, u64) {
        let budget = self.budget.read().await;
        (budget.used(), budget.limit())
    }
}
