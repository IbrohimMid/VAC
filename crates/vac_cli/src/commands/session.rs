//! `vac session-run "<input>"` — drive a single submit through
//! [`vac_session_engine::submit_one`].
//!
//! Trae-style one-shot runner. Defaults:
//!
//! - **Adapter**: `EchoAdapter` (deterministic, no provider calls).
//!   Pass `--provider mock` explicitly or leave unset for the same.
//!   Real providers will plug in as additional `--provider` values
//!   once the adapter registry lands.
//! - **Trajectory**: on by default (F8.3). The submit's transcript
//!   JSONL *is* the trajectory record; `--no-trajectory` disables
//!   writing to `<root>/.vac/sessions/`. This mirrors Trae's
//!   research-first ergonomics — every run is audit-ready.
//! - **Isolation**: host by default. Pass `--docker <image>` (F8.2)
//!   to request the submit be executed inside the named image via
//!   `IsolationManager`. Currently the flag is validated + recorded
//!   in the metadata slot; full wiring into `IsolationManager::
//!   spawn_background` lands when the engine gains a tool-execution
//!   phase for session-run.

use std::path::PathBuf;

use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EchoAdapter, SlashProcessor, SubmitContext, SubmitEvent,
    TranscriptWriter, TrivialCompactBoundary, UsageTracker, submit_one,
};

/// Parsed provider selector. Kept as an enum even though there's only
/// one variant today so the CLI surface is stable when real providers
/// land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderKind {
    /// Deterministic echo adapter — always returns `"echo: <prompt>"`.
    Mock,
}

impl ProviderKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.to_ascii_lowercase().as_str() {
            "mock" | "echo" => Ok(Self::Mock),
            other => Err(format!(
                "unknown provider '{other}'; supported: mock"
            )),
        }
    }
}

/// Options carried from `main.rs` down to `execute`.
#[derive(Debug, Clone)]
pub struct SessionRunOptions {
    pub input: String,
    pub provider: ProviderKind,
    /// When true, the transcript JSONL is written to
    /// `<root>/.vac/sessions/`. Default: true (F8.3).
    pub trajectory: bool,
    /// Docker image to run the submit in, if requested (F8.2). Empty
    /// string means "host".
    pub docker_image: Option<String>,
}

impl Default for SessionRunOptions {
    fn default() -> Self {
        Self {
            input: String::new(),
            provider: ProviderKind::Mock,
            trajectory: true,
            docker_image: None,
        }
    }
}

pub async fn execute(project_root: PathBuf, opts: SessionRunOptions) -> anyhow::Result<()> {
    // F8.2 — `--docker` currently acts as a labelled pass-through:
    // the image is surfaced to the operator and recorded in the
    // submit metadata, but tool execution stays on the host until
    // IsolationManager wiring lands for the session-engine path.
    if let Some(image) = &opts.docker_image {
        if image.trim().is_empty() {
            anyhow::bail!("--docker requires a non-empty image name");
        }
        println!("🐳 isolation requested: docker image '{image}'");
    }

    // F8.3 — trajectory-first default: print the routing so operators
    // see the file they'll later hand to evaluators / replay harness.
    let writer = TranscriptWriter::new(project_root.clone());
    if opts.trajectory {
        println!(
            "📓 trajectory enabled → {}",
            writer.sessions_dir().display(),
        );
    } else {
        println!("📓 trajectory disabled (--no-trajectory)");
    }

    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let llm = match opts.provider {
        ProviderKind::Mock => EchoAdapter,
    };

    let mut ctx = SubmitContext::new(Uuid::new_v4(), opts.input);
    if let Some(image) = &opts.docker_image {
        ctx = ctx.with_metadata(serde_json::json!({
            "isolation": { "docker": image },
            "trajectory": opts.trajectory,
        }));
    } else {
        ctx = ctx.with_metadata(serde_json::json!({
            "trajectory": opts.trajectory,
        }));
    }
    let sid = ctx.session_id;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let render = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev {
                SubmitEvent::Accepted { entry_id } => {
                    println!("[accepted] {entry_id}");
                }
                SubmitEvent::SlashHandled { command, .. } => {
                    println!("[slash] /{command}");
                }
                SubmitEvent::Compacted { kept, dropped } => {
                    println!("[compact] kept={kept} dropped={dropped}");
                }
                SubmitEvent::LlmRequested { provider, model } => {
                    println!("[llm.request] {provider}/{model}");
                }
                SubmitEvent::LlmChunk { text } => {
                    println!("[llm.chunk] {text}");
                }
                SubmitEvent::ToolRequested { name, .. } => {
                    println!("[tool.request] {name}");
                }
                SubmitEvent::ToolResult { name, success, .. } => {
                    println!("[tool.result] {name} ok={success}");
                }
                SubmitEvent::Finished { usage } => {
                    println!(
                        "[finished] in={} out={} tools={}",
                        usage.input_tokens, usage.output_tokens, usage.tool_calls
                    );
                }
                SubmitEvent::Aborted { reason } => {
                    println!("[aborted] {reason}");
                }
                other => {
                    println!("[{}]", other.label());
                }
            }
        }
    });

    let snap = submit_one(
        ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &llm,
        CompactConfig::default(),
        Some(tx),
    )
    .await?;
    if let Err(e) = render.await {
        tracing::warn!(target: "vac_cli::session", "event renderer join error: {e}");
    }

    println!("\nsession: {sid}");
    if opts.trajectory {
        println!(
            "transcript: {}",
            writer.sessions_dir().join(format!("{sid}.jsonl")).display()
        );
    }
    println!("total tokens: {}", snap.total_tokens());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_parse_accepts_aliases() {
        assert_eq!(ProviderKind::parse("mock").unwrap(), ProviderKind::Mock);
        assert_eq!(ProviderKind::parse("Mock").unwrap(), ProviderKind::Mock);
        assert_eq!(ProviderKind::parse("echo").unwrap(), ProviderKind::Mock);
    }

    #[test]
    fn provider_parse_rejects_unknown() {
        let err = ProviderKind::parse("gpt-9000").unwrap_err();
        assert!(err.contains("unknown provider"));
    }

    #[test]
    fn default_options_trajectory_on_provider_mock() {
        let o = SessionRunOptions::default();
        assert!(o.trajectory);
        assert_eq!(o.provider, ProviderKind::Mock);
        assert!(o.docker_image.is_none());
    }
}
