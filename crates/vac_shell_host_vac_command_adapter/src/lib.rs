//! D7B — host-side `ShellCommandExecutor` backed by
//! `vac_session_engine::submit_one`.
//!
//! This crate is the **second** ADR-sanctioned exception in the
//! shell stack allowed to depend on an engine crate. The first
//! (D7A) is `vac_shell_host_vac_engine_probe`. Both crates are
//! producer/executor adapters installed by *hosts* — not reached
//! by any UI / widget / bridge / app / runtime-loop graph.
//!
//! # Boundary
//!
//! ```text
//! vac_shell_runtime_loop
//!   └── vac_shell_host_commands         (trait only — no engine dep)
//!
//! vac_shell_host_vac_command_adapter
//!   ├── vac_shell_host_commands         (impls the trait)
//!   ├── vac_shell_contracts             (ShellCommandSpec)
//!   └── vac_session_engine              (D7B exception)
//! ```
//!
//! No UI / widget / bridge / app crate may depend on this
//! crate. Hosts attach it through
//! `ShellRuntimeContext::with_executor(Arc::new(adapter))`
//! before entering the runtime loop.
//!
//! # Execution model (D7B v1)
//!
//! Custom palette slashes that have an explicit
//! [`AdapterCommandSpec`] mapping are submitted to
//! `submit_one` with the configured prompt template. Unmapped
//! commands return `ShellCommandError::Unsupported(...)` so the
//! existing operator-visible error path keeps working.
//!
//! `EchoAdapter` is the LLM stub for v1 — this slice proves the
//! engine seam, transcript durability, and the host-side
//! routing direction. Real provider routing is a later slice.
//!
//! # Sync ↔ async bridge
//!
//! `ShellCommandExecutor::execute` is a sync trait, but
//! `submit_one` is `async`. The adapter resolves this without
//! infecting the shell trait:
//!
//! * If a tokio runtime is already on the calling thread
//!   (typical for a host invoking from inside a `tokio::main`),
//!   the adapter uses `tokio::task::block_in_place` +
//!   `Handle::block_on`. This requires the multi-thread runtime
//!   flavour; tests + dogfood use it.
//! * Otherwise it spins up a private current-thread runtime
//!   and `block_on`s.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tokio::runtime::{Builder, Handle, RuntimeFlavor};
use uuid::Uuid;

use async_trait::async_trait;
use vac_session_engine::{
    CompactConfig, CompositeGate, EchoAdapter, LlmAdapter, LlmRequest, LlmResponse,
    SlashProcessor, SubmitContext, ToolCallRequest, ToolDispatcher, TranscriptWriter,
    TrivialCompactBoundary, UsageTracker, submit_one,
};
use vac_session_engine::EngineError;
use vac_shell_contracts::ShellCommandSpec;
use vac_shell_host_commands::{ShellCommandError, ShellCommandExecutor};

// =====================================================================
// Public configuration types
// =====================================================================

/// One mapping from a registry slash command to a concrete
/// engine submit prompt. The adapter only executes commands
/// that the host has explicitly mapped; unmapped commands are
/// rejected as [`ShellCommandError::Unsupported`].
#[derive(Debug, Clone)]
pub struct AdapterCommandSpec {
    /// Match against [`ShellCommandSpec::id`] first.
    pub id: String,
    /// Match against [`ShellCommandSpec::slash`] as fallback.
    pub slash: String,
    /// Prompt text submitted to the engine when this command
    /// fires.
    pub prompt: String,
}

impl AdapterCommandSpec {
    pub fn new(
        id: impl Into<String>,
        slash: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            slash: slash.into(),
            prompt: prompt.into(),
        }
    }
}

/// LLM adapter selection for [`VacCommandExecutorAdapter`].
///
/// D7B v1 only shipped `Echo`. D7C adds `Custom`, which lets a
/// host inject any `LlmAdapter` implementation — including the
/// `vil_llm`-backed bridge produced by
/// [`AdapterConfig::with_vil_llm_router`].
#[derive(Clone)]
pub enum AdapterLlm {
    /// Default — `vac_session_engine::EchoAdapter`. Deterministic
    /// echo response, useful for the dogfood example and any
    /// host that wants engine-seam wiring without a provider
    /// round-trip.
    Echo,
    /// Host-injected adapter. The provided implementation is
    /// driven by `submit_one` exactly the same way `EchoAdapter`
    /// is. Errors returned by the adapter become
    /// `ShellCommandError::Failed` at the trait surface.
    Custom(Arc<dyn LlmAdapter>),
}

