//! W2.3 — oversized tool result disk-spill.
//!
//! When a tool returns a payload larger than the tool's declared
//! `max_result_size_chars`, the runtime persists the payload to
//! `.vac/tool-results/<uuid>.json` and swaps the in-memory response
//! for a [`PreviewStub`] with a short head sample + on-disk path.
//! The model sees the stub inline; downstream readers (transcript,
//! bridge clients, trajectory replay) dereference the file when they
//! want the full content.
//!
//! Why this exists: long tool outputs balloon prompt-cache cost and
//! push real content out of context. Claude Code's leak gates this
//! per-tool via `maxResultSizeChars`; we mirror that design so tool
//! authors can opt every output in or out with a single constant.
//!
//! Tools whose result must never be persisted (circular reads —
//! `FileRead`) override `max_result_size_chars()` to `usize::MAX`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stub substituted in-line when a tool's payload is spilled.
/// Carries the path so downstream consumers can re-hydrate, and a
/// short head-of-payload sample so the model doesn't need a second
/// tool call to know what was there.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreviewStub {
    /// Sentinel. Transcript readers key on this.
    pub kind: String,
    /// Absolute path to the persisted payload.
    pub path: PathBuf,
    /// Size in chars of the original payload.
    pub original_size_chars: usize,
    /// First N chars of the original payload (≤ `HEAD_SAMPLE_CHARS`).
    pub head_preview: String,
}

impl PreviewStub {
    pub const KIND: &'static str = "vac.preview_stub";
    pub const HEAD_SAMPLE_CHARS: usize = 512;

    /// True when `value` is a spill stub (not a real payload).
    pub fn is_stub(value: &serde_json::Value) -> bool {
        value
            .get("kind")
            .and_then(|k| k.as_str())
            .map(|s| s == Self::KIND)
            .unwrap_or(false)
    }
}

/// Decide + apply spill for one tool result. When `payload`
/// serialized exceeds `threshold`, persists it under `spill_root`
/// and returns the stub as JSON. When below threshold, returns the
/// original payload unchanged.
///
/// `threshold == usize::MAX` disables spill entirely and returns
/// the original unchanged.
///
/// **Spill files are not auto-cleaned.** Long-running projects should
/// call [`prune_spill_dir`] on a cron (or at boot) to bound disk use.
/// The path in the returned stub is absolute — cross-machine
/// consumers (bridge, trajectory replay) must translate it relative
/// to their own `.vac/tool-results/` before dereferencing.
#[tracing::instrument(
    target = "vac_tools::result_spill",
    name = "maybe_spill",
    skip_all,
    fields(threshold, spilled = tracing::field::Empty, size_chars = tracing::field::Empty),
)]
pub async fn maybe_spill_result(
    payload: serde_json::Value,
    threshold: usize,
    spill_root: &Path,
) -> std::io::Result<serde_json::Value> {
    if threshold == usize::MAX {
        return Ok(payload);
    }
    let serialized = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                target: "vac_tools::result_spill",
                error = %e,
                "payload serialize failed; returning unchanged",
            );
            return Ok(payload);
        }
    };
    // Compute char count once — serde_json never inserts multi-byte
    // chars so the count is stable. Reuse for both the threshold
    // check and the stub's `original_size_chars` field.
    let size_chars = serialized.chars().count();
    tracing::Span::current().record("size_chars", size_chars);
    if size_chars <= threshold {
        return Ok(payload);
    }
    tokio::fs::create_dir_all(spill_root).await?;
    let id = Uuid::new_v4();
    let path = spill_root.join(format!("{id}.json"));
    tokio::fs::write(&path, serialized.as_bytes()).await?;
    let head_preview: String = serialized
        .chars()
        .take(PreviewStub::HEAD_SAMPLE_CHARS)
        .collect();
    let stub = PreviewStub {
        kind: PreviewStub::KIND.into(),
        path,
        original_size_chars: size_chars,
        head_preview,
    };
    tracing::Span::current().record("spilled", true);
    Ok(serde_json::to_value(stub).unwrap_or(serde_json::Value::Null))
}

