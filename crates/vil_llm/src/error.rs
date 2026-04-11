use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Provider '{provider}' error: {message}")]
    Provider { provider: String, message: String },

    #[error("API key not found for provider '{0}' (env: {1})")]
    ApiKeyMissing(String, String),

    #[error("Token budget exceeded: used {used}, limit {limit}")]
    BudgetExceeded { used: u64, limit: u64 },

    #[error("All providers in fallback chain failed")]
    AllProvidersFailed,

    #[error("Rate limited by provider '{0}', retry after {1}s")]
    RateLimited(String, u64),

    #[error("Streaming error: {0}")]
    Streaming(String),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type LlmResult<T> = Result<T, LlmError>;
