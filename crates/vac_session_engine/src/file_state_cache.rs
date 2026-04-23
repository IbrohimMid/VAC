//! W1.3 — file-state cache with fork/merge semantics.
//!
//! The cache tracks "we have read this file at this mtime/size; here
//! is the summary we derived". Fork speculation needs to read files
//! without racing the parent — so the cache supports **fork-then-merge**:
//! a fork sees a snapshot of the parent's entries plus its own
//! additions, and on accept the additions merge back into the parent.
//! On reject (guard dropped without accept) the fork's overlay dir is
//! GC'd and the parent's cache is untouched.
//!
//! Bounded via a simple LRU — the oldest *touched* entry evicts when
//! size exceeds capacity. Claude Code's leak uses
//! `READ_FILE_STATE_CACHE_SIZE` for the same guard; we default to
//! 256 entries which matches typical working-set size for a day of
//! code review.

use std::collections::HashMap;
use std::path::PathBuf;

/// One cached entry — what we observed when reading the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStateEntry {
    pub mtime_unix: i64,
    pub size_bytes: u64,
    /// Monotonic touch counter — the largest value is the most-recently
    /// accessed entry; LRU evicts the smallest.
    pub last_touched: u64,
}

impl FileStateEntry {
    pub fn new(mtime_unix: i64, size_bytes: u64) -> Self {
        Self {
            mtime_unix,
            size_bytes,
            last_touched: 0,
        }
    }
}

/// Read-file state cache. Thread-safe wrapping is the caller's job —
/// in practice one lives per session behind an `RwLock`.
#[derive(Debug, Clone)]
pub struct FileStateCache {
    entries: HashMap<PathBuf, FileStateEntry>,
    capacity: usize,
    /// Monotonic counter used to stamp `last_touched` on insert/touch.
    tick: u64,
}

pub const DEFAULT_FILE_STATE_CAPACITY: usize = 256;

impl Default for FileStateCache {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_FILE_STATE_CAPACITY)
    }
}

impl FileStateCache {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "FileStateCache capacity must be > 0");
        Self {
            entries: HashMap::with_capacity(capacity),
            capacity,
            tick: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Record an observation. If the path already exists, the entry
    /// is replaced and touch-counter bumped (fresh read wins).
    pub fn record(&mut self, path: PathBuf, mut entry: FileStateEntry) {
        self.tick = self.tick.saturating_add(1);
        entry.last_touched = self.tick;
        self.entries.insert(path, entry);
        self.evict_if_needed();
    }

    /// Mark an entry as touched without modifying its observation —
    /// moves it to the front of the LRU order.
    pub fn touch(&mut self, path: &PathBuf) -> bool {
        self.tick = self.tick.saturating_add(1);
        if let Some(e) = self.entries.get_mut(path) {
            e.last_touched = self.tick;
            true
        } else {
            false
        }
    }

    pub fn get(&self, path: &PathBuf) -> Option<&FileStateEntry> {
        self.entries.get(path)
    }

    fn evict_if_needed(&mut self) {
        while self.entries.len() > self.capacity {
            let Some((oldest, _)) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_touched)
                .map(|(p, e)| (p.clone(), e.clone()))
            else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }

    /// W1.3 — Produce a fork snapshot scoped to `overlay_dir`. The
    /// fork owns its own cache state; writes to the fork do not
    /// affect the parent until `merge` is called.
    pub fn fork(&self, overlay_dir: PathBuf) -> ForkedCache {
        ForkedCache {
            inherited: self.entries.clone(),
            added: HashMap::new(),
            overlay_dir,
            tick: self.tick,
        }
    }

    /// W1.3 — Merge a forked cache back into this one. Fork's added
    /// entries win on conflict (they are newer by construction). The
    /// LRU stamp is reapplied from this cache's tick so the merge
    /// doesn't smuggle the fork's tick namespace into parent ordering.
    pub fn merge(&mut self, forked: ForkedCache) {
        for (path, entry) in forked.added {
            self.record(path, entry);
        }
    }
}

/// Fork-scoped view. Owns only the additions made during the fork so
/// merge-on-accept is O(fork-writes), not O(parent-size).
#[derive(Debug, Clone)]
pub struct ForkedCache {
    /// Snapshot of parent entries at fork time. The fork reads through
    /// these but must not mutate — all mutations go to `added`.
    inherited: HashMap<PathBuf, FileStateEntry>,
    /// Entries observed during the fork. Merge-back installs these
    /// into the parent cache.
    added: HashMap<PathBuf, FileStateEntry>,
    pub overlay_dir: PathBuf,
    tick: u64,
}

impl ForkedCache {
    /// Read-side lookup: fork-local writes shadow inherited entries.
    pub fn get(&self, path: &PathBuf) -> Option<&FileStateEntry> {
        self.added.get(path).or_else(|| self.inherited.get(path))
    }

    /// Record a fork-scope observation.
    pub fn record(&mut self, path: PathBuf, mut entry: FileStateEntry) {
        self.tick = self.tick.saturating_add(1);
        entry.last_touched = self.tick;
        self.added.insert(path, entry);
    }

    pub fn added_count(&self) -> usize {
        self.added.len()
    }

    pub fn inherited_count(&self) -> usize {
        self.inherited.len()
    }

