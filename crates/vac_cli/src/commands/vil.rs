//! `vac vil` — passthrough command surface for the external VIL binary.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, anyhow};
use glob::glob;
use regex::Regex;
use semver::{Version, VersionReq};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use uuid::Uuid;

pub async fn execute(
    project_root: PathBuf,
    _format: &str,
    action: crate::VilAction,
) -> anyhow::Result<()> {
    match action {
        crate::VilAction::Init => {
            let config = load_vac_config(&project_root)?;
            run_vil(&project_root, &config.vil, vec!["init".to_string()]).await
        }
        crate::VilAction::Dev => {
            let config = load_vac_config(&project_root)?;
            run_vil(&project_root, &config.vil, vec!["dev".to_string()]).await
        }
        crate::VilAction::Gen {
            entity,
            kind,
            execution_mode,
            name,
        } => execute_codegen(&project_root, entity, kind, execution_mode, name).await,
        crate::VilAction::Deploy { target } => {
            let config = load_vac_config(&project_root)?;
            execute_deploy(&project_root, &config, target).await
        }
    }
}

fn load_vac_config(project_root: &Path) -> anyhow::Result<vac_core::VacConfig> {
    vac_core::VacConfig::load_with_fallback(project_root)
        .with_context(|| format!("failed to load VAC config for {}", project_root.display()))
}

async fn execute_deploy(
    project_root: &Path,
    config: &vac_core::VacConfig,
    target: Option<String>,
) -> anyhow::Result<()> {
    let decision = vac_core::policy_gate::evaluate(
        &config.policy_gate,
        vac_core::PolicyGateAction::Deploy,
        None,
    );
    let approval_target = target.clone();

    match &decision {
        vac_core::PolicyGateDecision::Allow => {
            let mut args = vec!["deploy".to_string()];
            if let Some(target) = target {
                args.push(target);
            }
            return run_vil(project_root, &config.vil, args).await;
        }
        vac_core::PolicyGateDecision::Warn(message)
        | vac_core::PolicyGateDecision::Block(message) => {
            eprintln!("{message}");

            let approval_store = vac_approvals::ApprovalStore::new(project_root.to_path_buf());
            let tool_call_id = format!("vil-deploy-{}", Uuid::new_v4());
            let approval_payload = json!({
                "action": "deploy",
                "target": approval_target,
                "policy_gate": {
                    "enabled": config.policy_gate.enable,
                    "mode": config.policy_gate.mode,
                    "threshold": config.policy_gate.threshold,
                }
            });

            approval_store.record_request(
                tool_call_id.clone(),
                "vil deploy".to_string(),
                approval_payload,
                Some(message.clone()),
                None,
                None,
            )?;

            eprint!("Proceed with `vil deploy`? [y/N]: ");
            std::io::stderr().flush()?;

            let input = crate::io::read_line_async().await.unwrap_or_default();
            let approved = matches!(input.trim().to_lowercase().as_str(), "y" | "yes");
            let reason = if approved {
                Some("Approved via vac vil deploy".to_string())
            } else {
                Some("Rejected via vac vil deploy".to_string())
            };
            approval_store.record_decision(tool_call_id, approved, reason)?;

            if !approved {
                return Err(anyhow!("`vil deploy` rejected by user"));
            }

            let mut args = vec!["deploy".to_string()];
            if let Some(target) = target {
                args.push(target);
            }
            run_vil(project_root, &config.vil, args).await
        }
    }
}

