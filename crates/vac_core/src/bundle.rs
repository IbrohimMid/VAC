use crate::approval::{ApprovalRecord, ApprovalState, ApprovalStore};
use crate::error::{VacError, VacResult};
use crate::session::{Session, SessionMetadata};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use vil_llm::provider::Message;

const CURRENT_BUNDLE_SCHEMA_VERSION: &str = "0.1.0";
const MAX_CONTEXT_SUMMARY_BYTES: u64 = 10 * 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSignature {
    pub algorithm: String,
    pub public_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMetadata {
    pub version: String,
    pub session_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub exported_at: DateTime<Utc>,
    pub redacted: bool,
    #[serde(default)]
    pub session_metadata: SessionMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<BundleSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacBundle {
    pub metadata: BundleMetadata,
    pub transcript: Vec<Message>,
    #[serde(default)]
    pub approvals: Vec<ApprovalRecord>,
    pub context_summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BundleExportOptions {
    pub redact_secrets: bool,
    pub sign: bool,
}

impl Default for BundleExportOptions {
    fn default() -> Self {
        Self {
            redact_secrets: true,
            sign: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BundleImportOptions {
    pub require_signed: bool,
    pub overwrite_session: bool,
    pub trust_approvals: bool,
    pub redact_on_import: bool,
}

impl Default for BundleImportOptions {
    fn default() -> Self {
        Self {
            require_signed: false,
            overwrite_session: false,
            trust_approvals: false,
            redact_on_import: true,
        }
    }
}

fn default_export_path(project_root: &Path, session_id: Uuid) -> PathBuf {
    project_root.join(format!(".vac/exports/{}.bundle.json", session_id))
}

fn checkpoint_state_path(project_root: &Path, session_id: Uuid) -> PathBuf {
    project_root
        .join(".vac/checkpoints")
        .join(format!("{session_id}_state.json"))
}

fn session_path(project_root: &Path, session_id: Uuid) -> PathBuf {
    project_root
        .join(".vac/sessions")
        .join(format!("{session_id}.json"))
}

fn read_context_summary(project_root: &Path) -> Option<String> {
    let candidates = [
        project_root.join("summary.md"),
        project_root.join(".vac/summary.md"),
    ];

    for path in candidates {
        match read_text_with_cap(&path, MAX_CONTEXT_SUMMARY_BYTES) {
            Ok(Some(content)) => return Some(content),
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to read context summary");
            }
        }
    }

    None
}

fn read_text_with_cap(path: &Path, max_bytes: u64) -> VacResult<Option<String>> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    if meta.len() > max_bytes {
        tracing::warn!(
            path = %path.display(),
            size = meta.len(),
            limit = max_bytes,
            "skipping file larger than configured cap"
        );
        return Ok(None);
    }

    Ok(Some(std::fs::read_to_string(path)?))
}

fn write_context_summary(project_root: &Path, content: &str) -> VacResult<PathBuf> {
    if content.len() as u64 > MAX_CONTEXT_SUMMARY_BYTES {
        return Err(VacError::Session(format!(
            "Context summary exceeds {MAX_CONTEXT_SUMMARY_BYTES} byte cap"
        )));
    }

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

fn bundle_major(version: &str) -> VacResult<u64> {
    version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u64>().ok())
        .ok_or_else(|| VacError::Session(format!("Invalid bundle schema version: {version}")))
}

fn ensure_supported_bundle_version(version: &str) -> VacResult<()> {
    let current_major = bundle_major(CURRENT_BUNDLE_SCHEMA_VERSION)?;
    let bundle_major = bundle_major(version)?;
    if bundle_major != current_major {
        return Err(VacError::Session(format!(
            "Unsupported bundle schema major {bundle_major} (expected {current_major})"
        )));
    }
    Ok(())
}

fn signed_payload(bundle: &VacBundle) -> VacResult<Vec<u8>> {
    let mut unsigned = bundle.clone();
    unsigned.metadata.signature = None;
    serde_json::to_vec(&unsigned).map_err(VacError::from)
}

fn sign_bundle(bundle: &mut VacBundle) -> VacResult<()> {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let payload = signed_payload(bundle)?;
    let signature = signing_key.sign(&payload);

    bundle.metadata.signature = Some(BundleSignature {
        algorithm: "ed25519".to_string(),
        public_key: STANDARD.encode(verifying_key.as_bytes()),
        signature: STANDARD.encode(signature.to_bytes()),
    });

    Ok(())
}

fn verify_bundle_signature(bundle: &VacBundle, require_signed: bool) -> VacResult<()> {
    let Some(signature) = &bundle.metadata.signature else {
        if require_signed {
            return Err(VacError::Session(
                "Bundle signature required but missing".to_string(),
            ));
        }
        return Ok(());
    };

    if signature.algorithm != "ed25519" {
        return Err(VacError::Session(format!(
            "Unsupported bundle signature algorithm: {}",
            signature.algorithm
        )));
    }

    let public_key = STANDARD
        .decode(signature.public_key.as_bytes())
        .map_err(|e| VacError::Session(format!("Invalid bundle public key encoding: {e}")))?;
    let signature_bytes = STANDARD
        .decode(signature.signature.as_bytes())
        .map_err(|e| VacError::Session(format!("Invalid bundle signature encoding: {e}")))?;

    let public_key_bytes: [u8; 32] = public_key
        .as_slice()
        .try_into()
        .map_err(|_| VacError::Session("Invalid bundle public key length".to_string()))?;
    let public_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|e| VacError::Session(format!("Invalid bundle public key: {e}")))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|e| VacError::Session(format!("Invalid bundle signature: {e}")))?;
    let payload = signed_payload(bundle)?;

    public_key
        .verify(&payload, &signature)
        .map_err(|e| VacError::Session(format!("Bundle signature verification failed: {e}")))?;

    Ok(())
}

fn purge_existing_session(project_root: &Path, session_id: Uuid) -> VacResult<()> {
    let session_path = session_path(project_root, session_id);
    let checkpoint_path = checkpoint_state_path(project_root, session_id);

    if session_path.exists() {
        std::fs::remove_file(&session_path)?;
    }
    if checkpoint_path.exists() {
        std::fs::remove_file(&checkpoint_path)?;
    }

    let store = ApprovalStore::new(project_root.to_path_buf());
    let _ = store.remove_by_session(session_id)?;
    Ok(())
}

fn session_collision_exists(project_root: &Path, session_id: Uuid) -> bool {
    session_path(project_root, session_id).exists()
        || checkpoint_state_path(project_root, session_id).exists()
}

pub fn export_bundle_to_path(
    project_root: &Path,
    session_id: Option<Uuid>,
    output: Option<&Path>,
    redact_secrets: bool,
) -> VacResult<PathBuf> {
    export_bundle_to_path_with_options(
        project_root,
        session_id,
        output,
        BundleExportOptions {
            redact_secrets,
            sign: false,
        },
    )
}

pub fn export_bundle_to_path_with_options(
    project_root: &Path,
    session_id: Option<Uuid>,
    output: Option<&Path>,
    options: BundleExportOptions,
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
        version: CURRENT_BUNDLE_SCHEMA_VERSION.to_string(),
        session_id: sid,
        created_at,
        exported_at: Utc::now(),
        redacted: false,
        session_metadata: session.metadata.clone(),
        signature: None,
    };

    let mut bundle = VacBundle {
        metadata,
        transcript: state.messages,
        approvals,
        context_summary: read_context_summary(project_root),
    };

    if options.redact_secrets {
        bundle = redact_bundle(bundle);
    }
    if options.sign {
        sign_bundle(&mut bundle)?;
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
    import_bundle_from_path_with_options(project_root, input, BundleImportOptions::default())
}

pub fn import_bundle_from_path_with_options(
    project_root: &Path,
    input: &Path,
    options: BundleImportOptions,
) -> VacResult<Uuid> {
    let meta = std::fs::metadata(input)?;
    if meta.len() > MAX_BUNDLE_BYTES {
        return Err(VacError::Session(format!(
            "Bundle exceeds {MAX_BUNDLE_BYTES} byte cap: {}",
            input.display()
        )));
    }

    let content = std::fs::read(input)?;
    let mut bundle: VacBundle = serde_json::from_slice(&content)?;
    let sid = bundle.metadata.session_id;

    ensure_supported_bundle_version(&bundle.metadata.version)?;
    verify_bundle_signature(&bundle, options.require_signed)?;

    if session_collision_exists(project_root, sid) && !options.overwrite_session {
        return Err(VacError::Session(format!(
            "Session {sid} already exists. Re-run with overwrite enabled to replace it."
        )));
    }

    if options.redact_on_import {
        bundle = redact_bundle(bundle);
    } else {
        tracing::warn!(
            session_id = %sid,
            "importing bundle without redaction; secrets in the bundle will be preserved"
        );
    }

    if let Some(summary) = bundle.context_summary.as_ref() {
        if summary.len() as u64 > MAX_CONTEXT_SUMMARY_BYTES {
            return Err(VacError::Session(format!(
                "Imported context summary exceeds {MAX_CONTEXT_SUMMARY_BYTES} byte cap"
            )));
        }
    }

    if options.trust_approvals {
        tracing::warn!(
            session_id = %sid,
            "trusting imported approvals and restoring approved tool calls"
        );
    }

    if options.overwrite_session {
        purge_existing_session(project_root, sid)?;
    }

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
    if options.trust_approvals {
        state.approved_tools = bundle
            .approvals
            .iter()
            .filter(|r| r.state == ApprovalState::Approved)
            .map(|r| r.tool_call_id.clone())
            .collect();
    }

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
