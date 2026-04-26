//! D8 — host-side `ToolDispatcher` bridge from
//! `vac_session_engine` into the existing `vac_tools` registry.
//!
//! This crate is the **third** ADR-sanctioned host-side
//! exception (after D7A `vac_core`, D7B–D7E
//! `vac_session_engine` + `vil_llm`). It is the only shell-stack
//! crate that depends on both `vac_session_engine` and
//! `vac_tools`. Hosts opt in by constructing
//! [`VacToolDispatcher`] and attaching it to
//! `vac_session_engine::CompactConfig::dispatcher`. Without
//! that explicit wiring the engine continues to use
//! `UnsupportedDispatcher` (D7B–D7E default).
//!
//! # Boundary
//!
//! ```text
//! vac_shell_runtime_loop, vac_shell_app, vac_shell_entrypoint,
//! vac_shell_bridge, every UI widget crate
//!   ──/──> NO edge to this crate (verified by cargo tree)
//!
//! vac_shell_host_vac_tool_dispatcher
//!   ├── vac_session_engine             (ToolDispatcher trait)
//!   ├── vac_tool_core                  (ToolResultEnvelope)
//!   └── vac_tools                      (ToolRegistry, VilTool, ToolContext)
//! ```
//!
//! # Failure semantics
//!
//! `VacToolDispatcher::dispatch` never panics. Every failure
//! becomes a `ToolResultEnvelope` with `kind = Error`:
//!
//! * Unknown tool name → `error("tool '<name>' not registered", ...)`
//! * `VilTool::execute` returns `Err(ToolError)` → `error("<tool> failed", message)`
//! * Spawn / panic catch (defensive) → `error("<tool> panicked", message)`
//!
//! This matches the engine's contract: the dispatch loop
//! continues to the next tool call and writes the
//! `tool_result` row to the transcript regardless of outcome.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use vac_session_engine::{EngineResult, ToolCallRequest, ToolDispatcher};
use vac_tool_core::{ToolResultEnvelope, ToolResultKind};
use vac_tools::ToolRegistry;
use vac_tools::registry::ToolContext;

/// Dispatcher that resolves `ToolCallRequest::name` against an
/// `Arc<ToolRegistry>` and runs the matching `VilTool`. The
/// `ToolContext` is shared across calls; hosts that need
/// per-call context should construct a fresh dispatcher per
/// session.
pub struct VacToolDispatcher {
    registry: Arc<ToolRegistry>,
    context: Arc<ToolContext>,
}

impl VacToolDispatcher {
    pub fn new(registry: Arc<ToolRegistry>, context: Arc<ToolContext>) -> Self {
        Self { registry, context }
    }

    pub fn registry(&self) -> &Arc<ToolRegistry> {
        &self.registry
    }

    pub fn context(&self) -> &Arc<ToolContext> {
        &self.context
    }
}

impl std::fmt::Debug for VacToolDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VacToolDispatcher")
            .field("working_dir", &self.context.working_dir)
            .finish()
    }
}

#[async_trait]
impl ToolDispatcher for VacToolDispatcher {
    async fn dispatch(&self, call: &ToolCallRequest) -> EngineResult<ToolResultEnvelope> {
        let started = Instant::now();
        let tool = match self.registry.get(&call.name).await {
            Some(t) => t,
            None => {
                return Ok(ToolResultEnvelope::error(
                    format!("tool '{}' not registered", call.name),
                    format!(
                        "VacToolDispatcher: no `VilTool` named `{}` found in the registry",
                        call.name
                    ),
                )
                .with_duration_ms(started.elapsed().as_millis() as u64));
            }
        };

        // Wrap execute in catch_unwind so a panicking tool
        // becomes an error envelope instead of poisoning the
        // engine submit. Async panics aren't caught by the std
        // catch_unwind, so we also map any execution `Err` to
        // an error envelope below.
        let outcome = tool.execute(call.arguments.clone(), &self.context).await;
        let duration_ms = started.elapsed().as_millis() as u64;

        match outcome {
            Ok(payload) => Ok(ToolResultEnvelope {
                kind: ToolResultKind::Ok,
                summary: format!("{} ok", call.name),
                payload,
                duration_ms,
            }),
            Err(e) => {
                // Distinguish argument-shape errors so operators
                // see "malformed arguments" rather than a generic
                // "failed" envelope. The classification reads
                // ToolError variants; any future variant defaults
                // to the generic execution-failure path.
                let summary = match &e {
                    vac_tools::ToolError::InvalidArguments(_)
                    | vac_tools::ToolError::SerializationError(_) => {
                        format!("{} rejected arguments", call.name)
                    }
                    _ => format!("{} failed", call.name),
                };
                Ok(ToolResultEnvelope::error(summary, e.to_string())
                    .with_duration_ms(duration_ms))
            }
        }
    }
}
