//! `vac session-run "<input>"` — drive a single submit through
//! [`vac_session_engine::submit_one`].
//!
//! **R0.d status:** this subcommand is retained as the mock-only
//! entry point (EchoAdapter). For real runs wiring VacEngine, use
//! `vac run --engine session`. Once R0.c migrates the TUI and every
//! integration test passes on the session engine, `session-run` will
//! become an alias for `vac run --engine session --provider mock`.
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

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EchoAdapter, SlashProcessor, SubmitContext, SubmitEvent,
    TranscriptWriter, TrivialCompactBoundary, UsageTracker, submit_one,
};

/// F8.2 / H3 — formalized submit metadata that session-run records
/// into the `Accepted` transcript row. Replay harnesses + eval tools
/// read this back via `serde_json::from_value`. Adding a field here
/// is forward-compatible because `#[serde(default)]` is applied on
/// every reader path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubmitMetadata {
    /// Isolation request recorded at submit time. `None` means
    /// "run on host".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<IsolationMeta>,
    /// Whether the caller asked for the transcript JSONL to be kept.
    /// Mirrors `--no-trajectory`.
    #[serde(default)]
    pub trajectory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolationMeta {
    /// Docker image name/tag the caller requested.
    pub docker: String,
}

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
        println!(
            "🐳 isolation requested: docker image '{image}' \
             (preview — image is recorded in submit metadata; \
             tool execution still runs on host)"
        );
        // H2 — trajectory-off + docker is incoherent: the replay
        // contract (isolation.docker) would land in a transcript
        // nobody keeps. Refuse rather than silently swallow.
        if !opts.trajectory {
            anyhow::bail!(
                "--docker requires trajectory enabled; drop \
                 --no-trajectory or omit --docker",
            );
        }
    }

    // F8.3 / C1 — trajectory-first default. When disabled, the
    // transcript still gets written (submit_one always persists the
    // Accepted row for its durability contract) but we route it to a
    // tempdir that is dropped at end of call, so no file lives under
    // `<root>/.vac/sessions/` after the run. The tempdir handle is
    // kept alive for the duration of `execute` so tokio-side writers
    // don't race with cleanup.
    let ephemeral_dir: Option<tempfile::TempDir> = if !opts.trajectory {
        Some(tempfile::tempdir()?)
    } else {
        None
    };
    let writer = if let Some(dir) = ephemeral_dir.as_ref() {
        println!("📓 trajectory disabled (--no-trajectory); transcript ephemeral");
        TranscriptWriter::new(dir.path().to_path_buf())
    } else {
        let w = TranscriptWriter::new(project_root.clone());
        println!("📓 trajectory enabled → {}", w.sessions_dir().display());
        w
    };

    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let llm = match opts.provider {
        ProviderKind::Mock => EchoAdapter,
    };

    // H3 — metadata shape is now a formalized struct instead of an
    // inline JSON map, so replay harnesses can `from_value::<SubmitMetadata>`
    // and break on schema drift rather than silently mis-parse.
    let metadata = SubmitMetadata {
        isolation: opts.docker_image.clone().map(|docker| IsolationMeta { docker }),
        trajectory: opts.trajectory,
    };
    let metadata_value = serde_json::to_value(&metadata)
        .map_err(|e| anyhow::anyhow!("metadata serialize: {e}"))?;
    let ctx = SubmitContext::new(Uuid::new_v4(), opts.input).with_metadata(metadata_value);
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
    // Keep the tempdir alive until everything above has run, then
    // drop it explicitly so the transcript file is unlinked before
    // we return control. Explicit drop beats "goes out of scope"
    // here because it makes the intent visible to readers.
    drop(ephemeral_dir);
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

    #[test]
    fn submit_metadata_roundtrips_with_and_without_isolation() {
        let m1 = SubmitMetadata {
            isolation: Some(IsolationMeta {
                docker: "alpine:3.19".into(),
            }),
            trajectory: true,
        };
        let v = serde_json::to_value(&m1).unwrap();
        assert_eq!(v["isolation"]["docker"], "alpine:3.19");
        assert_eq!(v["trajectory"], true);
        let back: SubmitMetadata = serde_json::from_value(v).unwrap();
        assert_eq!(
            back.isolation.as_ref().map(|i| i.docker.as_str()),
            Some("alpine:3.19"),
        );

        let m2 = SubmitMetadata {
            isolation: None,
            trajectory: false,
        };
        let v2 = serde_json::to_value(&m2).unwrap();
        // Skip-if-none keeps the field absent, not serialized as null.
        assert!(v2.get("isolation").is_none(), "{v2}");
        let back2: SubmitMetadata = serde_json::from_value(v2).unwrap();
        assert!(back2.isolation.is_none());
        assert!(!back2.trajectory);
    }

    /// C1 regression — `--no-trajectory` must keep the real
    /// `<root>/.vac/sessions/` tree clean even though `submit_one`
    /// always writes a transcript for its durability contract.
    #[tokio::test]
    async fn no_trajectory_does_not_leave_transcript_in_project_root() {
        let project = tempfile::tempdir().unwrap();
        let opts = SessionRunOptions {
            input: "hello".into(),
            provider: ProviderKind::Mock,
            trajectory: false,
            docker_image: None,
        };
        execute(project.path().to_path_buf(), opts).await.unwrap();
        let sessions_dir = project.path().join(".vac").join("sessions");
        if sessions_dir.exists() {
            let mut entries = tokio::fs::read_dir(&sessions_dir).await.unwrap();
            let mut found = false;
            while let Ok(Some(_)) = entries.next_entry().await {
                found = true;
            }
            assert!(
                !found,
                "no-trajectory run must not persist under {}",
                sessions_dir.display(),
            );
        }
    }

    /// Positive path: trajectory ON writes a transcript into the
    /// project's sessions dir.
    #[tokio::test]
    async fn trajectory_on_writes_transcript() {
        let project = tempfile::tempdir().unwrap();
        let opts = SessionRunOptions {
            input: "write me".into(),
            provider: ProviderKind::Mock,
            trajectory: true,
            docker_image: None,
        };
        execute(project.path().to_path_buf(), opts).await.unwrap();
        let sessions_dir = project.path().join(".vac").join("sessions");
        let mut entries = tokio::fs::read_dir(&sessions_dir).await.unwrap();
        let mut found = 0;
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s == "jsonl")
            {
                found += 1;
            }
        }
        assert_eq!(found, 1, "trajectory ON must persist exactly one transcript");
    }

    /// H2 — `--no-trajectory` + `--docker` is refused at entry.
    #[tokio::test]
    async fn no_trajectory_with_docker_is_rejected() {
        let project = tempfile::tempdir().unwrap();
        let opts = SessionRunOptions {
            input: "x".into(),
            provider: ProviderKind::Mock,
            trajectory: false,
            docker_image: Some("alpine:3.19".into()),
        };
        let err = execute(project.path().to_path_buf(), opts).await.unwrap_err();
        assert!(format!("{err}").contains("trajectory"));
    }

    /// Empty docker image passed through main.rs without clap
    /// validation must still be rejected by `execute`.
    #[tokio::test]
    async fn empty_docker_image_is_rejected() {
        let project = tempfile::tempdir().unwrap();
        let opts = SessionRunOptions {
            input: "x".into(),
            provider: ProviderKind::Mock,
            trajectory: true,
            docker_image: Some("   ".into()),
        };
        let err = execute(project.path().to_path_buf(), opts).await.unwrap_err();
        assert!(format!("{err}").contains("docker"));
    }
}
