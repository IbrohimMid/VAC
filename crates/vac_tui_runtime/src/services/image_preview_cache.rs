//! Async image-preview cache (PR-T17 / M1).
//!
//! `review_preview::prepare_image_preview` performs synchronous disk I/O
//! and PNG header decoding. Calling it from the ratatui render closure
//! (as the original R8b wiring did) blocks the tokio runtime for the
//! full duration of the read + decode — a material responsiveness debt
//! flagged by the reviewer audit of `7c26f8a..7f02440`.
//!
//! This module owns the off-render-path replacement:
//!
//! 1. `ImagePreviewCache` stores `Loading` / `Ready` entries per path.
//! 2. `request_load` marks the path as `Loading` and spawns a *plain
//!    OS thread* (not `tokio::task::spawn_blocking`) to run the
//!    blocking prepare. Using `std::thread::spawn` means this cache is
//!    usable from contexts that do not hold a tokio runtime handle
//!    (tests, non-async callers); the spawned thread is short-lived
//!    (a single file read + header decode, capped at
//!    `IMAGE_PREVIEW_MAX_BYTES`) so OS-thread creation cost is
//!    negligible compared to the syscall + decode work itself.
//! 3. The worker sends its `Result` back through an `mpsc` channel.
//! 4. `drain_pending` moves any buffered results into the cache; the
//!    event loop calls this once per iteration *before* `terminal.draw`
//!    so the next frame always reflects the freshest completed load.
//! 5. The render path is now pure: it reads the cache and either
//!    renders the preview, the error line, or a "Loading…" placeholder.
//!    No blocking syscalls happen while ratatui is drawing.
//!
//! ### Liveness
//!
//! Completed results are delivered via the same channel regardless of
//! whether the user is still looking at the preview. Stale entries
//! (e.g. the user switched files while a load was in flight) stay in
//! the cache; their memory cost is bounded by the `IMAGE_PREVIEW_MAX_BYTES`
//! cap per entry and the entries get evicted the next time the user
//! navigates back to that path and we observe the file changed (future
//! enhancement — today we never evict by design, because the review
//! workspace is typically read-only during a review session).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use crate::services::review_preview::{
    IMAGE_PREVIEW_MAX_BYTES, ImagePreview, ImagePreviewError, prepare_image_preview,
};

/// A single cache slot. `Loading` means a worker thread is currently
/// running `prepare_image_preview` for this path; subsequent
/// `request_load` calls for the same path are no-ops. `Ready` carries
/// the outcome so the render path can show either the preview or a
/// deterministic error line.
#[derive(Debug, Clone)]
pub enum ImagePreviewCacheEntry {
    Loading,
    Ready(Result<ImagePreview, ImagePreviewError>),
}

/// Off-render-path cache for image previews. See module docs for the
/// scheduling model.
pub struct ImagePreviewCache {
    entries: HashMap<PathBuf, ImagePreviewCacheEntry>,
    tx: Sender<(PathBuf, Result<ImagePreview, ImagePreviewError>)>,
    rx: Receiver<(PathBuf, Result<ImagePreview, ImagePreviewError>)>,
}

impl Default for ImagePreviewCache {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ImagePreviewCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImagePreviewCache")
            .field("entries", &self.entries)
            .field("in_flight", &self.in_flight_count())
            .finish_non_exhaustive()
    }
}

