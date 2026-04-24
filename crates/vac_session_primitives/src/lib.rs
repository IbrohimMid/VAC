//! Leaf-level primitives shared by `vac_session_engine` and
//! `vac_tools`. Extracted to break the dependency cycle
//! (`vac_tools → vac_session_engine → vac_core → vac_tools`) so
//! the LLM-facing tool wrappers can reuse the same storage types
//! and transport helpers that the session engine uses internally.
//!
//! No session-runtime deps (no LlmAdapter, SubagentRunner,
//! SubmitStream); this crate sits below them in the graph.

pub mod error;
pub mod cron;
pub mod hooks;
pub mod web;
pub mod monitor;

pub use error::{EngineError, EngineResult};
pub use cron::{CronEntry, CronStore, DEFAULT_CRON_FILENAME, unix_now};
#[allow(deprecated)]
pub use hooks::exec_hook;
pub use hooks::{
    DEFAULT_HOOKS_FILENAME, HOOK_ARGV_MAX_LEN, HOOK_HTTP_SCHEME_ALLOWLIST,
    HOOK_PROMPT_MAX_LEN, HookCommand, HookDecision, HookEntry, HookEvent, HookSandbox,
    HookStore, exec_hook_sandboxed, validate_hook_store,
};
pub use web::{
    BraveBackend, DEFAULT_RESPONSE_CAP, DEFAULT_TIMEOUT, REQUEST_HEADER_ALLOWLIST,
    SearchBackend, WebFetchRequest, WebFetchResult, WebSearchHit, WebSearchRequest, fetch,
};
pub use monitor::{MonitorHandle, MonitorLine, MonitorSpec, spawn_monitor};
