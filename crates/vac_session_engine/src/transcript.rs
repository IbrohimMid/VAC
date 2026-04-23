//! Transcript durability primitive.
//!
//! Implements the "transcript-before-query" pattern from Claude Code:
//! a stub transcript entry is written to disk BEFORE the LLM is
//! contacted. If the process dies between `Accepted` and `Finished`,
//! the next boot can inspect the transcript and resume.
//!
//! Physical layout: one JSONL file per session at
//! `<root>/.vac/sessions/<session_id>.jsonl`. Each line is one
//! [`TranscriptEntry`]. Append-only; compact boundary writes a single
//! `kind = "compact_boundary"` row rather than rewriting history.
//!
//! ### Concurrency
//!
//! Each [`TranscriptHandle`] owns its `File` under a `tokio::sync::Mutex`,
//! so concurrent `append` calls serialize cleanly and never interleave
//! JSONL lines — O_APPEND atomicity on POSIX is only guaranteed below
//! `PIPE_BUF` (4 KiB) and not guaranteed at all on Windows. The mutex
//! is per-session; different sessions do not contend.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};

/// Kinds of transcript rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TranscriptKind {
    /// Stub row written synchronously before LLM contact — durability
    /// checkpoint.
    Accepted,
    /// Slash command handled locally — no LLM round-trip.
    Slash,
    /// LLM request issued.
    LlmRequest,
    /// LLM streamed chunk aggregated at finish.
    LlmResponse,
    /// Tool call request emitted by LLM.
    ToolCall,
    /// Tool call result.
    ToolResult,
    /// Compact boundary marker.
    CompactBoundary,
    /// Submit finished cleanly.
    Finished,
    /// Submit aborted.
    Aborted,
}

/// One row in a transcript JSONL file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub id: Uuid,
    pub session_id: Uuid,
    pub kind: TranscriptKind,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub content: serde_json::Value,
}

impl TranscriptEntry {
    pub fn new(
        session_id: Uuid,
        kind: TranscriptKind,
        content: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            kind,
            timestamp: chrono::Utc::now(),
            content,
        }
    }
}

/// Opaque handle held by the engine + drivers. Every append is fsynced
/// so "transcript-before-query" gives real durability (not just
/// buffered). The file handle is cached under a mutex so concurrent
/// appends from multiple tasks serialize cleanly.
#[derive(Debug, Clone)]
pub struct TranscriptHandle {
    path: PathBuf,
    session_id: Uuid,
    file: Arc<Mutex<File>>,
}

impl TranscriptHandle {
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Constructs + owns transcript files. One writer per project root;
/// handles are per-session.
#[derive(Debug, Clone)]
pub struct TranscriptWriter {
    root: PathBuf,
}

impl TranscriptWriter {
    /// `root` is the project root; the writer puts JSONL files under
    /// `<root>/.vac/sessions/`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join(".vac").join("sessions")
    }

    /// Open (or create) the transcript for a session. On first creation
    /// the parent directory is fsynced so the file's directory entry
    /// survives a post-`open` crash.
    pub async fn open(&self, session_id: Uuid) -> EngineResult<TranscriptHandle> {
        let dir = self.sessions_dir();
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(format!("{session_id}.jsonl"));
        let fresh = !tokio::fs::try_exists(&path).await.unwrap_or(false);
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        if fresh {
            // fsync the directory so the new file's entry is durable.
            // Best effort: some filesystems/platforms reject fsync on
            // dirs — ignore those errors rather than failing the open.
            if let Ok(dir_f) = tokio::fs::File::open(&dir).await {
                let _ = dir_f.sync_all().await;
            }
        }
        Ok(TranscriptHandle {
            path,
            session_id,
            file: Arc::new(Mutex::new(file)),
        })
    }

    /// Append one entry to a handle. Syncs to disk so a crash right
    /// after this call still preserves the entry. Concurrent callers on
    /// the same handle serialize through the handle's mutex.
    pub async fn append(
        &self,
        handle: &TranscriptHandle,
        entry: &TranscriptEntry,
    ) -> EngineResult<()> {
        let mut line = serde_json::to_vec(entry)?;
        line.push(b'\n');
        let mut f = handle.file.lock().await;
        f.write_all(&line).await?;
        f.flush().await?;
        f.sync_data().await?;
        Ok(())
    }