impl Default for AdapterLlm {
    fn default() -> Self {
        Self::Echo
    }
}

impl std::fmt::Debug for AdapterLlm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterLlm::Echo => f.write_str("AdapterLlm::Echo"),
            AdapterLlm::Custom(_) => f.write_str("AdapterLlm::Custom(<dyn LlmAdapter>)"),
        }
    }
}

/// D8 — error returned by the pre-flight checks that guard
/// [`AdapterConfig::try_with_tool_dispatcher`]. Live tool
/// dispatch without a `CompositeGate` is rejected here so a
/// host cannot accidentally turn on real tool execution
/// without a policy / hook gate path attached.
#[derive(Debug, thiserror::Error)]
pub enum AdapterConfigError {
    #[error(
        "live tool dispatcher attached without a CompositeGate — D8 requires a gate to enforce policy/hook checks"
    )]
    DispatcherWithoutGate,
}

/// Configuration for the real D7B/D7C/D8 adapter. Hosts
/// construct one of these per session and pass it to
/// [`VacCommandExecutorAdapter::new`].
#[derive(Clone)]
pub struct AdapterConfig {
    /// Project root used for transcript persistence
    /// (`<root>/.vac/sessions/<session_id>.jsonl`).
    pub project_root: PathBuf,
    /// Explicit command map. Order is irrelevant; lookup is by
    /// id or slash.
    pub commands: Vec<AdapterCommandSpec>,
    /// LLM adapter wired into `submit_one`. Defaults to
    /// [`AdapterLlm::Echo`] so D7B behaviour is unchanged for
    /// callers that do not opt in.
    pub llm: AdapterLlm,
    /// D8 — opt-in `ToolDispatcher`. Default `None` keeps the
    /// engine on `UnsupportedDispatcher` so unmapped tools
    /// continue to write `tool_result.kind=error` envelopes
    /// without aborting the submit. Must be paired with `gate`
    /// — `try_with_tool_dispatcher` enforces that.
    pub tool_dispatcher: Option<Arc<dyn ToolDispatcher>>,
    /// D8 — opt-in `CompositeGate`. Required whenever
    /// `tool_dispatcher` is `Some`. Hosts compose the gate
    /// (PolicyGate + HookGate + …) before passing it in.
    pub gate: Option<Arc<CompositeGate>>,
}

impl std::fmt::Debug for AdapterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterConfig")
            .field("project_root", &self.project_root)
            .field("commands", &self.commands)
            .field("llm", &self.llm)
            .field(
                "tool_dispatcher",
                &self.tool_dispatcher.as_ref().map(|_| "<dyn ToolDispatcher>"),
            )
            .field("gate", &self.gate.as_ref().map(|_| "<CompositeGate>"))
            .finish()
    }
}

