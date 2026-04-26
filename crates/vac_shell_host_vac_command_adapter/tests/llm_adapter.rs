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
// 4. vil_llm router bridge constructs cleanly + plugs into config.
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
