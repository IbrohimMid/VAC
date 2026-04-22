//! `vac signal` — inspect rewind databases produced by `vac_signal`.

use std::path::PathBuf;

use crate::SignalCommand;

pub async fn dispatch(
    project_root: PathBuf,
    format: &str,
    cmd: SignalCommand,
) -> anyhow::Result<()> {
    match cmd {
        SignalCommand::List => list(project_root, format),
        SignalCommand::Tail { db_path, stream, n } => tail(db_path, stream, n, format),
    }
}

fn list(project_root: PathBuf, format: &str) -> anyhow::Result<()> {
    let dir = project_root.join(".vac").join("signal");
    let mut entries: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("db") {
                entries.push(p);
            }
        }
    }
    entries.sort();

    if format == "json" {
        return crate::output::print_json(&serde_json::json!({ "databases": entries }));
    }
    if entries.is_empty() {
        println!("No signal rewind databases found under {}.", dir.display());
        return Ok(());
    }
    println!("Signal rewind databases ({}):", dir.display());
    for p in entries {
        println!("  {}", p.display());
    }
    Ok(())
}

#[cfg(feature = "signal-rewind")]
fn tail(db_path: PathBuf, stream: String, n: i64, format: &str) -> anyhow::Result<()> {
    let store = vac_signal::rewind::RewindStore::open(&db_path)?;
    let lines = store.recent(&stream, n)?;
    if format == "json" {
        return crate::output::print_json(&serde_json::json!({
            "db": db_path,
            "stream": stream,
            "lines": lines.iter().map(|l| serde_json::json!({
                "seq": l.seq,
                "text": l.text,
            })).collect::<Vec<_>>(),
        }));
    }
    println!("Tail [{}] from {} ({} lines):", stream, db_path.display(), lines.len());
    for line in lines {
        println!("  [{:>6}] {}", line.seq, line.text);
    }
    Ok(())
}

#[cfg(not(feature = "signal-rewind"))]
fn tail(_db_path: PathBuf, _stream: String, _n: i64, _format: &str) -> anyhow::Result<()> {
    anyhow::bail!(
        "vac signal tail requires the `signal-rewind` feature. \
         Rebuild with: cargo build -p vac_cli --features signal-rewind"
    );
}