impl AdapterConfig {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            commands: Vec::new(),
            llm: AdapterLlm::Echo,
            tool_dispatcher: None,
            gate: None,
        }
    }

    /// D8 — attach a live `ToolDispatcher` together with a
    /// `CompositeGate`. Both are mandatory: a live dispatcher
    /// without a gate is rejected by `try_with_tool_dispatcher`
    /// so this infallible variant simply takes both.
    pub fn with_tool_dispatcher(
        mut self,
        dispatcher: Arc<dyn ToolDispatcher>,
        gate: Arc<CompositeGate>,
    ) -> Self {
        self.tool_dispatcher = Some(dispatcher);
        self.gate = Some(gate);
        self
    }

    /// D8 — fallible variant used by builders that may produce
    /// a `None` gate (e.g. config files where the gate is
    /// optional). Returns `Err(DispatcherWithoutGate)` when a
    /// dispatcher is supplied without a gate.
    pub fn try_with_tool_dispatcher(
        mut self,
        dispatcher: Arc<dyn ToolDispatcher>,
        gate: Option<Arc<CompositeGate>>,
    ) -> Result<Self, AdapterConfigError> {
        let Some(gate) = gate else {
            return Err(AdapterConfigError::DispatcherWithoutGate);
        };
        self.tool_dispatcher = Some(dispatcher);
        self.gate = Some(gate);
        Ok(self)
    }

    pub fn with_command(mut self, spec: AdapterCommandSpec) -> Self {
        self.commands.push(spec);
        self
    }

    /// Inject any `LlmAdapter` implementation. Use this for
    /// cassette playback in tests, custom routing in hosts, or
    /// the `vil_llm` bridge via [`Self::with_vil_llm_router`].
    pub fn with_llm(mut self, llm: Arc<dyn LlmAdapter>) -> Self {
        self.llm = AdapterLlm::Custom(llm);
        self
    }

    /// D7C — convenience wrapper that turns a `vil_llm::LlmRouter`
    /// into a `LlmAdapter`. The host configures providers (and
    /// implicitly env-var-based credentials) on the router; the
    /// adapter just translates each `submit_one` request into a
    /// single-message `vil_llm::LlmRequest` and forwards it.
    /// Errors from the router become `EngineError::Other`, which
    /// surface to the operator via the existing
    /// `ShellCommandError::Failed` path.
    pub fn with_vil_llm_router(self, router: vil_llm::LlmRouter) -> Self {
        self.with_llm(Arc::new(VilLlmRouterAdapter::new(router)))
    }

    /// Convenience preset for the dogfood example. Maps two
    /// well-known custom slashes to engine submits with safe
    /// boilerplate prompts.
    pub fn dogfood(project_root: impl Into<PathBuf>) -> Self {
        Self::new(project_root)
            .with_command(AdapterCommandSpec::new(
                "memorize",
                "/memorize",
                "Memorize the current operator context and \
                 summarize persistent facts. (D7B dogfood preset.)",
            ))
            .with_command(AdapterCommandSpec::new(
                "ultraplan",
                "/ultraplan",
                "Produce an exhaustive plan for the operator's \
                 current task. (D7B dogfood preset.)",
            ))
    }
}

// =====================================================================
// Adapter
// =====================================================================

/// Real engine-backed `ShellCommandExecutor`. Replaces the D5.1
/// stub in `vac_shell_host_commands` for hosts that opt into
/// the engine bridge.
pub struct VacCommandExecutorAdapter {
    project_root: PathBuf,
    by_id: HashMap<String, AdapterCommandSpec>,
    by_slash: HashMap<String, AdapterCommandSpec>,
    llm: AdapterLlm,
    tool_dispatcher: Option<Arc<dyn ToolDispatcher>>,
    gate: Option<Arc<CompositeGate>>,
    /// Last completed transcript path — recorded after a
    /// successful submit so tests can assert engine reach
    /// without re-deriving the path.
    last_transcript: Arc<std::sync::Mutex<Option<PathBuf>>>,
    /// D10 — optional activity log for live feed bridging.
    /// When set, each submit spawns a background task that drains
    /// `SubmitChunk` events into the log via
    /// `vac_shell_host_event_projection::spawn_activity_feed_bridge`.
    activity_log: Option<vac_shell_host_activity::ActivityLog>,
}

impl VacCommandExecutorAdapter {
    pub fn new(config: AdapterConfig) -> Self {
        let mut by_id = HashMap::with_capacity(config.commands.len());
        let mut by_slash = HashMap::with_capacity(config.commands.len());
        for spec in config.commands {
            by_id.insert(spec.id.clone(), spec.clone());
            by_slash.insert(spec.slash.clone(), spec);
        }
        Self {
            project_root: config.project_root,
            by_id,
            by_slash,
            llm: config.llm,
            tool_dispatcher: config.tool_dispatcher,
            gate: config.gate,
            last_transcript: Arc::new(std::sync::Mutex::new(None)),
            activity_log: None,
        }
    }

    /// Whether this adapter has a live `ToolDispatcher` attached.
    /// Tests use this to assert default-config inertness.
    pub fn has_live_tool_dispatcher(&self) -> bool {
        self.tool_dispatcher.is_some()
    }

    /// D10 — attach an `ActivityLog`. When set, each `execute` call
    /// spawns a fire-and-forget bridge task that forwards
    /// `SubmitChunk` events into the log as `ShellActivityEntry` rows.
    pub fn with_activity_log(mut self, log: vac_shell_host_activity::ActivityLog) -> Self {
        self.activity_log = Some(log);
        self
    }
}

