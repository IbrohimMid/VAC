//! Export — package artifacts as ZIP with records, SBOM, and checksums.

use crate::error::{TraceError, TraceResult};
use crate::recorder::TraceRecord;
use crate::vac_format::VacEnvelope;
use std::path::Path;

pub fn export_vac(
    session_id: &str,
    records: Vec<TraceRecord>,
    output_path: &Path,
    sign: bool,
) -> TraceResult<()> {
    let envelope = VacEnvelope::new(session_id, records);

    let bytes = if sign {
        envelope.sign(&[])?
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
