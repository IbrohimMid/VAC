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
//! Every `ToolError` returned by the registry is mapped into a
//! `ToolResultEnvelope` with `kind = Error`. The dispatcher
//! itself does **not** return `EngineError` for tool failures
//! — the engine's dispatch loop continues to the next tool
//! call and writes a `tool_result` row in every case.
//!
//! Mapping table:
//!
//! * `ToolError::NotFound(_)` (registry says no such tool) →
//!   `error("tool '<n>' not registered", message)`
//! * `ToolError::InvalidArguments(_)` /
//!   `ToolError::SerializationError(_)` →
//!   `error("<tool> rejected arguments", message)`
//! * any other `ToolError` →
//!   `error("<tool> failed", message)`
//!
//! Panics inside `VilTool::execute` are **not** caught by this
//! dispatcher. Async panics are not interceptable via
//! `std::panic::catch_unwind` without isolating the future on
//! a separate task, and the registry does not isolate tool
//! work today. A panicking `VilTool` will unwind through the
//! engine's `submit_one` future. Hosts that need stronger
//! containment must implement it inside their `VilTool::execute`
//! body or wrap the dispatcher.

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
        // D8 hardening — go through `ToolRegistry::execute`
        // instead of `get` + direct `VilTool::execute`. The
        // registry path adds load-bearing semantics like
        // oversized-result spill (`maybe_spill_result`) that
        // we must inherit; bypassing it would diverge from
        // the rest of the VAC tool runtime.
        let outcome = self
            .registry
            .execute(&call.name, call.arguments.clone(), &self.context)
            .await;
        let duration_ms = started.elapsed().as_millis() as u64;

        match outcome {
            Ok(payload) => Ok(ToolResultEnvelope {
                kind: ToolResultKind::Ok,
                summary: format!("{} ok", call.name),
                payload,
                duration_ms,
            }),
            Err(e) => {
                let summary = match &e {
                    vac_tools::ToolError::NotFound(_) => {
                        format!("tool '{}' not registered", call.name)
                    }
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