async fn run_vil(
    project_root: &Path,
    vil: &vac_core::VilConfig,
    args: Vec<String>,
) -> anyhow::Result<()> {
    let display_args = if args.is_empty() {
        String::new()
    } else {
        args.join(" ")
    };

    let binary = resolve_vil_binary_from_config(project_root, vil);
    if let Some(requirement) = vil.min_version.as_ref() {
        let version = probe_vil_version(&binary)
            .await
            .with_context(|| format!("failed to determine version for `{}`", binary.display()))?;
        match version {
            Some(version) if version_satisfies(&version, Some(requirement)) => {}
            Some(version) => {
                return Err(anyhow!(
                    "`{}` version {} does not satisfy required {}",
                    binary.display(),
                    version,
                    requirement
                ));
            }
            None => {
                return Err(anyhow!(
                    "`{}` did not report a parseable version but {} is required",
                    binary.display(),
                    requirement
                ));
            }
        }
    }

    let mut command = Command::new(&binary);
    command
        .args(&args)
        .current_dir(project_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    let status = command
        .status()
        .await
        .with_context(|| format!("failed to spawn `{} {display_args}`", binary.display()))?;

    if !status.success() {
        let code = status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        return Err(anyhow!(
            "`{} {display_args}` exited with {code}",
            binary.display()
        ));
    }

    Ok(())
}

pub fn resolve_vil_binary_from_config(project_root: &Path, vil: &vac_core::VilConfig) -> PathBuf {
    if let Ok(override_bin) = std::env::var("VAC_VIL_BIN") {
        let trimmed = override_bin.trim();
        if !trimmed.is_empty() {
            return resolve_binary_candidate(project_root, Path::new(trimmed));
        }
    }

    vil.binary_path
        .as_deref()
        .map(|path| resolve_binary_candidate(project_root, path))
        .unwrap_or_else(|| PathBuf::from("vil"))
}

fn resolve_binary_candidate(project_root: &Path, candidate: &Path) -> PathBuf {
    if candidate.is_absolute() || is_bare_command_name(candidate) {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    }
}

fn is_bare_command_name(candidate: &Path) -> bool {
    let mut components = candidate.components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

pub fn collect_vwfd_documents(
    project_root: &Path,
    patterns: &[PathBuf],
) -> anyhow::Result<Vec<PathBuf>> {
    let mut seen = HashSet::new();
    let mut docs = Vec::new();

    for pattern in patterns {
        let resolved = if pattern.is_absolute() {
            pattern.clone()
        } else {
            project_root.join(pattern)
        };

        let pattern_str = resolved.to_string_lossy().to_string();
        for entry in glob(&pattern_str)
            .with_context(|| format!("invalid VIL VWFD glob pattern: {pattern_str}"))?
        {
            match entry {
                Ok(path) => {
                    if seen.insert(path.clone()) {
                        docs.push(path);
                    }
                }
                Err(err) => {
                    return Err(anyhow!(err.to_string()));
                }
            }
        }
    }

    docs.sort();
    Ok(docs)
}

pub async fn probe_vil_version(binary: &Path) -> anyhow::Result<Option<Version>> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .await
        .with_context(|| format!("failed to execute `{}` --version", binary.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "`{} --version` exited with {}",
            binary.display(),
            output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(extract_version(&stdout).or_else(|| extract_version(&stderr)))
}

pub fn version_satisfies(version: &Version, requirement: Option<&VersionReq>) -> bool {
    requirement.is_none_or(|req| req.matches(version))
}

async fn execute_codegen(
    project_root: &Path,
    entity: String,
    kind: String,
    execution_mode: String,
    name: String,
) -> anyhow::Result<()> {
    let entity = entity.trim().to_ascii_lowercase();
    if entity != "handler" {
        return Err(anyhow!("unsupported VIL generator entity: {entity}"));
    }

    let kind_label = kind.trim().to_string();
    let kind = vil_vwfd::codegen::parse_kind(&kind)?;
    let execution_mode_label = execution_mode.trim().to_string();
    let execution_mode = vil_vwfd::codegen::parse_execution_mode(&execution_mode)?;
    let artifact = vil_vwfd::generate_handler(kind, execution_mode, &name)?;
    let staging_dir = tempfile::tempdir().context("failed to create staging dir")?;
    write_generated_artifact_to(staging_dir.path(), &artifact).await?;

    let scaffold_name = artifact.document.metadata.name.clone();
    let preview = artifact.preview.clone();
    println!("{}", preview);
    let entity_label = entity.clone();

    let approval_store = vac_approvals::ApprovalStore::new(project_root.to_path_buf());
    let tool_call_id = format!("vil-gen-{}", Uuid::new_v4());
    let approval_payload = json!({
        "action": "gen",
        "entity": entity_label.clone(),
        "kind": kind_label.clone(),
        "execution_mode": execution_mode_label.clone(),
        "name": scaffold_name.clone(),
        "files": artifact.files.iter().map(|file| json!({
            "path": file.path.display().to_string(),
            "bytes": file.contents.len(),
        })).collect::<Vec<_>>(),
        "vwfd_preview": preview,
    });
    approval_store.record_request(
        tool_call_id.clone(),
        "vil gen".to_string(),
        approval_payload,
        Some(format!(
            "Generate {kind_label} scaffold with {} file(s)",
            artifact.files.len()
        )),
        None,
        None,
    )?;

    eprint!(
        "Proceed with `vil gen {entity_label} --kind {kind_label} --execution-mode {execution_mode_label} --name {scaffold_name}`? [y/N]: "
    );
    std::io::stderr().flush()?;

    let input = crate::io::read_line_async().await.unwrap_or_default();
    let approved = matches!(input.trim().to_lowercase().as_str(), "y" | "yes");
    let reason = if approved {
        Some("Approved via vac vil gen".to_string())
    } else {
        Some("Rejected via vac vil gen".to_string())
    };
    approval_store.record_decision(tool_call_id, approved, reason)?;

    if !approved {
        return Err(anyhow!("`vil gen` rejected by user"));
    }

    let scaffold_root = staging_dir
        .path()
        .join("handlers")
        .join(&artifact.document.metadata.name);
    let parity_issues = vil_validate::passes::vwfd_parity_pass(&artifact.document, &scaffold_root);
    if !parity_issues.is_empty() {
        let details = parity_issues
            .iter()
            .map(|issue| issue.label())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(anyhow!(
            "VWFD parity gate failed for `{}`: {}",
            artifact.document.metadata.name,
            details
        ));
    }

    write_generated_artifact_to(project_root, &artifact).await?;

    println!(
        "Generated {} file(s) for `{}`",
        artifact.files.len(),
        artifact.document.metadata.name.as_str()
    );
    Ok(())
}

async fn write_generated_artifact_to(
    project_root: &Path,
    artifact: &vil_vwfd::GeneratedArtifact,
) -> anyhow::Result<()> {
    for file in &artifact.files {
        let target = project_root.join(&file.path);
        if tokio::fs::try_exists(&target).await? {
            let existing = tokio::fs::read_to_string(&target)
                .await
                .with_context(|| format!("failed to read existing {}", target.display()))?;
            if existing != file.contents {
                return Err(anyhow!(
                    "refusing to overwrite existing file with different contents: {}",
                    target.display()
                ));
            }
        }
    }

    for file in &artifact.files {
        let target = project_root.join(&file.path);
        if tokio::fs::try_exists(&target).await? {
            continue;
        }

        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create parent {}", parent.display()))?;
        }

        let mut handle = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)
            .await
            .with_context(|| format!("failed to create {}", target.display()))?;
        handle
            .write_all(file.contents.as_bytes())
            .await
            .with_context(|| format!("failed to write {}", target.display()))?;
    }

    Ok(())
}

fn extract_version(text: &str) -> Option<Version> {
    let re = Regex::new(
        r"(?x)
        (?P<version>
            \d+\.\d+\.\d+
            (?:-[0-9A-Za-z.-]+)?
            (?:\+[0-9A-Za-z.-]+)?
        )
    ",
    )
    .ok()?;

    let captures = re.captures(text)?;
    Version::parse(captures.name("version")?.as_str()).ok()
}