impl std::fmt::Debug for VacCommandExecutorAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VacCommandExecutorAdapter")
            .field("project_root", &self.project_root)
            .field("commands_by_id", &self.by_id.keys().collect::<Vec<_>>())
            .field("llm", &self.llm)
            .field(
                "tool_dispatcher",
                &self.tool_dispatcher.as_ref().map(|_| "<dyn ToolDispatcher>"),
            )
            .field("gate", &self.gate.as_ref().map(|_| "<CompositeGate>"))
            .finish()
    }
}

// (Inner-impl placeholder closed by the explicit Debug impl above.)
#[allow(dead_code)]
impl VacCommandExecutorAdapter {

    /// Path to the most recently written transcript, if any. Set
    /// by `execute` after a successful `submit_one`.
    pub fn last_transcript(&self) -> Option<PathBuf> {
        self.last_transcript
            .lock()
            .ok()
            .and_then(|g| g.clone())
    }

    fn resolve(&self, command: &ShellCommandSpec) -> Option<&AdapterCommandSpec> {
        self.by_id
            .get(&command.id)
            .or_else(|| self.by_slash.get(&command.slash))
    }

    fn run_submit(
        &self,
        command: &ShellCommandSpec,
        mapping: &AdapterCommandSpec,
    ) -> Result<PathBuf, String> {
        let project_root = self.project_root.clone();
        let prompt = mapping.prompt.clone();
        let metadata = json!({
            "source": "shell_palette",
            "command_id": command.id,
            "slash": command.slash,
            "title": command.title,
        });

        let session_id = Uuid::new_v4();
        let writer = TranscriptWriter::new(project_root.clone());
        let slash = SlashProcessor::new();
        let compact = TrivialCompactBoundary::default();
        let usage = UsageTracker::new();
        let ctx = SubmitContext::new(session_id, prompt).with_metadata(metadata);

        // D8 — thread the optional tool dispatcher + gate
        // through to CompactConfig. When neither is attached
        // (D7B–D7E default), the engine continues to use
        // UnsupportedDispatcher.
        let mut compact_cfg = CompactConfig::default();
        compact_cfg.dispatcher = self.tool_dispatcher.clone();
        compact_cfg.gate = self.gate.clone();

        // D7C — pick the LLM adapter chosen at config time.
        // D10 — if an activity_log is wired, create a submit event channel
        // and spawn the bridge before the submit starts.
        let llm_choice = self.llm.clone();
        let activity_log = self.activity_log.clone();
        let session_id_str = session_id.to_string();
        let fut = async move {
            // D10 — build optional event sender for the live feed bridge.
            let (event_tx, bridge_handle) = if let Some(log) = activity_log {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let stream = {
                    use tokio_stream::wrappers::UnboundedReceiverStream;
                    use vac_session_engine::stream::SubmitChunk;
                    let raw: tokio_stream::wrappers::UnboundedReceiverStream<
                        vac_session_engine::event::SubmitEvent,
                    > = UnboundedReceiverStream::new(rx);
                    use futures::StreamExt as _;
                    let chunk_stream = raw.map(SubmitChunk::from);
                    Box::pin(chunk_stream) as vac_session_engine::stream::SubmitStream
                };
                let handle = vac_shell_host_event_projection::spawn_activity_feed_bridge(
                    stream,
                    log,
                    session_id_str,
                );
                (Some(tx), Some(handle))
            } else {
                (None, None)
            };

            let echo;
            let llm_ref: &dyn LlmAdapter = match &llm_choice {
                AdapterLlm::Echo => {
                    echo = EchoAdapter;
                    &echo
                }
                AdapterLlm::Custom(arc) => arc.as_ref(),
            };
            let result = submit_one(
                ctx,
                &writer,
                &slash,
                &compact,
                &usage,
                llm_ref,
                compact_cfg,
                event_tx,
            )
            .await
            .map(|_snapshot| ());

            // Wait for bridge to drain (fire-and-forget is also fine — drop handle).
            if let Some(h) = bridge_handle {
                let _ = h.await;
            }
            result
        };

        match Handle::try_current() {
            Ok(handle) => {
                let result = match handle.runtime_flavor() {
                    RuntimeFlavor::MultiThread => {
                        tokio::task::block_in_place(|| handle.block_on(fut))
                    }
                    // Current-thread runtime: cannot block_on the
                    // same runtime; fall back to a fresh one on a
                    // helper thread so we never deadlock.
                    _ => run_in_helper_thread(fut)?,
                };
                result.map_err(|e| e.to_string())?;
            }
            Err(_) => {
                // No tokio runtime present — common in tests and
                // single-threaded host bootstraps.
                let rt = Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("adapter runtime build failed: {e}"))?;
                rt.block_on(fut).map_err(|e| e.to_string())?;
            }
        }

