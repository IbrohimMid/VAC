//! VIL LLM — Cloud LLM provider abstraction with streaming, fallback, and token tracking.

pub mod error;
pub mod models;
pub mod provider;
pub mod providers;
pub mod retry;
pub mod router;
pub mod rulebook_hook;
pub mod sanitize;
pub mod streaming;
pub mod token_budget;

pub use error::LlmError;
pub use models::{LlmContent, LlmMessage, LlmRole};
pub use provider::{LlmProvider, LlmRequest, LlmResponse, Message, Role};
pub use router::LlmRouter;
pub use rulebook_hook::RulebookContext;
pub use token_budget::TokenBudget;
