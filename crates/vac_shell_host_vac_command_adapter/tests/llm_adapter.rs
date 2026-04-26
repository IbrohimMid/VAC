//! D7C — provider-backed LLM adapter contract tests.
//!
//! These tests prove that:
//!
//! * `AdapterLlm::Echo` is the default and matches D7B
//!   behaviour byte-for-byte (transcript still lands).
//! * A `Custom` adapter is reached by `submit_one`, sees the
//!   exact prompt the host mapped, and its response shows up
//!   in the transcript as the assistant turn.
//! * A `Custom` adapter that returns `EngineError` surfaces as
//!   `ShellCommandError::Failed` at the trait surface, with
//!   the error string preserved for the operator-visible
//!   activity log.
//!
//! Real `vil_llm` provider routing is **not** exercised here —
//! that requires CI credentials and is handled in a separate
//! provider-smoke matrix. What we DO pin: the
//! `VilLlmRouterAdapter` bridge constructs cleanly from a
//! router and threads through the same path.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use vac_session_engine::{EngineError, LlmAdapter, LlmRequest, LlmResponse};
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec};
use vac_shell_host_commands::{ShellCommandError, ShellCommandExecutor};
use vac_shell_host_vac_command_adapter::{
    AdapterCommandSpec, AdapterConfig, AdapterLlm, VacCommandExecutorAdapter,
    VilLlmRouterAdapter,
};

fn cmd(id: &str, slash: &str) -> ShellCommandSpec {
    ShellCommandSpec {
        id: id.to_string(),
        slash: slash.to_string(),
        title: format!("{slash} title"),
        description: format!("{slash} description"),
        kind: ShellCommandKind::PromptTemplate,
        palette_visible: true,
        shortcut: None,
        category: None,
        aliases: vec![],
        keywords: vec![],
        disabled_reason: None,
    }
}

// ---------------------------------------------------------------------
// Stub LlmAdapter — captures the prompt + emits a fixed response.
// ---------------------------------------------------------------------

struct CapturingAdapter {
    seen_prompt: Arc<std::sync::Mutex<Vec<String>>>,
    response_content: String,
    calls: Arc<AtomicUsize>,
}

