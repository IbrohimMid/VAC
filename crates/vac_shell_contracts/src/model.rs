//! Model adapter — flat projection of a VAC LLM provider/model into
//! the shape the donor model switcher widget renders. We deliberately
//! do not re-export `vil_llm` types here so the donor never depends on
//! VAC internals.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacModelView {
    pub provider: ProviderId,
    /// Stable identifier sent to the provider (e.g. `claude-3-7-sonnet-20250219`).
    pub id: String,
    /// Human-friendly display name (e.g. `Claude Sonnet 4.5`).
    pub label: String,
    /// True when this model is the currently active routing target.
    pub active: bool,
    /// True when credentials for this provider have been resolved.
    pub credentials_present: bool,
}
