use anyhow::Result;
use std::path::PathBuf;
use tracing::info;
use vac_ingest::{bm25::Bm25Index, build_file_index};

pub async fn execute(project_root: PathBuf) -> Result<()> {
    let index_path = project_root.join(".vac").join("bm25.index");

    // Ensure .vac directory exists
    let vac_dir = project_root.join(".vac");
    if !vac_dir.exists() {
        std::fs::create_dir_all(&vac_dir)?;
    }

    println!("Building file index for {}...", project_root.display());
    let files = build_file_index(&project_root, 50_000).await?;
    println!("Found {} files. Tokenizing corpus...", files.len());

    let index = Bm25Index::build(&files);
    index.write_to_file(&index_path)?;

    println!(
        "Successfully wrote persistent BM25 index to {}",
        index_path.display()
    );
    info!("Ingest command completed successfully.");
    Ok(())
}