impl CapturingAdapter {
    fn new(content: &str) -> Self {
        Self {
            seen_prompt: Arc::new(std::sync::Mutex::new(Vec::new())),
            response_content: content.to_string(),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn seen(&self) -> Vec<String> {
        self.seen_prompt.lock().unwrap().clone()
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl LlmAdapter for CapturingAdapter {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, EngineError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen_prompt.lock().unwrap().push(req.prompt.clone());
        Ok(LlmResponse {
            provider: "test-provider".into(),
            model: "test-model".into(),
            content: self.response_content.clone(),
            input_tokens: 1,
            output_tokens: 1,
            tool_calls: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------
// Stub LlmAdapter — always fails. Verifies operator-visible error
// surface for the Custom path.
// ---------------------------------------------------------------------

struct FailingAdapter;

#[async_trait]
impl LlmAdapter for FailingAdapter {
    async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, EngineError> {
        Err(EngineError::Other(
            "synthetic provider failure for D7C test".into(),
        ))
    }
}

fn mapped(root: std::path::PathBuf) -> AdapterConfig {
    AdapterConfig::new(root).with_command(AdapterCommandSpec::new(
        "memorize",
        "/memorize",
        "MEMORIZE-PROMPT-D7C",
    ))
}

// ---------------------------------------------------------------------
// 1. Default config keeps EchoAdapter (D7B behaviour preserved).
// ---------------------------------------------------------------------

#[test]
fn default_adapter_llm_is_echo() {
    let cfg = AdapterConfig::new(std::path::PathBuf::from("/tmp"));
    assert!(matches!(cfg.llm, AdapterLlm::Echo));
}

#[test]
fn echo_path_still_writes_transcript_after_d7c_refactor() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(mapped(tmp.path().to_path_buf()));
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();
    let path = adapter.last_transcript().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    // EchoAdapter prefixes its echo with "echo:" so the
    // transcript must contain that substring somewhere.
    assert!(
        body.contains("echo:"),
        "EchoAdapter response missing from transcript: {body}"
    );
}

// ---------------------------------------------------------------------
// 2. Custom adapter is invoked with the configured prompt.
// ---------------------------------------------------------------------

#[test]
fn custom_adapter_receives_configured_prompt() {
    let capturer = Arc::new(CapturingAdapter::new("D7C-CUSTOM-RESPONSE"));
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(
        mapped(tmp.path().to_path_buf()).with_llm(capturer.clone()),
    );
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();

    assert_eq!(capturer.calls(), 1);
    let prompts = capturer.seen();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0], "MEMORIZE-PROMPT-D7C");
}

#[test]
fn custom_adapter_response_lands_in_transcript() {
    let capturer = Arc::new(CapturingAdapter::new("D7C-RESPONSE-NEEDLE"));
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(
        mapped(tmp.path().to_path_buf()).with_llm(capturer),
    );
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();

    let path = adapter.last_transcript().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("D7C-RESPONSE-NEEDLE"),
        "custom adapter content missing from transcript: {body}"
    );
}

// ---------------------------------------------------------------------
// 3. Custom adapter failure surfaces as ShellCommandError::Failed.
// ---------------------------------------------------------------------

#[test]
fn custom_adapter_engine_error_surfaces_as_shell_command_failed() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(
        mapped(tmp.path().to_path_buf()).with_llm(Arc::new(FailingAdapter)),
    );
    let err = adapter
        .execute(&cmd("memorize", "/memorize"))
        .expect_err("failing LlmAdapter must produce ShellCommandError");
    match err {
        ShellCommandError::Failed(msg) => {
            assert!(
                msg.contains("synthetic provider failure for D7C test"),
                "operator-visible error must preserve adapter error: {msg}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(adapter.last_transcript().is_none());
}

// ---------------------------------------------------------------------
// 4. vil_llm router bridge — fake providers prove the actual provider
//    that satisfied the request lands in the engine response and
//    transcript, even under fallback. No live credentials required.
// ---------------------------------------------------------------------

struct VilOk {
    name: &'static str,
    model: &'static str,
}

#[async_trait]
impl vil_llm::LlmProvider for VilOk {
    fn name(&self) -> &str {
        self.name
    }
    async fn complete(
        &self,
        _req: &vil_llm::LlmRequest,
    ) -> vil_llm::error::LlmResult<vil_llm::LlmResponse> {
        Ok(vil_llm::LlmResponse {
            content: format!("d7c-{}-needle", self.name),
            model: self.model.to_string(),
            finish_reason: vil_llm::provider::FinishReason::Stop,
            usage: vil_llm::provider::TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
                ..Default::default()
            },
            tool_calls: vec![],
        })
    }
    async fn stream(
        &self,
        _req: &vil_llm::LlmRequest,
    ) -> vil_llm::error::LlmResult<tokio::sync::mpsc::Receiver<vil_llm::provider::StreamChunk>> {
        unimplemented!()
    }
}

struct VilFail {
    name: &'static str,
}

#[async_trait]
impl vil_llm::LlmProvider for VilFail {
    fn name(&self) -> &str {
        self.name
    }
    async fn complete(
        &self,
        _req: &vil_llm::LlmRequest,
    ) -> vil_llm::error::LlmResult<vil_llm::LlmResponse> {
        Err(vil_llm::LlmError::Provider {
            provider: self.name.to_string(),
            status: None,
            message: "synthetic fail".into(),
        })
    }
    async fn stream(
        &self,
        _req: &vil_llm::LlmRequest,
    ) -> vil_llm::error::LlmResult<tokio::sync::mpsc::Receiver<vil_llm::provider::StreamChunk>> {
        unimplemented!()
    }
}

#[test]
fn vil_llm_router_bridge_records_actual_provider_in_transcript() {
    // Default-only success path: provider == default == "good".
    let mut router = vil_llm::LlmRouter::new("good", 1_000);
    router.add_provider_named(
        "good",
        Arc::new(VilOk {
            name: "good",
            model: "good-model",
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(
        mapped(tmp.path().to_path_buf()).with_vil_llm_router(router),
    );
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();

    let path = adapter.last_transcript().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("\"provider\":\"good\""), "transcript: {body}");
    assert!(body.contains("\"model\":\"good-model\""), "transcript: {body}");
    assert!(body.contains("d7c-good-needle"), "transcript: {body}");
}

#[test]
fn vil_llm_router_bridge_records_actual_fallback_provider_in_transcript() {
    // Default `bad` always fails; fallback `good` satisfies the
    // request. The transcript provider must be `good`, not `bad`.
    let mut router = vil_llm::LlmRouter::new("bad", 1_000);
    router.set_fallback_chain(vec!["bad".into(), "good".into()]);
    router.add_provider_named("bad", Arc::new(VilFail { name: "bad" }));
    router.add_provider_named(
        "good",
        Arc::new(VilOk {
            name: "good",
            model: "good-model",
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(
        mapped(tmp.path().to_path_buf()).with_vil_llm_router(router),
    );
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();

    let path = adapter.last_transcript().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("\"provider\":\"good\""),
        "fallback provider must land in transcript, not the configured default: {body}"
    );
    assert!(
        !body.contains("\"provider\":\"bad\""),
        "configured-default `bad` must NOT be recorded as the satisfying provider: {body}"
    );
}

#[test]
fn vil_llm_router_bridge_propagates_failure_when_all_providers_fail() {
    let mut router = vil_llm::LlmRouter::new("bad", 1_000);
    router.set_fallback_chain(vec!["bad".into()]);
    router.add_provider_named("bad", Arc::new(VilFail { name: "bad" }));

    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(
        mapped(tmp.path().to_path_buf()).with_vil_llm_router(router),
    );
    let err = adapter.execute(&cmd("memorize", "/memorize")).unwrap_err();
    match err {
        ShellCommandError::Failed(msg) => {
            assert!(
                msg.contains("vil_llm router error"),
                "operator-visible error must mention the bridge: {msg}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(adapter.last_transcript().is_none());
}

// ---------------------------------------------------------------------
// 4c. D7D — tool-use round-tripping. vil_llm::ToolCall blocks survive
//     translation into vac_session_engine::ToolCallRequest, and the
//     engine's no-dispatcher path remains safe (writes error envelope
//     via UnsupportedDispatcher, never panics).
// ---------------------------------------------------------------------

struct VilOkWithTools {
    name: &'static str,
    model: &'static str,
    tool_calls: Vec<vil_llm::provider::ToolCall>,
}

#[async_trait]
impl vil_llm::LlmProvider for VilOkWithTools {
    fn name(&self) -> &str {
        self.name
    }
    async fn complete(
        &self,
        _req: &vil_llm::LlmRequest,
    ) -> vil_llm::error::LlmResult<vil_llm::LlmResponse> {
        Ok(vil_llm::LlmResponse {
            content: format!("from-{}", self.name),
            model: self.model.to_string(),
            finish_reason: vil_llm::provider::FinishReason::ToolUse,
            usage: vil_llm::provider::TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
                ..Default::default()
            },
            tool_calls: self.tool_calls.clone(),
        })
    }
    async fn stream(
        &self,
        _req: &vil_llm::LlmRequest,
    ) -> vil_llm::error::LlmResult<tokio::sync::mpsc::Receiver<vil_llm::provider::StreamChunk>>
    {
        unimplemented!()
    }
}

fn fake_tool_call(id: &str, name: &str, args: serde_json::Value) -> vil_llm::provider::ToolCall {
    vil_llm::provider::ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments: args,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vil_llm_router_bridge_translates_tool_calls_into_engine_response() {
    // Drive the bridge directly (no submit_one round-trip) so we
    // can assert the translated `LlmResponse.tool_calls` exactly.
    let mut router = vil_llm::LlmRouter::new("good", 1_000);
    router.add_provider_named(
        "good",
        Arc::new(VilOkWithTools {
            name: "good",
            model: "good-model",
            tool_calls: vec![
                fake_tool_call("t1", "search", serde_json::json!({"query": "vil"})),
                fake_tool_call("t2", "read", serde_json::json!({"path": "Cargo.toml"})),
            ],
        }),
    );
    let bridge = VilLlmRouterAdapter::new(router);
    let resp = bridge
        .complete(LlmRequest {
            prompt: "hi".into(),
            context: vec![],
        })
        .await
        .unwrap();

    assert_eq!(resp.tool_calls.len(), 2);
    assert_eq!(resp.tool_calls[0].id, "t1");
    assert_eq!(resp.tool_calls[0].name, "search");
    assert_eq!(
        resp.tool_calls[0].arguments,
        serde_json::json!({"query": "vil"})
    );
    assert!(resp.tool_calls[0].reason.is_none());
    assert_eq!(resp.tool_calls[0].estimated_tokens, 0);

    assert_eq!(resp.tool_calls[1].id, "t2");
    assert_eq!(resp.tool_calls[1].name, "read");
    assert_eq!(resp.provider, "good");
    assert_eq!(resp.model, "good-model");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vil_llm_router_bridge_emits_empty_tool_calls_when_provider_emits_none() {
    let mut router = vil_llm::LlmRouter::new("good", 1_000);
    router.add_provider_named(
        "good",
        Arc::new(VilOk {
            name: "good",
            model: "good-model",
        }),
    );
    let bridge = VilLlmRouterAdapter::new(router);
    let resp = bridge
        .complete(LlmRequest {
            prompt: "hi".into(),
            context: vec![],
        })
        .await
        .unwrap();
    assert!(resp.tool_calls.is_empty());
}

/// Custom `LlmAdapter` that returns a synthetic tool call,
/// proving the engine's no-dispatcher path stays safe (writes
/// an error envelope via `UnsupportedDispatcher` and never
/// panics) when D7D's translation produces a non-empty
/// `tool_calls` vector.
struct ToolEmittingAdapter;

#[async_trait]
impl LlmAdapter for ToolEmittingAdapter {
    async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, EngineError> {
        Ok(LlmResponse {
            provider: "tool-stub".into(),
            model: "tool-stub-1".into(),
            content: "I want a tool".into(),
            input_tokens: 1,
            output_tokens: 1,
            tool_calls: vec![vac_session_engine::ToolCallRequest {
                id: "call-1".into(),
                name: "search".into(),
                arguments: serde_json::json!({"q": "needle"}),
                reason: None,
                estimated_tokens: 0,
            }],
        })
    }
}

#[test]
fn engine_remains_safe_when_tool_calls_arrive_without_a_dispatcher() {
    // The host has not attached a `ToolDispatcher` (CompactConfig
    // default), so each tool call should fall through to
    // `UnsupportedDispatcher` and produce a `ToolResult` error
    // envelope without aborting the submit. Verifies the engine
    // contract D7D relies on for safety.
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(
        mapped(tmp.path().to_path_buf()).with_llm(Arc::new(ToolEmittingAdapter)),
    );
    adapter
        .execute(&cmd("memorize", "/memorize"))
        .expect("non-empty tool_calls without a dispatcher must not abort the submit");
    let path = adapter.last_transcript().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    // The Finished row is the engine's "submit completed" marker.
    assert!(
        body.contains("\"kind\":\"finished\"")
            || body.contains("\"kind\": \"finished\"")
            || body.contains("\"finished\""),
        "Finished transcript row must be present after tool-call no-dispatcher path: {body}"
    );
}

// ---------------------------------------------------------------------
// 4d. D7D-HARDENING — drive submit_one DIRECTLY with the bridge so we
//     can subscribe to the event channel and prove the engine's
//     dispatch/event loop receives the translated tool calls and
//     produces an UnsupportedDispatcher error envelope when no
//     dispatcher is attached. The adapter's `execute` path passes
//     `None` for events; these tests bypass it on purpose.
// ---------------------------------------------------------------------

async fn drain_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<vac_session_engine::SubmitEvent>,
) -> Vec<vac_session_engine::SubmitEvent> {
    let mut out = Vec::new();
    while let Some(ev) = rx.recv().await {
        out.push(ev);
    }
    out
}

async fn run_submit_with_bridge(
    project_root: std::path::PathBuf,
    router: vil_llm::LlmRouter,
) -> Vec<vac_session_engine::SubmitEvent> {
    use vac_session_engine::{
        CompactConfig, SlashProcessor, SubmitContext, TranscriptWriter,
        TrivialCompactBoundary, UsageTracker, submit_one,
    };
    let bridge = VilLlmRouterAdapter::new(router);
    let writer = TranscriptWriter::new(project_root);
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let session_id = uuid::Uuid::new_v4();
    let ctx = SubmitContext::new(session_id, "drive D7D bridge");
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    submit_one(
        ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &bridge,
        CompactConfig::default(),
        Some(tx),
    )
    .await
    .expect("submit_one must succeed even when tool calls fall through to UnsupportedDispatcher");
    drain_events(rx).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn submit_one_emits_tool_requested_then_tool_result_when_no_dispatcher_attached() {
    use vac_session_engine::SubmitEvent;
    let mut router = vil_llm::LlmRouter::new("good", 1_000);
    router.add_provider_named(
        "good",
        Arc::new(VilOkWithTools {
            name: "good",
            model: "good-model",
            tool_calls: vec![fake_tool_call(
                "t-only",
                "search",
                serde_json::json!({"q": "needle"}),
            )],
        }),
    );
    let tmp = tempfile::tempdir().unwrap();
    let events = run_submit_with_bridge(tmp.path().to_path_buf(), router).await;

    // Find the ToolRequested with the exact id/name/arguments.
    let req_idx = events
        .iter()
        .position(|e| {
            matches!(
                e,
                SubmitEvent::ToolRequested { id, name, arguments }
                    if id == "t-only" && name == "search" && arguments == &serde_json::json!({"q": "needle"})
            )
        })
        .expect("ToolRequested with translated id/name/arguments must appear");

    // ToolResult must follow with the same id/name and an error
    // envelope mentioning the no-dispatcher condition.
    let res_idx = events
        .iter()
        .enumerate()
        .skip(req_idx + 1)
        .find_map(|(i, e)| match e {
            SubmitEvent::ToolResult { id, name, payload }
                if id == "t-only" && name == "search" =>
            {
                let dump = serde_json::to_string(payload).unwrap();
                assert!(
                    dump.contains("no ToolDispatcher")
                        || dump.contains("cannot run tool")
                        || dump.contains("dispatch error"),
                    "ToolResult envelope must mention the no-dispatcher / dispatch-error path: {dump}"
                );
                assert!(
                    dump.contains("\"kind\":\"error\""),
                    "ToolResult envelope kind must be `error`: {dump}"
                );
                Some(i)
            }
            _ => None,
        })
        .expect("ToolResult after ToolRequested must appear");

    // Finished must come after ToolResult.
    let fin_idx = events
        .iter()
        .position(|e| matches!(e, SubmitEvent::Finished { .. }))
        .expect("Finished event must appear");
    assert!(fin_idx > res_idx, "Finished must follow ToolResult");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn submit_one_preserves_order_for_multiple_tool_calls() {
    use vac_session_engine::SubmitEvent;
    let mut router = vil_llm::LlmRouter::new("good", 1_000);
    router.add_provider_named(
        "good",
        Arc::new(VilOkWithTools {
            name: "good",
            model: "good-model",
            tool_calls: vec![
                fake_tool_call("a", "alpha", serde_json::json!({"i": 1})),
                fake_tool_call("b", "beta", serde_json::json!({"i": 2})),
                fake_tool_call("c", "gamma", serde_json::json!({"i": 3})),
            ],
        }),
    );
    let tmp = tempfile::tempdir().unwrap();
    let events = run_submit_with_bridge(tmp.path().to_path_buf(), router).await;

    let requested_ids: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            SubmitEvent::ToolRequested { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(requested_ids, vec!["a", "b", "c"]);

    let result_ids: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            SubmitEvent::ToolResult { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(result_ids, vec!["a", "b", "c"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn submit_one_preserves_tool_calls_under_provider_fallback() {
    use vac_session_engine::SubmitEvent;
    let mut router = vil_llm::LlmRouter::new("bad", 1_000);
    router.set_fallback_chain(vec!["bad".into(), "good".into()]);
    router.add_provider_named("bad", Arc::new(VilFail { name: "bad" }));
    router.add_provider_named(
        "good",
        Arc::new(VilOkWithTools {
            name: "good",
            model: "good-model",
            tool_calls: vec![fake_tool_call(
                "fallback-1",
                "lookup",
                serde_json::json!({"k": "v"}),
            )],
        }),
    );
    let tmp = tempfile::tempdir().unwrap();
    let events = run_submit_with_bridge(tmp.path().to_path_buf(), router).await;

    // LlmRequested or transcript provider should be `good`. The
    // event uses the request-time provider id, so we check the
    // transcript file for the satisfying provider directly.
    let session_files: Vec<_> = std::fs::read_dir(tmp.path().join(".vac").join("sessions"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(session_files.len(), 1);
    let body = std::fs::read_to_string(session_files[0].path()).unwrap();
    assert!(
        body.contains("\"provider\":\"good\""),
        "fallback provider must land in transcript: {body}"
    );

    // Translated tool call must appear in the event stream.
    let saw_tool = events.iter().any(|e| {
        matches!(
            e,
            SubmitEvent::ToolRequested { id, name, .. }
                if id == "fallback-1" && name == "lookup"
        )
    });
    assert!(
        saw_tool,
        "translated tool call must survive provider fallback"
    );
}

// ---------------------------------------------------------------------
// 4e. Strengthened mapping coverage — the original test only asserted
// against the first translated call. Pin the second call's defaults too.
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vil_llm_bridge_second_tool_call_keeps_conservative_defaults() {
    let mut router = vil_llm::LlmRouter::new("good", 1_000);
    router.add_provider_named(
        "good",
        Arc::new(VilOkWithTools {
            name: "good",
            model: "good-model",
            tool_calls: vec![
                fake_tool_call("first", "alpha", serde_json::json!({"k": 1})),
                fake_tool_call("second", "beta", serde_json::json!({"k": 2})),
            ],
        }),
    );
    let bridge = VilLlmRouterAdapter::new(router);
    let resp = bridge
        .complete(LlmRequest {
            prompt: "hi".into(),
            context: vec![],
        })
        .await
        .unwrap();

    assert_eq!(resp.tool_calls.len(), 2);
    let second = &resp.tool_calls[1];
    assert_eq!(second.id, "second");
    assert_eq!(second.name, "beta");
    assert_eq!(second.arguments, serde_json::json!({"k": 2}));
    assert!(second.reason.is_none());
    assert_eq!(second.estimated_tokens, 0);
}

// ---------------------------------------------------------------------
// 4b. Construction-only test (kept from D7C v1) — the bridge plugs in
//     even when the router has no providers attached.
// ---------------------------------------------------------------------

#[test]
fn vil_llm_router_bridge_constructs_and_plugs_in() {
    // Build a minimal LlmRouter with no providers attached. We
    // do NOT execute it (no CI credentials); we only verify the
    // bridge constructs and the AdapterConfig accepts it via
    // the convenience helper.
    let router = vil_llm::LlmRouter::new("anthropic", 0);
    let bridge = VilLlmRouterAdapter::new(router);
    let dbg = format!("{bridge:?}");
    assert!(dbg.contains("anthropic"));

    let router2 = vil_llm::LlmRouter::new("anthropic", 0);
    let cfg = AdapterConfig::new(std::path::PathBuf::from("/tmp"))
        .with_vil_llm_router(router2);
    assert!(matches!(cfg.llm, AdapterLlm::Custom(_)));
}

// ---------------------------------------------------------------------
// 5. AdapterLlm Debug is non-leaky.
// ---------------------------------------------------------------------

#[test]
fn adapter_llm_debug_does_not_leak_inner_handle() {
    let dbg_echo = format!("{:?}", AdapterLlm::Echo);
    assert_eq!(dbg_echo, "AdapterLlm::Echo");

    let dbg_custom = format!(
        "{:?}",
        AdapterLlm::Custom(Arc::new(FailingAdapter) as Arc<dyn LlmAdapter>)
    );
    assert!(dbg_custom.contains("Custom"));
    assert!(!dbg_custom.contains("FailingAdapter"));
}
