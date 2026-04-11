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

    // Create output directory
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Export based on format
    match format.as_str() {
        "vac-cbor" => {
            println!("   💾 Exporting as VAC CBOR...");
            // TODO: vac_trace::export_vac_cbor(&session, &output_path, sign)?;
            println!("   (VAC CBOR export implementation pending)");
        }
        "opencode-json" => {
            let json = serde_json::to_string_pretty(&session)?;
            std::fs::write(&output_path, json)?;
            println!("   ✓ Exported as OpenCode JSON");
        }
        "claude-jsonl" => {
            println!("   (Claude JSONL export implementation pending)");
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
