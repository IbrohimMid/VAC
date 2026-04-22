use anyhow::Result;
use std::path::Path;

pub async fn collect_recent_trajectories(root: &Path, limit: usize) -> Result<Vec<String>> {
    vac_trajectory::collect_recent_labels(root, limit).await
}
