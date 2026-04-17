use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, input: PathBuf, format: String) -> anyhow::Result<()> {
    println!("📥 Importing session...");
    println!("   Format: {}", format);
    println!("   Input: {}", input.display());

    match format.as_str() {
        "bundle-json" | "bundle" => {
            let session_id = vac_core::bundle::import_bundle_from_path(&project_root, &input)?;
            println!("   ✓ Imported bundle (session_id={})", session_id);
        }
        _ => {
            anyhow::bail!("Unknown format: {}. Use: bundle-json", format);
        }
    }

    Ok(())
}
