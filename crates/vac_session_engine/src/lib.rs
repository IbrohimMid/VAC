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
pub mod event;
pub mod file_state_cache;
pub mod fork;
pub mod gate;
pub mod hooks_gate;
pub mod llm;
pub mod notify_hooks;
pub mod schedule;
pub mod slash;
pub mod stream;
pub mod subagent;
pub mod submit;
pub mod tool_use_replay;
pub mod transcript;
pub mod usage;
pub mod worktree;

// Re-export the primitives modules so `crate::cron::…`,
// `crate::hooks::…`, `crate::error::…`, `crate::web::…`,
// `crate::monitor::…` keep resolving for in-crate call sites.
pub use vac_session_primitives::{cron, error, hooks, monitor, web};

pub use agent_dispatch_impl::EngineAgentDispatcher;
pub use agent_tool::{
    AgentToolInput, BUILT_IN_SUBAGENTS, BuiltInAgentSpec, BuiltInKind, dispatch_agent_tool,
    find_built_in, resolve_subagent_kind,
};
pub use compact::{CompactBoundary, CompactHint, CompactInput, TrivialCompactBoundary};
pub use event::{SubmitContext, SubmitEvent};
pub use file_state_cache::{
    DEFAULT_FILE_STATE_CAPACITY, FileStateCache, FileStateEntry, ForkedCache,
};
pub use fork::{
    CacheSafeParams, ForkBudget, ForkResult, ForkedAgentRunner, MAX_SPECULATION_MESSAGES,
    MAX_SPECULATION_TURNS, OverlayGuard,
};
pub use gate::{
    CompositeGate, GateDecision, NoopHookGate, PlanModeGate, PolicyGate, ToolCheckCtx, ToolGate,
};
pub use hooks_gate::HookGate;
pub use llm::{
    CassetteAdapter, EchoAdapter, LlmAdapter, LlmRequest, LlmResponse, ToolCallRequest,
    ToolDispatcher, UnsupportedDispatcher,
};
pub use schedule::{
    MAX_DELAY, MIN_DELAY, WakeupSpec, clamp_delay, schedule_interval_loop, schedule_wakeup,
};
pub use slash::{SlashCommand, SlashProcessor};
pub use stream::{SubmitChunk, SubmitStream, submit_stream};
pub use subagent::{SubagentDispatchContext, SubagentKind, SubagentRunner, SubagentSpec};
pub use submit::{CompactConfig, submit_one};
pub use tool_use_replay::{ToolUseReplayError, ToolUseTranscriptView, read_tool_use_rows};
pub use transcript::{TranscriptEntry, TranscriptHandle, TranscriptKind, TranscriptWriter};
pub use usage::{UsageSnapshot, UsageTracker};
#[allow(deprecated)]
pub use vac_session_primitives::exec_hook;
pub use vac_session_primitives::fetch as web_fetch;
pub use vac_session_primitives::{AgentDispatchInput, AgentDispatcher};
pub use vac_session_primitives::{
    BraveBackend, CronEntry, CronStore, DEFAULT_CRON_FILENAME, DEFAULT_HOOKS_FILENAME,
    DEFAULT_RESPONSE_CAP, HookCommand, HookDecision, HookEntry, HookEvent, HookSandbox, HookStore,
    MonitorHandle, MonitorLine, MonitorSpec, SearchBackend, WebFetchRequest, WebFetchResult,
    WebSearchHit, WebSearchRequest, exec_hook_sandboxed, spawn_monitor, unix_now,
    validate_hook_store,
};
pub use vac_session_primitives::{EngineError, EngineResult};
pub use worktree::{
    EnterWorktreeRequest, ExitWorktreeRequest, WorktreeHandle, enter_worktree, exit_worktree,
};