    /// Read a transcript from disk. Returns entries in append order.
    /// Used for resume/replay flows.
    pub async fn read(&self, session_id: Uuid) -> EngineResult<Vec<TranscriptEntry>> {
        let path = self.sessions_dir().join(format!("{session_id}.jsonl"));
        let file = match tokio::fs::File::open(&path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();
        let mut buf = String::new();
        let mut idx = 0usize;
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await?;
            if n == 0 {
                break;
            }
            let line = buf.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                idx += 1;
                continue;
            }
            let entry: TranscriptEntry = serde_json::from_str(line).map_err(|e| {
                EngineError::Transcript(format!("line {idx}: {e}"))
            })?;
            entries.push(entry);
            idx += 1;
        }
        Ok(entries)
    }

    /// Inspect a session for the "crashed mid-submit" signature: a
    /// trailing `Accepted` with no `Finished`/`Aborted` following it.
    /// Returns `Some(entry_id)` if recovery is needed.
    ///
    /// Streams the file line-by-line rather than slurping into memory,
    /// so cost is O(lines) rather than O(filesize) in RSS.
    pub async fn last_pending_submit(
        &self,
        session_id: Uuid,
    ) -> EngineResult<Option<Uuid>> {
        let path = self.sessions_dir().join(format!("{session_id}.jsonl"));
        let file = match tokio::fs::File::open(&path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut reader = BufReader::new(file);
        let mut last_accepted: Option<Uuid> = None;
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await?;
            if n == 0 {
                break;
            }
            let line = buf.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                continue;
            }
            // Parse only the fields we need. Full TranscriptEntry is
            // fine here — entries are small — but stay defensive.
            let entry: TranscriptEntry = match serde_json::from_str(line) {
                Ok(e) => e,
                Err(e) => {
                    return Err(EngineError::Transcript(format!("parse: {e}")));
                }
            };
            match entry.kind {
                TranscriptKind::Accepted => last_accepted = Some(entry.id),
                TranscriptKind::Finished | TranscriptKind::Aborted => {
                    last_accepted = None
                }
                _ => {}
            }
        }
        Ok(last_accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_creates_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let sid = Uuid::new_v4();
        let h = w.open(sid).await.unwrap();
        assert_eq!(h.session_id(), sid);
        assert!(h.path().exists());
    }

    #[tokio::test]
    async fn append_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let sid = Uuid::new_v4();
        let h = w.open(sid).await.unwrap();
        let e = TranscriptEntry::new(
            sid,
            TranscriptKind::Accepted,
            serde_json::json!({ "input": "hello" }),
        );
        w.append(&h, &e).await.unwrap();
        w.append(
            &h,
            &TranscriptEntry::new(sid, TranscriptKind::Finished, serde_json::json!({})),
        )
        .await
        .unwrap();
        let back = w.read(sid).await.unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].kind, TranscriptKind::Accepted);
        assert_eq!(back[1].kind, TranscriptKind::Finished);
    }

    #[tokio::test]
    async fn last_pending_submit_detects_crashed_session() {
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let sid = Uuid::new_v4();
        let h = w.open(sid).await.unwrap();
        let a = TranscriptEntry::new(sid, TranscriptKind::Accepted, serde_json::json!({}));
        w.append(&h, &a).await.unwrap();
        let pending = w.last_pending_submit(sid).await.unwrap();
        assert_eq!(pending, Some(a.id));
    }

    #[tokio::test]
    async fn last_pending_submit_none_after_finish() {
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let sid = Uuid::new_v4();
        let h = w.open(sid).await.unwrap();
        w.append(
            &h,
            &TranscriptEntry::new(sid, TranscriptKind::Accepted, serde_json::json!({})),
        )
        .await
        .unwrap();
        w.append(
            &h,
            &TranscriptEntry::new(sid, TranscriptKind::Finished, serde_json::json!({})),
        )
        .await
        .unwrap();
        assert!(w.last_pending_submit(sid).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn read_unknown_session_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let entries = w.read(Uuid::new_v4()).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn concurrent_appends_do_not_interleave_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let sid = Uuid::new_v4();
        let h = w.open(sid).await.unwrap();
        // Large payload >4 KiB to stress the POSIX O_APPEND boundary.
        let big = "x".repeat(8 * 1024);
        let mut tasks = Vec::new();
        for i in 0..8usize {
            let w = w.clone();
            let h = h.clone();
            let big = big.clone();
            tasks.push(tokio::spawn(async move {
                for _ in 0..4 {
                    let e = TranscriptEntry::new(
                        sid,
                        TranscriptKind::LlmResponse,
                        serde_json::json!({ "i": i, "blob": big }),
                    );
                    w.append(&h, &e).await.unwrap();
                }
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        // Every line must parse cleanly — no interleaving.
        let entries = w.read(sid).await.unwrap();
        assert_eq!(entries.len(), 8 * 4);
    }
}