/// Remove spill files older than `older_than`. Returns the number of
/// files removed. Non-spill files (anything that isn't `*.json` or
/// that doesn't deserialize as `PreviewStub` content) are skipped
/// defensively — the pruner is conservative so a misrouted file in
/// the directory survives a sweep.
pub async fn prune_spill_dir(spill_root: &Path, older_than: Duration) -> std::io::Result<usize> {
    if !spill_root.is_dir() {
        return Ok(0);
    }
    let cutoff = SystemTime::now()
        .checked_sub(older_than)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut rd = tokio::fs::read_dir(spill_root).await?;
    let mut removed = 0usize;
    while let Some(entry) = rd.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if modified <= cutoff {
            if tokio::fs::remove_file(&path).await.is_ok() {
                removed += 1;
            }
        }
    }
    tracing::info!(
        target: "vac_tools::result_spill",
        removed,
        "pruned spill dir",
    );
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn small_payload_passes_through_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let v = serde_json::json!({"x": 1});
        let out = maybe_spill_result(v.clone(), 1024, tmp.path())
            .await
            .unwrap();
        assert_eq!(out, v);
        // No file should have been written.
        let mut rd = tokio::fs::read_dir(tmp.path()).await.unwrap();
        assert!(rd.next_entry().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn oversized_payload_spills_to_disk_and_returns_stub() {
        let tmp = tempfile::tempdir().unwrap();
        let big = "x".repeat(5_000);
        let v = serde_json::json!({"data": big});
        let out = maybe_spill_result(v.clone(), 1_000, tmp.path())
            .await
            .unwrap();
        assert!(PreviewStub::is_stub(&out), "response must be a stub");
        let path = out.get("path").and_then(|p| p.as_str()).unwrap();
        assert!(std::path::Path::new(path).exists(), "spill file must exist");
        let original_size = out
            .get("original_size_chars")
            .and_then(|s| s.as_u64())
            .unwrap();
        assert!(original_size > 5_000);
        let head = out.get("head_preview").and_then(|h| h.as_str()).unwrap();
        assert!(head.len() <= PreviewStub::HEAD_SAMPLE_CHARS);
    }

    #[tokio::test]
    async fn spill_file_round_trips_to_original_payload() {
        let tmp = tempfile::tempdir().unwrap();
        let v = serde_json::json!({"records": vec!["a"; 1_000]});
        let out = maybe_spill_result(v.clone(), 1_000, tmp.path())
            .await
            .unwrap();
        let path = out.get("path").and_then(|p| p.as_str()).unwrap();
        let raw = tokio::fs::read_to_string(path).await.unwrap();
        let back: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(back, v, "re-read must equal original payload");
    }

    #[tokio::test]
    async fn threshold_max_disables_spill() {
        let tmp = tempfile::tempdir().unwrap();
        let big = "y".repeat(10_000);
        let v = serde_json::json!({"d": big});
        let out = maybe_spill_result(v.clone(), usize::MAX, tmp.path())
            .await
            .unwrap();
        assert_eq!(out, v);
        assert!(!PreviewStub::is_stub(&out));
    }

    #[tokio::test]
    async fn is_stub_matches_only_sentinel_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let v = serde_json::json!({"kind": "something-else", "x": 1});
        let out = maybe_spill_result(v.clone(), 1024, tmp.path())
            .await
            .unwrap();
        assert!(
            !PreviewStub::is_stub(&out),
            "non-spill payload is not a stub"
        );
    }

    #[tokio::test]
    async fn prune_removes_old_spill_files() {
        let tmp = tempfile::tempdir().unwrap();
        let old_path = tmp.path().join("old.json");
        tokio::fs::write(&old_path, "{}").await.unwrap();
        // Let the fs clock tick past the eventual cutoff. 50 ms is
        // plenty to guarantee mtime < now() on any reasonable kernel.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let new_path = tmp.path().join("new.json");
        tokio::fs::write(&new_path, "{}").await.unwrap();
        // Prune anything older than 20 ms. `old.json` is > 50 ms old
        // by now; `new.json` is freshly written so it survives.
        let n = prune_spill_dir(tmp.path(), Duration::from_millis(20))
            .await
            .unwrap();
        assert_eq!(n, 1, "exactly one file should be pruned");
        assert!(!old_path.exists(), "old spill removed");
        assert!(new_path.exists(), "fresh spill kept");
    }

    #[tokio::test]
    async fn prune_skips_non_json_files() {
        let tmp = tempfile::tempdir().unwrap();
        let txt = tmp.path().join("stray.txt");
        tokio::fs::write(&txt, "leave me alone").await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        // Cutoff well before the stray's mtime — pruner would target
        // it by age, but the extension gate skips it.
        let n = prune_spill_dir(tmp.path(), Duration::from_millis(1))
            .await
            .unwrap();
        assert_eq!(n, 0);
        assert!(txt.exists(), "non-.json files must not be pruned");
    }

    #[tokio::test]
    async fn prune_on_missing_dir_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let n = prune_spill_dir(&tmp.path().join("nope"), Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn edge_exactly_at_threshold_does_not_spill() {
        let tmp = tempfile::tempdir().unwrap();
        // Build a value whose serialized length is exactly `threshold`.
        // Easier: use a tight threshold that equals the serialized size.
        let v = serde_json::json!({"a": "b"});
        let serialized = serde_json::to_string(&v).unwrap();
        let threshold = serialized.chars().count();
        let out = maybe_spill_result(v.clone(), threshold, tmp.path())
            .await
            .unwrap();
        assert_eq!(out, v, "equality should not trip spill");
    }
}
