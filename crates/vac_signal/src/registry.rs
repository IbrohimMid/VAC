//! Name-indexed registry of [`SignalBuffer`]s.
//!
//! Many VAC subsystems produce noisy output streams (shell sessions,
//! `vil dev`, runtime jobs, MCP). `SignalRegistry` is an in-memory lookup
//! by stream id, so a single piece of code — MCP retrieval tools, a TUI
//! signal panel, trajectory exporters — can iterate or recall buffers
//! without knowing about every subsystem.
//!
//! The registry does not own the buffers; callers pass `&SignalBuffer`
//! references into a borrow-only view, because the actual buffers live in
//! state owned by the subsystem that produces them (e.g. `ShellSession`,
//! `VilDevState`). This avoids double-bookkeeping and keeps lifetime rules
//! simple.

use std::collections::BTreeMap;

use crate::buffer::{SignalBuffer, SignalStreamKind};
use crate::distill::DistilledView;

/// Borrowing view over a set of named signal buffers.
///
/// Construct with [`SignalRegistry::new`] then [`register`](Self::register)
/// each buffer before reading. Intended for short-lived snapshots, not
/// long-lived storage.
pub struct SignalRegistry<'a> {
    entries: BTreeMap<String, &'a SignalBuffer>,
}

impl<'a> SignalRegistry<'a> {
    pub fn new() -> Self {
        Self { entries: BTreeMap::new() }
    }

    pub fn register(&mut self, id: impl Into<String>, buf: &'a SignalBuffer) -> &mut Self {
        self.entries.insert(id.into(), buf);
        self
    }

    pub fn get(&self, id: &str) -> Option<&SignalBuffer> {
        self.entries.get(id).copied()
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Per-stream summary for overview surfaces.
    pub fn summary(&self) -> Vec<RegistrySummary> {
        self.entries
            .iter()
            .map(|(id, buf)| RegistrySummary {
                id: id.clone(),
                kind: buf.kind(),
                lines: buf.len(),
                dropped: buf.dropped(),
            })
            .collect()
    }

    /// Distill a specific stream with default heuristics.
    pub fn distill(&self, id: &str, tail_size: usize) -> Option<DistilledView> {
        self.get(id).map(|b| b.distilled_default(tail_size))
    }

    /// Persist every registered buffer into a [`RewindStore`][crate::rewind::RewindStore].
    /// Appends each line with the current wall-clock timestamp. Useful for
    /// session teardown or a manual "snapshot to disk" operation.
    ///
    /// Only builds with the `rewind` feature. Callers hold the lock on the
    /// store.
    #[cfg(feature = "rewind")]
    pub fn persist_to_rewind(
        &self,
        store: &mut crate::rewind::RewindStore,
    ) -> Result<usize, crate::rewind::RewindError> {
        let now = chrono::Utc::now().timestamp();
        let mut count = 0;
        for (id, buf) in &self.entries {
            for line in buf.iter() {
                store.append(id, buf.kind(), line, now)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

impl<'a> Default for SignalRegistry<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RegistrySummary {
    pub id: String,
    pub kind: SignalStreamKind,
    pub lines: usize,
    pub dropped: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_summary() {
        let mut a = SignalBuffer::new(SignalStreamKind::Shell, 10);
        a.push_line("hello");
        a.push_line("Error: boom");
        let b = SignalBuffer::new(SignalStreamKind::VilDev, 10);

        let mut reg = SignalRegistry::new();
        reg.register("shell-0", &a).register("vil_dev", &b);

        assert_eq!(reg.len(), 2);
        let ids: Vec<_> = reg.ids().collect();
        assert_eq!(ids, vec!["shell-0", "vil_dev"]);

        let summary = reg.summary();
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].lines, 2);
        assert_eq!(summary[1].lines, 0);
    }

    #[cfg(feature = "rewind")]
    #[test]
    fn persist_to_rewind_writes_every_line() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("rewind.db");
        let mut store = crate::rewind::RewindStore::open(&db).unwrap();

        let mut a = SignalBuffer::new(SignalStreamKind::Shell, 10);
        a.push_line("hello");
        a.push_line("world");
        let mut b = SignalBuffer::new(SignalStreamKind::VilDev, 10);
        b.push_line("Error: boom");

        let mut reg = SignalRegistry::new();
        reg.register("shell-0", &a).register("vil_dev", &b);
        let count = reg.persist_to_rewind(&mut store).unwrap();
        assert_eq!(count, 3);

        let recalled = store.recent("shell-0", 10).unwrap();
        assert_eq!(recalled.len(), 2);
    }

    #[test]
    fn distill_via_registry() {
        let mut buf = SignalBuffer::new(SignalStreamKind::Shell, 10);
        buf.push_line("ok");
        buf.push_line("Error: failed");
        let mut reg = SignalRegistry::new();
        reg.register("s", &buf);

        let view = reg.distill("s", 2).expect("present");
        assert!(view.key_lines.iter().any(|l| l.contains("Error")));
        assert_eq!(view.tail.len(), 2);

        assert!(reg.distill("missing", 2).is_none());
    }
}
