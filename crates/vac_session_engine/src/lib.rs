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

pub mod agent_dispatch_impl;
pub mod agent_tool;
pub mod compact;
pub mod schedule;
pub mod worktree;
pub mod event;
pub mod gate;
pub mod hooks_gate;
pub mod file_state_cache;
pub mod fork;
pub mod llm;
pub mod slash;
pub mod stream;
pub mod subagent;
pub mod submit;
pub mod tool_use_replay;
pub mod transcript;
pub mod usage;

// Re-export the primitives modules so `crate::cron::…`,
// `crate::hooks::…`, `crate::error::…`, `crate::web::…`,
// `crate::monitor::…` keep resolving for in-crate call sites.
pub use vac_session_primitives::{cron, error, hooks, monitor, web};

pub use compact::{CompactBoundary, CompactHint, CompactInput, TrivialCompactBoundary};
pub use vac_session_primitives::{EngineError, EngineResult};
pub use file_state_cache::{
    FileStateCache, FileStateEntry, ForkedCache, DEFAULT_FILE_STATE_CAPACITY,
};
pub use fork::{
    CacheSafeParams, ForkBudget, ForkResult, ForkedAgentRunner, OverlayGuard,
    MAX_SPECULATION_MESSAGES, MAX_SPECULATION_TURNS,
};
pub use event::{SubmitContext, SubmitEvent};
pub use llm::{
    CassetteAdapter, EchoAdapter, LlmAdapter, LlmRequest, LlmResponse, ToolCallRequest,
    ToolDispatcher, UnsupportedDispatcher,
};
pub use slash::{SlashCommand, SlashProcessor};
pub use gate::{
    CompositeGate, GateDecision, NoopHookGate, PlanModeGate, PolicyGate, ToolCheckCtx,
    ToolGate,
};
pub use stream::{SubmitChunk, SubmitStream, submit_stream};
#[allow(deprecated)]
pub use vac_session_primitives::exec_hook;
pub use vac_session_primitives::{
    CronEntry, CronStore, DEFAULT_CRON_FILENAME, unix_now,
    DEFAULT_HOOKS_FILENAME, HookCommand, HookDecision, HookEntry, HookEvent,
    HookSandbox, HookStore, exec_hook_sandboxed, validate_hook_store,
    MonitorHandle, MonitorLine, MonitorSpec, spawn_monitor,
    BraveBackend, DEFAULT_RESPONSE_CAP, SearchBackend, WebFetchRequest,
    WebFetchResult, WebSearchHit, WebSearchRequest,
};
pub use vac_session_primitives::fetch as web_fetch;
pub use hooks_gate::HookGate;
pub use worktree::{
    EnterWorktreeRequest, ExitWorktreeRequest, WorktreeHandle, enter_worktree,
    exit_worktree,
};
pub use schedule::{
    MAX_DELAY, MIN_DELAY, WakeupSpec, clamp_delay, schedule_interval_loop,
    schedule_wakeup,
};
pub use agent_dispatch_impl::EngineAgentDispatcher;
pub use agent_tool::{
    AgentToolInput, BUILT_IN_SUBAGENTS, BuiltInAgentSpec, BuiltInKind,
    dispatch_agent_tool, find_built_in, resolve_subagent_kind,
};
pub use vac_session_primitives::{AgentDispatchInput, AgentDispatcher};
pub use subagent::{
    SubagentDispatchContext, SubagentKind, SubagentRunner, SubagentSpec,
};
pub use submit::{CompactConfig, submit_one};
pub use tool_use_replay::{ToolUseReplayError, ToolUseTranscriptView, read_tool_use_rows};
pub use transcript::{TranscriptEntry, TranscriptHandle, TranscriptKind, TranscriptWriter};
pub use usage::{UsageSnapshot, UsageTracker};