        let transcript_path = project_root
            .join(".vac")
            .join("sessions")
            .join(format!("{session_id}.jsonl"));
        Ok(transcript_path)
    }
}

fn run_in_helper_thread<F>(fut: F) -> Result<F::Output, String>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = match Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(Err(format!("adapter runtime build failed: {e}")));
                return;
            }
        };
        let _ = tx.send(Ok(rt.block_on(fut)));
    });
    rx.recv()
        .map_err(|e| format!("adapter helper thread closed: {e}"))?
}

impl ShellCommandExecutor for VacCommandExecutorAdapter {
    fn execute(&self, command: &ShellCommandSpec) -> Result<(), ShellCommandError> {
        let mapping = match self.resolve(command) {
            Some(m) => m.clone(),
            None => {
                return Err(ShellCommandError::Unsupported(format!(
                    "{} — no adapter mapping for command id `{}`",
                    command.slash, command.id,
                )));
            }
        };

        match self.run_submit(command, &mapping) {
            Ok(path) => {
                if let Ok(mut g) = self.last_transcript.lock() {
                    *g = Some(path);
                }
                Ok(())
            }
            Err(e) => Err(ShellCommandError::Failed(e)),
        }
    }
}

// =====================================================================
// D7C — vil_llm router → vac_session_engine::LlmAdapter bridge
// =====================================================================

/// Adapter that turns a `vil_llm::LlmRouter` into a
/// `vac_session_engine::LlmAdapter`. The translation is
/// deliberately small: each `submit_one` request becomes a
/// single-message `vil_llm::LlmRequest` (role = user). The
/// router is responsible for provider selection, credential
/// resolution, retry, and fallback.
pub struct VilLlmRouterAdapter {
    router: vil_llm::LlmRouter,
}

impl std::fmt::Debug for VilLlmRouterAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VilLlmRouterAdapter")
            .field("default_provider", &self.router.default_provider())
            .finish()
    }
}

impl VilLlmRouterAdapter {
    pub fn new(router: vil_llm::LlmRouter) -> Self {
        Self { router }
    }
}

#[async_trait]
impl LlmAdapter for VilLlmRouterAdapter {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, EngineError> {
        let prompt_text = if req.context.is_empty() {
            req.prompt.clone()
        } else {
            let mut buf = String::new();
            for line in &req.context {
                buf.push_str(line);
                buf.push('\n');
            }
            buf.push_str(&req.prompt);
            buf
        };

        let vil_request =
            vil_llm::LlmRequest::new(vec![vil_llm::Message::user(prompt_text.clone())]);
        // D7C hardening — record the provider that ACTUALLY
        // satisfied the request, not the default provider id.
        // Under fallback, default may have errored and a later
        // entry in the chain succeeded; transcript / activity
        // logs must reflect that for honest observability.
        let (provider, vil_response) = self
            .router
            .complete_with_provider(&vil_request)
            .await
            .map_err(|e| EngineError::Other(format!("vil_llm router error: {e}")))?;

        // D7D — tool-use round-tripping. Translate every
        // `vil_llm::ToolCall` into the engine's
        // `ToolCallRequest`. `reason` and `estimated_tokens`
        // are not carried by `vil_llm::ToolCall` today; they
        // are filled with conservative defaults (None / 0) so
        // the engine's PolicyGate / ToolDispatcher seam treats
        // each request as a generic call. When a host has not
        // attached a dispatcher to `CompactConfig`, the engine
        // already routes such calls through
        // `UnsupportedDispatcher`, which writes an error
        // `ToolResult` event without panicking — the host
        // remains in control of when tool dispatch is live.
        let tool_calls = vil_response
            .tool_calls
            .into_iter()
            .map(|c| ToolCallRequest {
                id: c.id,
                name: c.name,
                arguments: c.arguments,
                reason: None,
                estimated_tokens: 0,
            })
            .collect();

        Ok(LlmResponse {
            provider,
            model: vil_response.model,
            content: vil_response.content,
            input_tokens: vil_response.usage.prompt_tokens,
            output_tokens: vil_response.usage.completion_tokens,
            tool_calls,
        })
    }
}
