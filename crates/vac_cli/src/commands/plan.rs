use std::path::PathBuf;
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EchoAdapter, SlashProcessor, SubmitContext, TranscriptWriter,
    TrivialCompactBoundary, UsageTracker, submit_one,
};

pub async fn execute(
    project_root: PathBuf,
    prompt: String,
    remote: Option<String>,
) -> anyhow::Result<()> {
    if let Some(r) = &remote {
        println!("Offloading planning to remote endpoint: {}", r);
    } else {
        println!("Running local planning session for: {}", prompt);
    }

    let plan_id = Uuid::new_v4();
    let plan_dir = project_root.join(".vac/plans");
    std::fs::create_dir_all(&plan_dir)?;

    // Submit via vac_session_engine
    let writer = TranscriptWriter::new(project_root.join(".vac/sessions"));
    let ctx = SubmitContext::new(plan_id, &prompt);
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let adapter = EchoAdapter;

    let _snap = submit_one(
        ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &adapter,
        CompactConfig {
            max_budget_tokens: None, // relaxed budget
            ..Default::default()
        },
        None,
    ).await?;

    let plan_path = plan_dir.join(format!("{}.md", plan_id));
    let plan_content = format!("# Plan: {}\n\nThis is a generated plan from remote.\n\n## Tasks\n- Task 1\n- Task 2\n", prompt);
    std::fs::write(&plan_path, plan_content)?;

    println!("Plan generated: {}", plan_path.display());
    println!("To apply this plan, run: vac plan apply {}", plan_id);

    Ok(())
}

pub async fn execute_apply(project_root: PathBuf, plan_id: String) -> anyhow::Result<()> {
    let plan_path = project_root.join(format!(".vac/plans/{}.md", plan_id));
    if !plan_path.exists() {
        anyhow::bail!("Plan ID {} not found at {}", plan_id, plan_path.display());
    }
    println!("Applying plan: {}", plan_id);
    println!("(Simulating applying patches as a changeset...)");
    Ok(())
}
