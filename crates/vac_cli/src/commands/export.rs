//! `vac export` — Export session as VAC artifact.

use std::path::PathBuf;

pub async fn execute(
    project_root: PathBuf,
    output: Option<PathBuf>,
    format: String,
    sign: bool,
) -> anyhow::Result<()> {
    let output_path = output.unwrap_or_else(|| {
        project_root.join(format!(
            ".vac/exports/session.{}",
            match format.as_str() {
                "opencode-json" => "json",
                "claude-jsonl" => "jsonl",
                _ => "vac",
            }
        ))
    });

    println!("📦 Exporting session...");
    println!("   Format: {}", format);
    println!("   Output: {}", output_path.display());
    println!("   Signing: {}", if sign { "enabled" } else { "disabled" });

    // Load session
    let session = vac_core::Session::load_latest(&project_root)?
        .ok_or_else(|| anyhow::anyhow!("No session found. Run a task first."))?;

    // Load trace records if available
    let trace_path = project_root.join(".vac/traces").join(format!("{}.json", session.id));
    let records = if trace_path.exists() {
        let content = std::fs::read_to_string(&trace_path)?;
        serde_json::from_str::<Vec<vac_trace::recorder::TraceRecord>>(&content)?
    } else {
        println!("   ⚠️  No trace records found for this session.");
        Vec::new()
    };

    // Create output directory
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Export based on format
    match format.as_str() {
        "vac-cbor" => {
            println!("   💾 Exporting as VAC CBOR...");
            vac_trace::exporter::export_vac(
                &session.id.to_string(),
                records,
                &output_path,
                sign,
                None,
            ).map_err(|e| anyhow::anyhow!("Failed to export VAC CBOR: {}", e))?;
            println!("   ✓ Exported as VAC CBOR");
        }
        "opencode-json" => {
            let json = serde_json::to_string_pretty(&session)?;
            std::fs::write(&output_path, json)?;
            println!("   ✓ Exported as OpenCode JSON");
        }
        "claude-jsonl" => {
            println!("   🤖 Exporting as Claude JSONL...");
            vac_trace::exporter::export_claude_jsonl(&records, &output_path)
                .map_err(|e| anyhow::anyhow!("Failed to export Claude JSONL: {}", e))?;
            println!("   ✓ Exported as Claude JSONL");
        }
        _ => {
            anyhow::bail!(
                "Unknown format: {}. Use: vac-cbor, opencode-json, claude-jsonl",
                format
            );
        }
    }

    println!("\n✓ Export complete: {}", output_path.display());
    Ok(())
}
