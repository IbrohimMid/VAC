//! VAC Session Engine — submit-message lifecycle and transcript
//! durability, decoupled from any particular UI.
//!
//! ## Why a dedicated crate?
//!
//! The submit-message lifecycle (prompt assembly, permission checks,
//! transcript write, tool-use stream, compact boundary, usage
//! accounting) is complex and used by at least three drivers:
//!
//! - **`vac_tui_runtime`** — interactive REPL cockpit
//! - **`vac_cli`** — headless one-shot `vac run "..."` flow
//! - **`vac_bridge`** (Fase 5) — remote/ACP companion surface
//!
//! Keeping this logic in the TUI crate couples headless and remote
//! flows to a UI they don't need. Moving it into a dedicated crate
//! gives every driver the same durability + observability for free.
//!
//! Crate uses only `vac_tool_core` + tokio; no UI deps, no heavy
//! subsystem. Downstream crates wire concrete LLM adapters + tool
//! registries in via the trait surface.
//!
//! ## Core types
//!
//! - [`SubmitContext`] — what the caller submits
//! - [`SubmitEvent`] — what the engine emits during execution
//! - [`TranscriptHandle`] — durability primitive (write-ahead log)
//! - Module `slash` — slash command registry + dispatch
//! - Module `compact` — context-budget boundary hooks
//! - Module `usage` — token/cost accounting

pub mod compact;
pub mod error;
pub mod event;
pub mod llm;
pub mod slash;
pub mod submit;
pub mod transcript;
pub mod usage;

pub use compact::{CompactBoundary, CompactHint, CompactInput, TrivialCompactBoundary};
pub use error::{EngineError, EngineResult};
pub use event::{SubmitContext, SubmitEvent};
pub use llm::{EchoAdapter, LlmAdapter, LlmRequest, LlmResponse};
pub use slash::{SlashCommand, SlashProcessor};
pub use submit::{CompactConfig, submit_one};
pub use transcript::{TranscriptEntry, TranscriptHandle, TranscriptKind, TranscriptWriter};
pub use usage::{UsageSnapshot, UsageTracker};
