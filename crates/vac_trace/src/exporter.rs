//! Export — package artifacts as ZIP with records, SBOM, and checksums.

use crate::error::{TraceError, TraceResult};
use crate::recorder::{RecordType, TraceRecord};
use crate::vac_format::{SigningKeyPair, VacEnvelope};
use std::path::Path;

pub fn export_vac(
    session_id: &str,
    records: Vec<TraceRecord>,
    output_path: &Path,
    sign: bool,
    keypair: Option<&SigningKeyPair>,
) -> TraceResult<()> {
    let envelope = VacEnvelope::new(session_id, records);

    let bytes = if sign {
        if let Some(kp) = keypair {
            envelope.sign(kp)?
        } else {
            tracing::info!("No keypair provided, generating one for signing");
            let kp = SigningKeyPair::generate();
            envelope.sign(&kp)?
        }
    } else {
        envelope.to_cbor()?
    };

    std::fs::write(output_path, &bytes)?;
    tracing::info!(path = %output_path.display(), size = bytes.len(), "VAC artifact exported");
    Ok(())
}

pub fn export_opencode_json(records: &[TraceRecord], output_path: &Path) -> TraceResult<()> {
    let json =
        serde_json::to_string_pretty(records).map_err(|e| TraceError::Export(e.to_string()))?;
    std::fs::write(output_path, json)?;
    Ok(())
}

pub fn export_claude_jsonl(records: &[TraceRecord], output_path: &Path) -> TraceResult<()> {
    let lines: Vec<String> = records
        .iter()
        .filter_map(|r| serde_json::to_string(r).ok())
        .collect();
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
    summary.push_str(&format!("| Metric | Count |\n"));
    summary.push_str(&format!("|--------|-------|\n"));
    summary.push_str(&format!("| Task Starts | {} |\n", task_starts));
    summary.push_str(&format!("| Task Completions | {} |\n", task_completes));
    summary.push_str(&format!("| Tool Calls | {} |\n", tool_calls));
    summary.push_str(&format!("| LLM Requests | {} |\n\n", llm_requests));

    summary.push_str("## Timeline\n\n");
    summary.push_str("| Time | Type | Details |\n");
    summary.push_str("|------|------|---------|\n");

    for record in records.iter().take(50) {
        let time = record.timestamp.format("%H:%M:%S").to_string();
        let type_str = match &record.record_type {
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
            RecordType::Error => "Error",
        };

        let details = match &record.content {
            serde_json::Value::Object(obj) => {
                if let Some(task_id) = obj.get("task_id") {
                    format!("task: {}", task_id)
                } else if let Some(tool) = obj.get("tool") {
                    format!("tool: {}", tool)
                } else if let Some(provider) = obj.get("provider") {
                    format!("provider: {}", provider)
                } else {
                    "...".to_string()
                }
            }
            _ => "...".to_string(),
        };

        summary.push_str(&format!("| {} | {} | {} |\n", time, type_str, details));
    }

    if records.len() > 50 {
        summary.push_str(&format!(
            "\n*... and {} more records*\n",
            records.len() - 50
        ));
    }

    std::fs::write(output_path, &summary)?;
    tracing::info!(path = %output_path.display(), "Markdown summary exported");
    Ok(())
}
