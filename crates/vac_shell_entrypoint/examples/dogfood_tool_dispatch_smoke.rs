//! D8 hardening — self-demonstrating live-dispatch smoke run.
//!
//! Unlike `dogfood_tool_dispatch.rs`, this example does NOT
//! launch the TUI. It drives one `vac_session_engine::submit_one`
//! call with a tool-emitting `LlmAdapter`, a real
//! `VacToolDispatcher` over a `ToolRegistry` containing a
//! `GlobTool`, and an empty `CompositeGate`. After the submit
//! finishes it prints the transcript path and a one-line
//! summary of every paired (call, result) view via
//! `read_tool_use_rows`.
//!
//! Run with:
//!
//! ```bash
//! cargo run -p vac_shell_entrypoint --example dogfood_tool_dispatch_smoke
//! ```
//!
//! The default `dogfood` example stays inert. This file exists
//! so an operator (or a CI smoke job) can produce a transcript
//! that demonstrably contains a `tool_result.kind = ok` from
//! the real `VacToolDispatcher` path.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use async_trait::async_trait;
use vac_session_engine::{
    CompactConfig, CompositeGate, EngineResult, LlmAdapter, LlmRequest, LlmResponse,
    SlashProcessor, SubmitContext, ToolCallRequest, TranscriptWriter, TrivialCompactBoundary,
    UsageTracker, read_tool_use_rows, submit_one,
};
use vac_shell_host_vac_tool_dispatcher::VacToolDispatcher;
use vac_tools::ToolRegistry;
use vac_tools::registry::ToolContext;

struct GlobEmittingLlm;

#[async_trait]
impl LlmAdapter for GlobEmittingLlm {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "smoke".into(),
            model: "smoke-1".into(),
            content: "calling glob".into(),
            input_tokens: 1,
            output_tokens: 1,
            tool_calls: vec![ToolCallRequest {
                id: "smoke-glob-1".into(),
                name: "glob".into(),
                arguments: serde_json::json!({"pattern": "Cargo.toml"}),
                reason: None,
                estimated_tokens: 0,
            }],
        })
    }
}

fn main() -> ExitCode {
    let root = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("smoke: cannot resolve cwd: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = run(root) {
        eprintln!("smoke: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run(root: PathBuf) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    rt.block_on(async {
        let registry = ToolRegistry::new();
        registry
            .register(vac_tools::builtin::glob::GlobTool)
            .await
            .map_err(|e| format!("register glob: {e}"))?;
        let registry = Arc::new(registry);
        let ctx = Arc::new(ToolContext::new(root.clone()));
        let dispatcher = Arc::new(VacToolDispatcher::new(registry, ctx));
        let gate = Arc::new(CompositeGate::new());

        let writer = TranscriptWriter::new(root.clone());
        let slash = SlashProcessor::new();
        let compact = TrivialCompactBoundary::default();
        let usage = UsageTracker::new();
        let llm = GlobEmittingLlm;
        let session_id = uuid::Uuid::new_v4();
        let submit_ctx = SubmitContext::new(session_id, "live dispatch smoke");
        let mut compact_cfg = CompactConfig::default();
        compact_cfg.dispatcher = Some(dispatcher);
        compact_cfg.gate = Some(gate);
        submit_one(
            submit_ctx,
            &writer,
            &slash,
            &compact,
            &usage,
            &llm,
            compact_cfg,
            None,
        )
        .await
        .map_err(|e| format!("submit_one: {e}"))?;

        let transcript = root
            .join(".vac")
            .join("sessions")
            .join(format!("{session_id}.jsonl"));
        let views =
            read_tool_use_rows(&transcript).map_err(|e| format!("replay: {e}"))?;
        println!("smoke transcript: {}", transcript.display());
        for v in &views {
            let summary = v
                .result
                .as_ref()
                .map(|env| format!("{:?}: {}", env.kind, env.summary))
                .unwrap_or_else(|| "<no result>".into());
            println!("  - {} {} -> {}", v.id, v.name, summary);
        }
        if views.iter().any(|v| {
            v.result
                .as_ref()
                .map(|e| matches!(e.kind, vac_tool_core::ToolResultKind::Ok))
                .unwrap_or(false)
        }) {
            println!("smoke: live dispatch produced at least one ok tool_result");
        } else {
            return Err("smoke: no ok tool_result observed".into());
        }
        Ok(())
    })
}
