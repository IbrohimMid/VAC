//! Export — package artifacts as ZIP with records, SBOM, and checksums.

use crate::error::{TraceError, TraceResult};
use crate::recorder::{RecordType, TraceRecord};
use crate::vac_format::{SigningKeyPair, VacEnvelope};
use std::path::Path;

/// Export a VAC audit artifact to `output_path`.
///
/// * `sign = Some(kp)` — produce a COSE_Sign1 signed CBOR artifact with the
///   provided key pair.  The public key is embedded in the artifact header so
///   verifiers can authenticate without an out-of-band trust store.
/// * `sign = None` — produce an unsigned CBOR artifact.
///
/// There is no "sign with a throwaway key" path: signing with an ephemeral
/// key that is immediately discarded is indistinguishable from not signing at
/// all (the signature cannot be verified), so that path has been removed.
pub fn export_vac(
    session_id: &str,
    records: Vec<TraceRecord>,
    output_path: &Path,
    sign: Option<&SigningKeyPair>,
) -> TraceResult<()> {
    let envelope = VacEnvelope::new(session_id, records)?;

    let bytes = match sign {
        Some(kp) => envelope.sign(kp)?,
        None => envelope.to_cbor()?,
    };

    std::fs::write(output_path, &bytes)?;
    tracing::info!(path = %output_path.display(), size = bytes.len(), signed = sign.is_some(), "VAC artifact exported");
    Ok(())
}

pub fn export_opencode_json(records: &[TraceRecord], output_path: &Path) -> TraceResult<()> {
    let json =
        serde_json::to_string_pretty(records).map_err(|e| TraceError::Export(e.to_string()))?;
    std::fs::write(output_path, json)?;
    Ok(())
}

/// Export records as newline-delimited JSON.
///
/// Fails loudly if any record cannot be serialized, rather than silently
/// dropping it, so the output line count always equals `records.len()`.
pub fn export_claude_jsonl(records: &[TraceRecord], output_path: &Path) -> TraceResult<()> {
    let mut lines = Vec::with_capacity(records.len());
    for (i, r) in records.iter().enumerate() {
        let line = serde_json::to_string(r).map_err(|e| {
            TraceError::Export(format!(
                "record #{i} (id={}) failed to serialize: {e}",
                r.id
            ))
        })?;
        lines.push(line);
    }
    std::fs::write(output_path, lines.join("\n"))?;
    Ok(())
}

pub fn export_summary(
    session_id: &str,
    records: &[TraceRecord],
    output_path: &Path,
) -> TraceResult<()> {
    let mut summary = String::new();

    summary.push_str("# VAC Session Audit Summary\n\n");
    summary.push_str(&format!("**Session ID:** `{}`\n\n", session_id));
    summary.push_str(&format!("**Total Records:** {}\n\n", records.len()));

    let tool_calls = records
        .iter()
        .filter(|r| matches!(r.record_type, RecordType::ToolCall))
        .count();
    let llm_requests = records
        .iter()
        .filter(|r| matches!(r.record_type, RecordType::LlmRequest))
        .count();
    let task_starts = records
        .iter()
        .filter(|r| matches!(r.record_type, RecordType::TaskStart))
        .count();
    let task_completes = records
        .iter()
        .filter(|r| matches!(r.record_type, RecordType::TaskComplete))
        .count();

    summary.push_str("## Statistics\n\n");
    summary.push_str("| Metric | Count |\n");
    summary.push_str("|--------|-------|\n");
    summary.push_str(&format!("| Task Starts | {} |\n", task_starts));
    summary.push_str(&format!("| Task Completions | {} |\n", task_completes));
    summary.push_str(&format!("| Tool Calls | {} |\n", tool_calls));
    summary.push_str(&format!("| LLM Requests | {} |\n\n", llm_requests));

    summary.push_str("## Timeline\n\n");
    summary.push_str("| Time | Type | Details |\n");
    summary.push_str("|------|------|---------|\n");

    for record in records {
        let time = record.timestamp.format("%H:%M:%S").to_string();
        let type_str = record_type_label(&record.record_type);
        let details = extract_details(&record.content);
        summary.push_str(&format!("| {} | {} | {} |\n", time, type_str, details));
    }

    std::fs::write(output_path, &summary)?;
    tracing::info!(path = %output_path.display(), "Markdown summary exported");
    Ok(())
}

fn record_type_label(rt: &RecordType) -> &'static str {
    match rt {
        RecordType::TaskStart => "TaskStart",
        RecordType::TaskComplete => "TaskComplete",
        RecordType::TaskFailed => "TaskFailed",
        RecordType::ToolCall => "ToolCall",
        RecordType::ToolResult => "ToolResult",
        RecordType::LlmRequest => "LlmRequest",
        RecordType::LlmResponse => "LlmResponse",
        RecordType::AgentMessage => "AgentMessage",
        RecordType::ContextRetrieval => "ContextRetrieval",
        RecordType::ValidationResult => "Validation",
        RecordType::PolicyDecision => "Policy",
        RecordType::LspStarted => "LspStarted",
        RecordType::LspDiagnosticsSnapshot => "LspDiagnostics",
        RecordType::LspPostEditRecheck => "LspRecheck",
        RecordType::SubagentSpawned => "SubagentSpawned",
        RecordType::SandboxCreated => "SandboxCreated",
        RecordType::SandboxDestroyed => "SandboxDestroyed",
        RecordType::PatchProposed => "PatchProposed",
        RecordType::PatchMerged => "PatchMerged",
        RecordType::Error => "Error",
    }
}

fn extract_details(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::Object(obj) => {
            if let Some(v) = obj.get("task_id") {
                format!("task: {v}")
            } else if let Some(v) = obj.get("tool") {
                format!("tool: {v}")
            } else if let Some(v) = obj.get("provider") {
                format!("provider: {v}")
            } else {
                "...".to_string()
            }
        }
        _ => "...".to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::recorder::{RecordType, TraceRecord};

    fn make_record(content: serde_json::Value) -> TraceRecord {
        TraceRecord {
            id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            record_type: RecordType::ToolCall,
            agent_id: None,
            content,
        }
    }

    #[test]
    fn export_jsonl_preserves_all_records() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.jsonl");
        let records: Vec<TraceRecord> = (0..5)
            .map(|i| make_record(serde_json::json!({ "n": i })))
            .collect();
        export_claude_jsonl(&records, &out).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert_eq!(content.lines().count(), 5, "all records must be exported");
    }

    #[test]
    fn export_vac_unsigned_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.vac");
        export_vac("sess-1", vec![], &out, None).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        let envelope: VacEnvelope = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(envelope.session_id, "sess-1");
    }

    #[test]
    fn export_vac_signed_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.vac");
        let kp = SigningKeyPair::generate();
        export_vac("sess-signed", vec![], &out, Some(&kp)).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        let envelope = VacEnvelope::verify(&bytes).unwrap();
        assert_eq!(envelope.session_id, "sess-signed");
    }

    #[test]
    fn export_vac_no_throwaway_signing_path() {
        // Confirm the old "sign=true, keypair=None → throwaway key" path is gone.
        // The new API makes `sign` an `Option<&SigningKeyPair>` — there is no bool.
        // This test documents the API contract by calling with None (unsigned).
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("unsigned.vac");
        export_vac("no-sign", vec![], &out, None).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        // Unsigned artifacts can be deserialized directly as VacEnvelope CBOR.
        let envelope: VacEnvelope = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(envelope.session_id, "no-sign");
    }
}
