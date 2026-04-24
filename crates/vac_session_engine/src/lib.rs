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

pub mod agent_tool;
pub mod compact;
pub mod cron;
pub mod error;
pub mod monitor;
pub mod event;
pub mod gate;
pub mod file_state_cache;
pub mod fork;
pub mod llm;
pub mod slash;
pub mod stream;
pub mod subagent;
pub mod submit;
pub mod transcript;
pub mod usage;

pub use compact::{CompactBoundary, CompactHint, CompactInput, TrivialCompactBoundary};
pub use error::{EngineError, EngineResult};
pub use file_state_cache::{
    FileStateCache, FileStateEntry, ForkedCache, DEFAULT_FILE_STATE_CAPACITY,
};
pub use fork::{
    CacheSafeParams, ForkBudget, ForkResult, ForkedAgentRunner, OverlayGuard,
    MAX_SPECULATION_MESSAGES, MAX_SPECULATION_TURNS,
};
pub use event::{SubmitContext, SubmitEvent};
pub use llm::{EchoAdapter, LlmAdapter, LlmRequest, LlmResponse};
pub use slash::{SlashCommand, SlashProcessor};
pub use gate::{
    CompositeGate, GateDecision, NoopHookGate, PlanModeGate, PolicyGate, ToolCheckCtx,
    ToolGate,
};
pub use stream::{SubmitChunk, SubmitStream, submit_stream};
pub use cron::{CronEntry, CronStore, DEFAULT_CRON_FILENAME, unix_now};
pub use monitor::{MonitorHandle, MonitorLine, MonitorSpec, spawn_monitor};
pub use agent_tool::{
    AgentToolInput, BUILT_IN_SUBAGENTS, BuiltInAgentSpec, BuiltInKind,
    dispatch_agent_tool, find_built_in, resolve_subagent_kind,
};
pub use subagent::{
    SubagentDispatchContext, SubagentKind, SubagentRunner, SubagentSpec,
};
pub use submit::{CompactConfig, submit_one};
pub use transcript::{TranscriptEntry, TranscriptHandle, TranscriptKind, TranscriptWriter};
pub use usage::{UsageSnapshot, UsageTracker};
