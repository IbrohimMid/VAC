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

pub async fn decisions(
    project_root: PathBuf,
    format: &str,
    path: Option<PathBuf>,
) -> anyhow::Result<()> {
    use vac_trajectory::decisions::{DecisionStats, load_decisions_from_file};

    let trace_path = match path {
        Some(p) => p,
        None => {
            let traces_dir = project_root.join(".vac").join("traces");
            let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
            if let Ok(entries) = std::fs::read_dir(&traces_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if newest
                                .as_ref()
                                .is_none_or(|(best, _)| modified > *best)
                            {
                                newest = Some((modified, p));
                            }
                        }
                    }
                }
            }
            newest
                .map(|(_, p)| p)
                .context("no trace files found under .vac/traces/")?
        }
    };

    let records = load_decisions_from_file(&trace_path).await?;
    let stats = DecisionStats::from_records(&records);

    if format == "json" {
        return crate::output::print_json(&serde_json::json!({
            "trace": trace_path,
            "stats": stats,
            "records": records,
        }));
    }

    println!("Decisions from: {}", trace_path.display());
    println!(
        "  total: {}, with_alternatives: {}, with_rationale: {}",
        stats.total, stats.with_alternatives, stats.with_rationale
    );
    if !stats.unique_chosen.is_empty() {
        println!("  unique chosen: {}", stats.unique_chosen.join(", "));
    }
    if records.is_empty() {
        println!("  (no AgentDecision records in this trace)");
        return Ok(());
    }
    println!();
    for (idx, r) in records.iter().enumerate() {
        println!("  [{}] {} -> {}", idx + 1, r.timestamp.to_rfc3339(), r.chosen);
        if !r.rejected.is_empty() {
            println!("      rejected: {}", r.rejected.join(", "));
        }
        if let Some(rationale) = &r.rationale {
            println!("      rationale: {}", rationale);
        }
    }

    Ok(())
}

pub async fn eval(
    project_root: PathBuf,
    format: &str,
    path: Option<PathBuf>,
    succeeded: bool,
    duration_ms: u64,
) -> anyhow::Result<()> {
    use vac_trajectory::decisions::{
        DecisionOutcome, DecisionStats, load_decisions_from_file, score_decisions,
    };

    let trace_path = resolve_trace_path(&project_root, path)?;
    let records = load_decisions_from_file(&trace_path).await?;
    let stats = DecisionStats::from_records(&records);
    let outcome = DecisionOutcome { task_succeeded: succeeded, duration_ms };
    let report = score_decisions(&records, outcome);

    if format == "json" {
        return crate::output::print_json(&serde_json::json!({
            "trace": trace_path,
            "outcome": outcome,
            "stats": stats,
            "score": report,
        }));
    }

    println!("Eval: {}", trace_path.display());
    println!(
        "  outcome:        succeeded={}, duration_ms={}",
        succeeded, duration_ms
    );
    println!("  decisions:      {} total", stats.total);
    println!("  score:          {}/100", report.score);
    println!("  rationale:      {}", report.rationale);
    Ok(())
}

fn resolve_trace_path(
    project_root: &std::path::Path,
    path: Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    if let Some(p) = path {
        return Ok(p);
    }
    let traces_dir = project_root.join(".vac").join("traces");
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(entries) = std::fs::read_dir(&traces_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if newest
                        .as_ref()
                        .is_none_or(|(best, _)| modified > *best)
                    {
                        newest = Some((modified, p));
                    }
                }
            }
        }
    }
    newest
        .map(|(_, p)| p)
        .context("no trace files found under .vac/traces/")
}

pub async fn why(
    project_root: PathBuf,
    format: &str,
    path: PathBuf,
    trajectory: Option<String>,
) -> anyhow::Result<()> {
    let report = vac_trajectory::why_file_changed(&project_root, &path, trajectory.as_deref())
        .await?
        .context("no trajectory matched the requested selector")?;

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
    if report.matches.is_empty() {
        println!("  note: no direct evidence found in recent trajectories");
        return;
    }
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
