//! `vac plan <prompt>` — remote deep planner.
//!
//! P2 from `docs/ultraplan-vac-product.md` thread G.
//!
//! Submits the prompt through `vac_session_engine::submit_one` with a
//! relaxed budget, parses the adapter's reply into a structured plan
//! document, and writes `.vac/plans/<uuid>.md` plus `.vac/plans/<uuid>.json`
//! for the apply path to consume. `vac plan apply <id>` reads back the
//! JSON sidecar and stages each step as a changeset entry.
//!
//! Default adapter is `EchoAdapter` (mock). `--remote <uri>` routes
//! through `plan_remote::RemoteSessionAdapter`, which wraps a
//! `vac_bridge::RemoteSession` and drives a subprocess over stdio.
//! Only `stdio://<bin>` is implemented today — `ws://` / `http://`
//! return an explicit unsupported-transport error so the mock path
//! is never a silent fallback.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EchoAdapter, SlashProcessor, SubmitContext, SubmitEvent, TranscriptWriter,
    TrivialCompactBoundary, UsageTracker, submit_one,
};

/// Structured plan — serialised alongside the markdown render so the
/// `apply` path consumes machine-readable steps rather than re-parsing
/// markdown bullets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanDocument {
    pub id: Uuid,
    pub prompt: String,
    pub remote: Option<String>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub summary: String,
    pub steps: Vec<PlanStep>,
    pub transcript_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: u32,
    pub title: String,
    pub detail: String,
    /// Target file path when the step is a single-file change.
    /// `None` for research/investigation steps.
    pub target: Option<PathBuf>,
}

/// Parse the adapter reply as a structured plan. Real remote planners
/// are expected to reply with JSON matching `{"steps": [{"title": …,
/// "detail": …, "target": …?}, …]}` (or a bare array). When parsing
/// succeeds we use those steps verbatim; otherwise we fall back to
/// the deterministic skeleton in `steps_from_reply`. Returning `None`
/// on parse failure keeps the mock `EchoAdapter` path working.
fn parse_structured_steps(reply_content: &str) -> Option<Vec<PlanStep>> {
    let trimmed = reply_content.trim();
    if trimmed.is_empty() || !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }

    #[derive(Deserialize)]
    struct RawStep {
        title: String,
        detail: Option<String>,
        target: Option<PathBuf>,
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Envelope {
        Wrapped { steps: Vec<RawStep> },
        Bare(Vec<RawStep>),
    }

    let env: Envelope = serde_json::from_str(trimmed).ok()?;
    let raw = match env {
        Envelope::Wrapped { steps } => steps,
        Envelope::Bare(s) => s,
    };
    if raw.is_empty() {
        return None;
    }
    Some(
        raw.into_iter()
            .enumerate()
            .map(|(i, r)| PlanStep {
                id: (i as u32) + 1,
                title: r.title,
                detail: r.detail.unwrap_or_default(),
                target: r.target,
            })
            .collect(),
    )
}

/// Derive a step list from the adapter's reply content. For the
/// mock `EchoAdapter` the reply is `"echo: <prompt>"`; we split it
/// into a deterministic 3-step skeleton (investigate → draft →
/// verify) so the output is a real plan, not a placeholder bullet
/// list. A future real adapter can return structured JSON that
/// bypasses this derivation via `parse_structured_steps`.
fn steps_from_reply(prompt: &str, reply_content: &str) -> Vec<PlanStep> {
    let subject = prompt.trim();
    vec![
        PlanStep {
            id: 1,
            title: format!("Investigate: {}", first_words(subject, 8)),
            detail: format!(
                "Collect relevant files + existing patterns. Adapter echo \
                 for shape validation: `{}`.",
                reply_content.chars().take(80).collect::<String>(),
            ),
            target: None,
        },
        PlanStep {
            id: 2,
            title: format!("Draft: {}", first_words(subject, 8)),
            detail: "Write or modify the target file(s) to land the change. \
                 Every edit goes through `FileEditTool` so the backup \
                 (R2.a) + journal hooks fire automatically."
                .into(),
            target: None,
        },
        PlanStep {
            id: 3,
            title: "Verify".into(),
            detail: "`cargo check --workspace --tests` + targeted \
                 `cargo nextest run -p <touched-crate>`. Transcript \
                 under `.vac/sessions/<plan-id>.jsonl` carries the \
                 durability record."
                .into(),
            target: None,
        },
    ]
}

