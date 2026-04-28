//! VIL LLM — Cloud LLM provider abstraction with streaming, fallback, and token tracking.

pub mod config;
pub mod error;
pub mod models;
pub mod provider;
pub mod providers;
pub mod rate_limit;
pub mod retry;
pub mod router;
pub mod rulebook_hook;
pub mod sanitize;
pub mod streaming;
pub mod token_budget;
pub mod tokenizer;

pub use config::{LlmConfig, LlmConfigError, ProviderConfig};
pub use error::LlmError;
pub use models::{LlmContent, LlmMessage, LlmRole};
pub use provider::{CacheControlHint, LlmProvider, LlmRequest, LlmResponse, Message, Role};
pub use rate_limit::{
    Clock, DEFAULT_BACKOFF, FakeClock, JITTER_FRACTION, MAX_BACKOFF, RPM_WINDOW_SECS,
    RateLimitTracker, SystemClock,
};
pub use router::LlmRouter;
pub use rulebook_hook::RulebookContext;
pub use token_budget::TokenBudget;
pub use tokenizer::{HeuristicAdapter, TiktokenAdapter, Tokenizer};
