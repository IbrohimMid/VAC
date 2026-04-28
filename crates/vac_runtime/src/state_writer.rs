use crate::scheduler::AutopilotStateFile;
use std::path::PathBuf;

pub fn write_state_atomic(path: PathBuf, state: AutopilotStateFile) {
    tokio::task::spawn_blocking(move || {
        if let Err(e) = write_state_atomic_sync(&path, &state) {
            tracing::error!(error = %e, path = %path.display(), "Failed to atomically write autopilot state");
        }
    });
}

pub fn write_state_atomic_sync(path: &PathBuf, state: &AutopilotStateFile) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let pid = std::process::id();
    let tmp_path = path.with_extension(format!("state.tmp.{pid}.{nonce}"));
    let mut f = std::fs::File::create(&tmp_path)?;
    f.write_all(json.as_bytes())?;
    f.sync_all()?;
    drop(f);

    std::fs::rename(&tmp_path, path)?;
    Ok(())
}
