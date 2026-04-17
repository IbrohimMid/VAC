use crate::approval::{ApprovalRecord, ApprovalState, ApprovalStore};
use crate::error::{VacError, VacResult};
use crate::session::{Session, SessionMetadata};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use vil_llm::provider::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMetadata {
    pub version: String,
    pub session_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub exported_at: DateTime<Utc>,
    pub redacted: bool,
    #[serde(default)]
    pub session_metadata: SessionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacBundle {
    pub metadata: BundleMetadata,
    pub transcript: Vec<Message>,
    #[serde(default)]
    pub approvals: Vec<ApprovalRecord>,
    pub context_summary: Option<String>,
}

fn default_export_path(project_root: &Path, session_id: Uuid) -> PathBuf {
    project_root.join(format!(".vac/exports/{}.bundle.json", session_id))
}

fn checkpoint_state_path(project_root: &Path, session_id: Uuid) -> PathBuf {
    project_root
        .join(".vac/checkpoints")
        .join(format!("{session_id}_state.json"))
}

fn read_context_summary(project_root: &Path) -> Option<String> {
    let candidates = [
        project_root.join("summary.md"),
        project_root.join(".vac/summary.md"),
    ];
    for path in candidates {
        if let Ok(content) = std::fs::read_to_string(&path) {
            return Some(content);
        }
    }
    None
}

fn write_context_summary(project_root: &Path, content: &str) -> VacResult<PathBuf> {
    let base = project_root.join("summary.md");
    if !base.exists() {
        std::fs::write(&base, content)?;
        return Ok(base);
    }
    for idx in 1..1000usize {
        let path = project_root.join(format!("summary-imported-{idx}.md"));
        if !path.exists() {
            std::fs::write(&path, content)?;
            return Ok(path);
        }
    }
    Err(VacError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "Failed to pick summary-imported-N.md path",
    )))
}

fn redact_bundle(mut bundle: VacBundle) -> VacBundle {
    bundle.metadata.redacted = true;

    for msg in &mut bundle.transcript {
        msg.content = vil_swarm::redaction::redact_secrets(&msg.content);
        if let Some(name) = &mut msg.name {
            *name = vil_swarm::redaction::redact_secrets(name);
        }
        for tc in &mut msg.tool_calls {
            vil_swarm::redaction::redact_json(&mut tc.arguments);
        }
        for img in &mut msg.image_parts {
            img.data = "[REDACTED]".to_string();
        }
    }

    for rec in &mut bundle.approvals {
        vil_swarm::redaction::redact_json(&mut rec.arguments);
        if let Some(expl) = &mut rec.explanation {
            *expl = vil_swarm::redaction::redact_secrets(expl);
        }
        if let Some(reason) = &mut rec.reason {
            *reason = vil_swarm::redaction::redact_secrets(reason);
        }
    }

    if let Some(summary) = &mut bundle.context_summary {
        *summary = vil_swarm::redaction::redact_secrets(summary);
    }

    bundle
}

pub fn export_bundle_to_path(
    project_root: &Path,
    session_id: Option<Uuid>,
    output: Option<&Path>,
    redact_secrets: bool,
) -> VacResult<PathBuf> {
    let session = Session::load_latest(project_root)?
        .ok_or_else(|| VacError::Session("No session found".to_string()))?;
    let sid = session_id.unwrap_or(session.id);

    let state_path = checkpoint_state_path(project_root, sid);
    if !state_path.exists() {
        return Err(VacError::Session(format!(
            "Checkpoint state not found: {}",
            state_path.display()
        )));
    }

    let state = vil_swarm::run_state::AgentRunState::from_checkpoint(&state_path)
        .map_err(|e| VacError::Session(format!("Failed to load checkpoint: {e}")))?;

    let approvals_store = ApprovalStore::new(project_root.to_path_buf());
    let approvals = approvals_store.list_by_session(sid).unwrap_or_default();

    let created_at = session.created_at;
    let metadata = BundleMetadata {
        version: "0.1.0".to_string(),
        session_id: sid,
        created_at,
        exported_at: Utc::now(),
        redacted: false,
        session_metadata: session.metadata.clone(),
    };

    let mut bundle = VacBundle {
        metadata,
        transcript: state.messages,
        approvals,
        context_summary: read_context_summary(project_root),
    };

    if redact_secrets {
        bundle = redact_bundle(bundle);
    }

    let output_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| default_export_path(project_root, sid));
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, serde_json::to_string_pretty(&bundle)?)?;
    Ok(output_path)
}

pub fn import_bundle_from_path(project_root: &Path, input: &Path) -> VacResult<Uuid> {
    let content = std::fs::read_to_string(input)?;
    let bundle: VacBundle = serde_json::from_str(&content)?;
    let sid = bundle.metadata.session_id;

    std::fs::create_dir_all(project_root.join(".vac/sessions"))?;
    std::fs::create_dir_all(project_root.join(".vac/checkpoints"))?;
    std::fs::create_dir_all(project_root.join(".vac/approvals"))?;
    std::fs::create_dir_all(project_root.join(".vac/exports"))?;

    let session = Session {
        id: sid,
        project_root: project_root.to_path_buf(),
        created_at: bundle.metadata.created_at,
        updated_at: bundle.metadata.exported_at,
        tasks: Vec::new(),
        results: std::collections::HashMap::new(),
        metadata: bundle.metadata.session_metadata.clone(),
    };
    session.save()?;

    let mut state = vil_swarm::run_state::AgentRunState::new(bundle.transcript.clone(), None);
    state.stage = vil_swarm::run_state::RunStage::Completed;
    state.approved_tools = bundle
        .approvals
        .iter()
        .filter(|r| r.state == ApprovalState::Approved)
        .map(|r| r.tool_call_id.clone())
        .collect();

    let state_path = checkpoint_state_path(project_root, sid);
    state
        .save_checkpoint(&state_path, Some(sid))
        .map_err(|e| VacError::Session(format!("Failed to save checkpoint: {e}")))?;

    let store = ApprovalStore::new(project_root.to_path_buf());
    for mut rec in bundle.approvals.clone() {
        if rec.session_id.is_none() {
            rec.session_id = Some(sid);
        }
        store.write_record(&rec)?;
    }

    if let Some(summary) = bundle.context_summary.as_deref() {
        let _ = write_context_summary(project_root, summary)?;
    }

    Ok(sid)
}
