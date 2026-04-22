//! `vac observe` / `vac explain` / `vac why` — trajectory inspection surface.

use std::path::PathBuf;

use anyhow::Context;

pub async fn observe(project_root: PathBuf, format: &str, limit: usize) -> anyhow::Result<()> {
    let trajectories = vac_trajectory::collect_recent_trajectories(&project_root, limit).await?;
    if format == "json" {
        return crate::output::print_json(&trajectories);
    }

    if trajectories.is_empty() {
        println!("No trajectory artifacts found.");
        return Ok(());
    }

    println!("VAC Trajectories");
    println!("{}", "=".repeat(80));
    for artifact in trajectories {
        print_artifact(&artifact);
        println!();
    }

    Ok(())
}

pub async fn explain(
    project_root: PathBuf,
    format: &str,
    target: Option<String>,
) -> anyhow::Result<()> {
    let report = vac_trajectory::explain_trajectory(&project_root, target.as_deref()).await?;
    let report = report.context("no trajectory matched the requested selector")?;

    if format == "json" {
        return crate::output::print_json(&report);
    }

    print_explain_report(&report);
    Ok(())
}

pub async fn why(
    project_root: PathBuf,
    format: &str,
    path: PathBuf,
    trajectory: Option<String>,
) -> anyhow::Result<()> {
    let report =
        vac_trajectory::why_file_changed(&project_root, &path, trajectory.as_deref()).await?;
    let report = report.context("no trajectory matched the requested file")?;

    if format == "json" {
        return crate::output::print_json(&report);
    }

    print_why_report(&report);
    Ok(())
}

fn print_artifact(artifact: &vac_trajectory::TrajectoryArtifact) {
    println!("[{}] {}", artifact.source.as_str(), artifact.short_label);
    println!("  title:   {}", artifact.title);
    println!("  status:  {:?}", artifact.status);
    println!(
        "  updated: {}",
        artifact
            .updated_at
            .map(|ts| ts.to_rfc3339())
            .unwrap_or_else(|| "<unknown>".to_string())
    );
    if let Some(summary) = &artifact.summary {
        println!("  summary: {}", summary);
    }
    if !artifact.modified_files.is_empty() || !artifact.created_files.is_empty() {
        println!(
            "  files:   {} modified, {} created",
            artifact.modified_files.len(),
            artifact.created_files.len()
        );
    }
    if let Some(tokens) = artifact.total_tokens_used {
        println!("  tokens:  {tokens}");
    }
}

fn print_explain_report(report: &vac_trajectory::ExplainReport) {
    let artifact = &report.artifact;
    println!("Trajectory: {}", artifact.display_name());
    println!("  path:    {}", artifact.artifact_path.display());
    println!("  title:   {}", artifact.title);
    println!("  status:  {:?}", artifact.status);
    println!(
        "  updated: {}",
        artifact
            .updated_at
            .map(|ts| ts.to_rfc3339())
            .unwrap_or_else(|| "<unknown>".to_string())
    );
    if let Some(summary) = &artifact.summary {
        println!("  summary: {}", summary);
    }
    if let Some(task) = &artifact.task_description {
        println!("  task:    {}", task);
    }
    if !artifact.modified_files.is_empty() {
        println!("  modified: {}", artifact.modified_files.join(", "));
    }
    if !artifact.created_files.is_empty() {
        println!("  created:  {}", artifact.created_files.join(", "));
    }
    if !artifact.target_paths.is_empty() {
        println!("  targets:  {}", artifact.target_paths.join(", "));
    }
    if !artifact.target_modules.is_empty() {
        println!("  modules:  {}", artifact.target_modules.join(", "));
    }
    if !artifact.evidence.is_empty() {
        println!("  evidence:");
        for item in artifact.evidence.iter().take(6) {
            println!("    - {}: {}", item.label, item.detail);
        }
    }
}

fn print_why_report(report: &vac_trajectory::FileWhyReport) {
    println!("Why: {}", report.path);
    println!("  normalized: {}", report.normalized_path);
    for (idx, item) in report.matches.iter().take(3).enumerate() {
        println!(
            "  match {}: [{}] {}",
            idx + 1,
            item.artifact.display_name(),
            match item.match_reason {
                vac_trajectory::FileMatchReason::ModifiedFile => "modified file",
                vac_trajectory::FileMatchReason::CreatedFile => "created file",
                vac_trajectory::FileMatchReason::TargetPath => "target path",
                vac_trajectory::FileMatchReason::TargetModule => "target module",
                vac_trajectory::FileMatchReason::TraceArgument => "trace evidence",
            }
        );
        println!("    detail: {}", item.detail);
        if let Some(summary) = &item.artifact.summary {
            println!("    summary: {}", summary);
        }
        if let Some(task) = &item.artifact.task_description {
            println!("    task:    {}", task);
        }
    }
}