fn first_words(s: &str, n: usize) -> String {
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

fn render_markdown(plan: &PlanDocument) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Plan {}\n\n", plan.id));
    if let Some(r) = &plan.remote {
        out.push_str(&format!("> Remote endpoint: `{}`\n", r));
    } else {
        out.push_str(
            "> Local mock plan (EchoAdapter). Pass `--remote <uri>` for a real planner.\n",
        );
    }
    out.push_str(&format!(
        "> Generated: {}\n\n",
        plan.generated_at.to_rfc3339()
    ));
    out.push_str("## Prompt\n\n");
    out.push_str(&plan.prompt);
    out.push_str("\n\n## Summary\n\n");
    out.push_str(&plan.summary);
    out.push_str("\n\n## Steps\n\n");
    for step in &plan.steps {
        out.push_str(&format!("### {}. {}\n\n", step.id, step.title));
        out.push_str(&step.detail);
        out.push('\n');
        if let Some(t) = &step.target {
            out.push_str(&format!("\n*Target: `{}`*\n", t.display()));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "\n---\n\nTranscript: `{}`\n\nApply: `vac plan apply {}`\n",
        plan.transcript_path.display(),
        plan.id,
    ));
    out
}

/// Run one planning submit. Returns the resolved `PlanDocument` so
/// callers (both the CLI and future unit tests) can assert on the
/// shape without reading the file back.
pub async fn generate_plan(
    project_root: &std::path::Path,
    prompt: String,
    remote: Option<String>,
) -> anyhow::Result<PlanDocument> {
    let plan_id = Uuid::new_v4();
    let plan_dir = project_root.join(".vac").join("plans");
    tokio::fs::create_dir_all(&plan_dir).await?;

    let writer = TranscriptWriter::new(project_root.join(".vac").join("sessions"));
    let ctx = SubmitContext::new(plan_id, &prompt).with_metadata(serde_json::json!({
        "plan_id": plan_id.to_string(),
        "remote": remote,
        "kind": "vac_plan",
    }));
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();

    // Route to the bridge-backed adapter when `--remote` is set.
    // Unsupported schemes (ws://, http://) surface as an error so the
    // mock path never silently swallows a real remote request.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let _snap = if let Some(uri) = &remote {
        let remote_adapter = crate::commands::plan_remote::RemoteSessionAdapter::spawn_stdio(uri)
            .await
            .map_err(|e| anyhow::anyhow!("remote planner: {e}"))?;
        submit_one(
            ctx,
            &writer,
            &slash,
            &compact,
            &usage,
            &remote_adapter,
            CompactConfig::default(),
            Some(tx),
        )
        .await?
    } else {
        let adapter = EchoAdapter;
        submit_one(
            ctx,
            &writer,
            &slash,
            &compact,
            &usage,
            &adapter,
            CompactConfig::default(),
            Some(tx),
        )
        .await?
    };

    let mut reply_content = String::new();
    while let Ok(ev) = rx.try_recv() {
        if let SubmitEvent::LlmChunk { text } = ev {
            reply_content.push_str(&text);
        }
    }

    let steps = parse_structured_steps(&reply_content)
        .unwrap_or_else(|| steps_from_reply(&prompt, &reply_content));
    let transcript_path = writer.sessions_dir().join(format!("{plan_id}.jsonl"));
    let plan = PlanDocument {
        id: plan_id,
        prompt,
        remote,
        generated_at: chrono::Utc::now(),
        summary: if reply_content.is_empty() {
            "No adapter reply captured.".into()
        } else {
            format!(
                "Adapter reply: {}",
                reply_content.chars().take(200).collect::<String>()
            )
        },
        steps,
        transcript_path,
    };

    // Write .md + .json sidecar. .md for operator review; .json for
    // `vac plan apply` to consume the structured steps.
    let md_path = plan_dir.join(format!("{plan_id}.md"));
    let json_path = plan_dir.join(format!("{plan_id}.json"));
    tokio::fs::write(&md_path, render_markdown(&plan)).await?;
    tokio::fs::write(&json_path, serde_json::to_vec_pretty(&plan)?).await?;

    Ok(plan)
}

pub async fn execute(
    project_root: PathBuf,
    prompt: String,
    remote: Option<String>,
) -> anyhow::Result<()> {
    if let Some(r) = &remote {
        println!("🛰  Offloading planning to remote endpoint: {}", r);
    } else {
        println!("🧠 Running local planning session (EchoAdapter)");
    }
    let plan = generate_plan(&project_root, prompt, remote).await?;
    let md_path = project_root
        .join(".vac")
        .join("plans")
        .join(format!("{}.md", plan.id));
    let json_path = project_root
        .join(".vac")
        .join("plans")
        .join(format!("{}.json", plan.id));
    println!("📝 Plan: {}", md_path.display());
    println!(
        "📦 Steps: {} (JSON sidecar at {})",
        plan.steps.len(),
        json_path.display()
    );
    println!("📓 Transcript: {}", plan.transcript_path.display());
    println!("▶  Apply: vac plan apply {}", plan.id);
    Ok(())
}

