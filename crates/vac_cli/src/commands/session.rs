//! `vac session-run "<input>"` — drive a single submit through
//! [`vac_session_engine::submit_one`] using the `EchoAdapter`. Writes
//! the transcript JSONL under `<root>/.vac/sessions/` so operators can
//! inspect the durability contract without wiring a real LLM.

use std::path::PathBuf;
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EchoAdapter, SlashProcessor, SubmitContext, SubmitEvent,
    TranscriptWriter, TrivialCompactBoundary, UsageTracker, submit_one,
};

pub async fn execute(project_root: PathBuf, input: String) -> anyhow::Result<()> {
    let writer = TranscriptWriter::new(project_root);
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let llm = EchoAdapter;

    let ctx = SubmitContext::new(Uuid::new_v4(), input);
    let sid = ctx.session_id;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let render = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            // Non-exhaustive match: SubmitEvent may gain variants over
            // time. Fall through with the event's label rather than
            // failing to compile.
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

    println!(
        "\nsession: {sid}\ntranscript: {}",
        writer.sessions_dir().join(format!("{sid}.jsonl")).display()
    );
    println!("total tokens: {}", snap.total_tokens());
    Ok(())
}
