use std::path::PathBuf;

pub async fn execute(
    project_root: PathBuf,
    input: PathBuf,
    format: String,
    require_signed: bool,
    overwrite_session: bool,
    trust_approvals: bool,
    redact: bool,
) -> anyhow::Result<()> {
    println!("📥 Importing session...");
    println!("   Format: {}", format);
    println!("   Input: {}", input.display());

    match format.as_str() {
        "bundle-json" | "bundle" => {
            if require_signed {
                println!("   WARNING: signed bundle required; unsigned imports will be rejected.");
            }
            if overwrite_session {
                println!(
                    "   WARNING: overwrite-session enabled; existing session state may be replaced."
                );
            }
            if trust_approvals {
                println!(
                    "   WARNING: trust-approvals enabled; imported approvals will be restored."
                );
            }
            if !redact {
                println!("   WARNING: redaction disabled; imported secrets will be preserved.");
            }

            let session_id = vac_core::bundle::import_bundle_from_path_with_options(
                &project_root,
                &input,
                vac_core::bundle::BundleImportOptions {
                    require_signed,
                    overwrite_session,
                    trust_approvals,
                    redact_on_import: redact,
                },
            )?;
            println!("   ✓ Imported bundle (session_id={})", session_id);
        }
        _ => {
            anyhow::bail!("Unknown format: {}. Use: bundle-json", format);
        }
    }

    Ok(())
}
