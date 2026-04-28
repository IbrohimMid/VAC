//! Optional SQLite-backed archive of dropped/old signal lines.
//!
//! OMNI keeps every raw terminal line in a rewind store so agents can
//! retrieve it later without paying the context cost up front. This module
//! provides the equivalent primitive for VAC, gated behind the `rewind`
//! feature to avoid a hard SQLite dependency in default builds.

use std::path::Path;

use rusqlite::{Connection, params};
use thiserror::Error;

use crate::buffer::{SignalLine, SignalStreamKind};

#[derive(Debug, Error)]
pub enum RewindError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Append-only archive. One DB per session is the intended usage pattern.
pub struct RewindStore {
    conn: Connection,
}

impl RewindStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RewindError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS signal_lines (
                stream_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                seq INTEGER NOT NULL,
                captured_at INTEGER NOT NULL,
                text TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS signal_lines_stream_idx
                ON signal_lines(stream_id, seq);
            "#,
        )?;
        Ok(Self { conn })
    }

    pub fn append(
        &mut self,
        stream_id: &str,
        kind: SignalStreamKind,
        line: &SignalLine,
        captured_at_epoch_s: i64,
    ) -> Result<(), RewindError> {
        self.conn.execute(
            "INSERT INTO signal_lines(stream_id, kind, seq, captured_at, text) VALUES (?, ?, ?, ?, ?)",
            params![
                stream_id,
                serde_json::to_string(&kind).unwrap_or_else(|_| "\"other\"".into()),
                line.seq as i64,
                captured_at_epoch_s,
                line.text
            ],
        )?;
        Ok(())
    }

    /// Return all distinct stream ids present in the store, sorted.
    pub fn list_streams(&self) -> Result<Vec<String>, RewindError> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT stream_id FROM signal_lines ORDER BY stream_id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn recent(&self, stream_id: &str, limit: i64) -> Result<Vec<SignalLine>, RewindError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, text FROM signal_lines WHERE stream_id = ? ORDER BY seq DESC LIMIT ?",
        )?;
        let rows = stmt.query_map(params![stream_id, limit], |row| {
            Ok(SignalLine {
                seq: row.get::<_, i64>(0)? as u64,
                text: row.get(1)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        out.reverse();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_recall() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("rewind.db");
        let mut store = RewindStore::open(&db).unwrap();
        for i in 0..5u64 {
            store
                .append(
                    "shell-1",
                    SignalStreamKind::Shell,
                    &SignalLine {
                        seq: i,
                        text: format!("line {i}"),
                    },
                    1_700_000_000 + i as i64,
                )
                .unwrap();
        }
        let got = store.recent("shell-1", 3).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got.last().unwrap().text, "line 4");
    }
}