impl ImagePreviewCache {
    /// Build an empty cache with its own internal channel. Each cache
    /// owns its own `(Sender, Receiver)` pair so two caches cannot
    /// accidentally steal each other's results. Constructing a new
    /// cache is O(1) and allocation-light.
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            entries: HashMap::new(),
            tx,
            rx,
        }
    }

    /// Look up a path without mutating the cache. Returns `None` for
    /// paths that have never been requested, `Some(Loading)` while a
    /// worker is in flight, and `Some(Ready(..))` once a result has
    /// been drained in.
    pub fn get(&self, path: &Path) -> Option<&ImagePreviewCacheEntry> {
        self.entries.get(path)
    }

    /// Number of entries currently marked `Loading` — exposed for tests
    /// and for the event loop to surface a progress indicator if it ever
    /// wants to. Not used as a flow-control signal today.
    pub fn in_flight_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| matches!(e, ImagePreviewCacheEntry::Loading))
            .count()
    }

    /// Kick off a background load for `path` unless one is already in
    /// flight or a result is already cached. Returns `true` when this
    /// call actually spawned a worker, `false` when the cache already
    /// knew the answer or a worker was already running. Idempotent.
    pub fn request_load(&mut self, path: PathBuf) -> bool {
        if self.entries.contains_key(&path) {
            return false;
        }
        self.entries
            .insert(path.clone(), ImagePreviewCacheEntry::Loading);
        let tx = self.tx.clone();
        let load_path = path.clone();
        std::thread::Builder::new()
            .name(format!("image-preview:{}", path.display()))
            .spawn(move || {
                let result = prepare_image_preview(&load_path, IMAGE_PREVIEW_MAX_BYTES);
                let _ = tx.send((load_path, result));
            })
            .expect("OS thread spawn for image preview cannot fail");
        true
    }

    /// Drain all results buffered in the channel into the cache. Called
    /// once per event-loop iteration *before* `terminal.draw`. Returns
    /// the number of entries that transitioned from `Loading` to
    /// `Ready` on this call — useful in tests to assert liveness.
    pub fn drain_pending(&mut self) -> usize {
        let mut settled = 0usize;
        loop {
            match self.rx.try_recv() {
                Ok((path, result)) => {
                    self.entries
                        .insert(path, ImagePreviewCacheEntry::Ready(result));
                    settled += 1;
                }
                Err(TryRecvError::Empty) => break,
                // The sender lives on `self`; the only way Disconnected
                // can happen is if the cache itself is being torn down,
                // at which point there is nothing more to drain.
                Err(TryRecvError::Disconnected) => break,
            }
        }
        settled
    }

    /// Test-only helper: synchronously drain until the given path
    /// transitions to `Ready`, up to `deadline`. Returns `true` if the
    /// entry became ready, `false` on timeout. Not used at runtime;
    /// runtime always drains opportunistically via `drain_pending`.
    #[cfg(test)]
    fn block_until_ready(&mut self, path: &Path, deadline: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < deadline {
            self.drain_pending();
            if matches!(
                self.entries.get(path),
                Some(ImagePreviewCacheEntry::Ready(_))
            ) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5)); // allow_sync_io: test-only polling helper
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Same 1x1 red PNG used by `review_preview` tests.
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0x99, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x5B, 0x6E, 0x2A, 0xA8, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn empty_cache_returns_none_for_any_path() {
        // Baseline: a just-constructed cache reports no knowledge of any
        // path. Prevents accidental global state bleed between tests.
        let cache = ImagePreviewCache::new();
        assert!(cache.get(Path::new("/tmp/nothing.png")).is_none());
        assert_eq!(cache.in_flight_count(), 0);
    }

    #[test]
    fn request_load_is_idempotent_across_repeated_calls_for_same_path() {
        // The render loop will call `request_load` every frame while the
        // user is looking at the same file. Only the first call spawns
        // a worker; subsequent calls are no-ops until the result lands.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("same.png");
        std::fs::File::create(&path) // allow_sync_io: test-only tempdir setup
            .and_then(|mut f| f.write_all(TINY_PNG))
            .expect("write tiny.png");

        let mut cache = ImagePreviewCache::new();
        assert!(
            cache.request_load(path.clone()),
            "first call must spawn a worker"
        );
        assert!(
            !cache.request_load(path.clone()),
            "second call for the same path must be a no-op"
        );
        assert_eq!(cache.in_flight_count(), 1);

        // Liveness: within a generous deadline the worker delivers its
        // result and `drain_pending` settles the entry to `Ready(Ok)`.
        assert!(
            cache.block_until_ready(&path, std::time::Duration::from_secs(2)),
            "worker must finish a 1x1 PNG decode well under 2s"
        );
        match cache.get(&path) {
            Some(ImagePreviewCacheEntry::Ready(Ok(preview))) => {
                assert_eq!(preview.width, 1);
                assert_eq!(preview.height, 1);
            }
            other => panic!("expected Ready(Ok), got {other:?}"),
        }
        assert_eq!(
            cache.in_flight_count(),
            0,
            "in-flight counter must decrement after drain settles the entry"
        );
    }

    #[test]
    fn ready_error_is_surfaced_verbatim() {
        // A non-existent path must settle as `Ready(Err(Io(_)))` so the
        // render path can show a deterministic error line instead of
        // spinning in `Loading` forever.
        let mut cache = ImagePreviewCache::new();
        let missing = PathBuf::from("/definitely/does/not/exist.png");
        assert!(cache.request_load(missing.clone()));
        assert!(cache.block_until_ready(&missing, std::time::Duration::from_secs(2)));
        match cache.get(&missing) {
            Some(ImagePreviewCacheEntry::Ready(Err(ImagePreviewError::Io(_)))) => {}
            other => panic!("expected Ready(Err(Io)), got {other:?}"),
        }
    }

    #[test]
    fn drain_pending_is_safe_when_channel_has_no_results_yet() {
        // Calling drain on an empty channel must return 0 without
        // blocking. This is the hot path — it runs every frame even
        // when nothing is loading.
        let mut cache = ImagePreviewCache::new();
        assert_eq!(cache.drain_pending(), 0);
        assert_eq!(cache.drain_pending(), 0);
    }

    #[test]
    fn multiple_distinct_paths_each_get_their_own_slot() {
        // Independent paths must not collide on the in-flight marker
        // and each must settle independently.
        let dir = tempfile::tempdir().expect("tempdir");
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        for p in [&a, &b] {
            std::fs::File::create(p) // allow_sync_io: test-only tempdir setup
                .and_then(|mut f| f.write_all(TINY_PNG))
                .expect("write png");
        }
        let mut cache = ImagePreviewCache::new();
        assert!(cache.request_load(a.clone()));
        assert!(cache.request_load(b.clone()));
        assert_eq!(cache.in_flight_count(), 2);
        assert!(cache.block_until_ready(&a, std::time::Duration::from_secs(2)));
        assert!(cache.block_until_ready(&b, std::time::Duration::from_secs(2)));
        assert!(matches!(
            cache.get(&a),
            Some(ImagePreviewCacheEntry::Ready(Ok(_)))
        ));
        assert!(matches!(
            cache.get(&b),
            Some(ImagePreviewCacheEntry::Ready(Ok(_)))
        ));
    }
}