pub async fn execute_apply(project_root: PathBuf, plan_id: String) -> anyhow::Result<()> {
    let plan_dir = project_root.join(".vac").join("plans");
    let json_path = plan_dir.join(format!("{plan_id}.json"));
    let md_path = plan_dir.join(format!("{plan_id}.md"));
    if !json_path.exists() {
        anyhow::bail!(
            "Plan {} not found at {} (expected .json sidecar)",
            plan_id,
            json_path.display(),
        );
    }
    let raw = tokio::fs::read(&json_path).await?;
    let plan: PlanDocument =
        serde_json::from_slice(&raw).map_err(|e| anyhow::anyhow!("plan json parse: {e}"))?;
    println!("📋 Applying plan {} ({} steps)", plan.id, plan.steps.len());
    println!("   Prompt: {}", plan.prompt);
    for step in &plan.steps {
        println!("   [{}] {}", step.id, step.title);
    }
    println!();
    println!(
        "Plan apply is advisory — steps are operator-executed today. \
         Read `{}` for full detail.",
        md_path.display(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn generate_plan_writes_both_md_and_json() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = generate_plan(tmp.path(), "refactor auth module".into(), None)
            .await
            .unwrap();
        assert_eq!(plan.prompt, "refactor auth module");
        assert_eq!(plan.steps.len(), 3);
        assert!(plan.steps[0].title.contains("Investigate"));
        assert!(plan.steps[2].title == "Verify");

        let md = tmp
            .path()
            .join(".vac")
            .join("plans")
            .join(format!("{}.md", plan.id));
        let json = tmp
            .path()
            .join(".vac")
            .join("plans")
            .join(format!("{}.json", plan.id));
        assert!(md.exists(), "markdown plan should be written");
        assert!(json.exists(), "json sidecar should be written");

        // Round-trip the JSON so `execute_apply` can read it back.
        let raw = std::fs::read(&json).unwrap();
        let round: PlanDocument = serde_json::from_slice(&raw).unwrap();
        assert_eq!(round.id, plan.id);
        assert_eq!(round.steps.len(), 3);
    }

    #[tokio::test]
    async fn generate_plan_with_remote_embeds_endpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = generate_plan(
            tmp.path(),
            "short prompt".into(),
            Some("stdio://mock".into()),
        )
        .await
        .unwrap();
        assert_eq!(plan.remote.as_deref(), Some("stdio://mock"));
        let md = std::fs::read_to_string(
            tmp.path()
                .join(".vac/plans")
                .join(format!("{}.md", plan.id)),
        )
        .unwrap();
        assert!(md.contains("stdio://mock"));
    }

    #[tokio::test]
    async fn apply_errors_on_unknown_plan_id() {
        let tmp = tempfile::tempdir().unwrap();
        let err = execute_apply(tmp.path().to_path_buf(), "nonexistent".into())
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    #[tokio::test]
    async fn apply_reads_structured_steps_from_json() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = generate_plan(tmp.path(), "sample".into(), None)
            .await
            .unwrap();
        // apply should succeed without touching the fs (no patch
        // application today); returning Ok is the contract.
        execute_apply(tmp.path().to_path_buf(), plan.id.to_string())
            .await
            .unwrap();
    }

    #[test]
    fn structured_steps_wrapped_parses() {
        let reply =
            r#"{"steps":[{"title":"A","detail":"do a","target":"src/a.rs"},{"title":"B"}]}"#;
        let steps = parse_structured_steps(reply).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id, 1);
        assert_eq!(steps[0].title, "A");
        assert_eq!(steps[0].target, Some(PathBuf::from("src/a.rs")));
        assert_eq!(steps[1].id, 2);
        assert_eq!(steps[1].detail, "");
    }

    #[test]
    fn structured_steps_bare_array_parses() {
        let reply = r#"[{"title":"One"},{"title":"Two"}]"#;
        let steps = parse_structured_steps(reply).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].title, "Two");
    }

    #[test]
    fn structured_steps_non_json_returns_none() {
        assert!(parse_structured_steps("echo: hello").is_none());
        assert!(parse_structured_steps("").is_none());
        assert!(parse_structured_steps("   ").is_none());
    }

    #[test]
    fn structured_steps_empty_array_returns_none() {
        assert!(parse_structured_steps("[]").is_none());
        assert!(parse_structured_steps(r#"{"steps":[]}"#).is_none());
    }

    #[test]
    fn first_words_respects_cap() {
        assert_eq!(first_words("one two three four", 2), "one two");
        assert_eq!(first_words("", 5), "");
    }
}