    pub fn total_visible(&self) -> usize {
        // Fork-local additions shadow inherited entries of the same path.
        let mut visible = self.inherited.len();
        for path in self.added.keys() {
            if !self.inherited.contains_key(path) {
                visible += 1;
            }
        }
        visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn e(mtime: i64, size: u64) -> FileStateEntry {
        FileStateEntry::new(mtime, size)
    }

    #[test]
    fn record_and_get_roundtrip() {
        let mut c = FileStateCache::default();
        c.record(p("a.rs"), e(100, 42));
        let got = c.get(&p("a.rs")).unwrap();
        assert_eq!(got.mtime_unix, 100);
        assert_eq!(got.size_bytes, 42);
        assert!(got.last_touched > 0);
    }

    #[test]
    fn lru_evicts_oldest_when_over_capacity() {
        let mut c = FileStateCache::with_capacity(2);
        c.record(p("a"), e(1, 1));
        c.record(p("b"), e(2, 2));
        // Touch a so b is now the oldest.
        c.touch(&p("a"));
        c.record(p("c"), e(3, 3));
        assert!(c.get(&p("a")).is_some(), "a kept by touch");
        assert!(c.get(&p("b")).is_none(), "b evicted as oldest");
        assert!(c.get(&p("c")).is_some());
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn touch_on_missing_returns_false() {
        let mut c = FileStateCache::default();
        assert!(!c.touch(&p("nope")));
    }

    #[test]
    fn fork_inherits_parent_snapshot() {
        let mut parent = FileStateCache::default();
        parent.record(p("a"), e(1, 1));
        parent.record(p("b"), e(2, 2));
        let fork = parent.fork(PathBuf::from("/tmp/overlay"));
        assert_eq!(fork.inherited_count(), 2);
        assert!(fork.get(&p("a")).is_some());
    }

    #[test]
    fn fork_writes_dont_leak_to_parent_until_merge() {
        let mut parent = FileStateCache::default();
        parent.record(p("a"), e(1, 1));
        let mut fork = parent.fork(PathBuf::from("/tmp/overlay"));
        fork.record(p("new"), e(9, 9));
        assert_eq!(fork.added_count(), 1);
        // Parent unchanged.
        assert!(parent.get(&p("new")).is_none());
        assert_eq!(parent.len(), 1);
    }

    #[test]
    fn merge_promotes_added_entries_into_parent() {
        let mut parent = FileStateCache::default();
        parent.record(p("a"), e(1, 1));
        parent.record(p("b"), e(2, 2));
        let mut fork = parent.fork(PathBuf::from("/tmp/overlay"));
        fork.record(p("c"), e(3, 3));
        parent.merge(fork);
        assert_eq!(parent.len(), 3);
        assert!(parent.get(&p("c")).is_some());
    }

    #[test]
    fn merge_does_not_duplicate_overlapping_paths() {
        let mut parent = FileStateCache::default();
        parent.record(p("a"), e(1, 1));
        let mut fork = parent.fork(PathBuf::from("/tmp/overlay"));
        fork.record(p("a"), e(2, 99)); // newer obs
        parent.merge(fork);
        assert_eq!(parent.len(), 1);
        let got = parent.get(&p("a")).unwrap();
        assert_eq!(got.size_bytes, 99, "fork obs wins on conflict");
    }

    #[test]
    fn total_visible_counts_shadow_correctly() {
        let mut parent = FileStateCache::default();
        parent.record(p("a"), e(1, 1));
        parent.record(p("b"), e(2, 2));
        let mut fork = parent.fork(PathBuf::from("/tmp/overlay"));
        fork.record(p("a"), e(9, 9)); // shadows inherited a
        fork.record(p("c"), e(3, 3)); // new entry
        assert_eq!(fork.total_visible(), 3, "a (shadowed) + b + c");
    }

    #[test]
    fn lru_preserved_across_merge() {
        let mut parent = FileStateCache::with_capacity(3);
        parent.record(p("a"), e(1, 1));
        parent.record(p("b"), e(2, 2));
        parent.record(p("c"), e(3, 3));
        let mut fork = parent.fork(PathBuf::from("/tmp/overlay"));
        fork.record(p("d"), e(4, 4));
        parent.merge(fork);
        // One of the four must have been evicted because capacity=3.
        assert_eq!(parent.len(), 3);
        assert!(parent.get(&p("d")).is_some(), "newest fork add kept");
    }

    #[test]
    fn acceptance_parent_reads_2_fork_reads_3_merge_yields_3() {
        // Acceptance assertion from W1.3 plan:
        //   parent reads 2 files; fork reads same 2 + 1 new;
        //   merge grows parent to 3 entries without duplicating.
        let mut parent = FileStateCache::with_capacity(16);
        parent.record(p("auth.rs"), e(100, 1000));
        parent.record(p("session.rs"), e(200, 2000));
        assert_eq!(parent.len(), 2);

        let mut fork = parent.fork(PathBuf::from("/tmp/overlay"));
        // Fork "reads" the same two (re-observes) plus a third.
        fork.record(p("auth.rs"), e(100, 1000));
        fork.record(p("session.rs"), e(200, 2000));
        fork.record(p("tokens.rs"), e(300, 3000));

        parent.merge(fork);
        assert_eq!(parent.len(), 3, "no duplicates after merge");
        assert!(parent.get(&p("tokens.rs")).is_some());
    }
}
