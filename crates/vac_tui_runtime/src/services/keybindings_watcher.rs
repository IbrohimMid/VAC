//! Hot-reload watcher for `.vac/keybindings.toml` (PR-T19 R3).
//!
//! Exposes two pieces:
//!
//! 1. [`build_and_install_keymap`] / [`build_and_reload_keymap`] — pure
//!    helpers that read the TOML, resolve effective bindings, build a
//!    [`ChordKeymap`], push it into the global slot (install-once or
//!    always-overwrite), and collect any diagnostic warnings.
//! 2. [`spawn_keybindings_watcher`] — spawns a tokio task that polls the
//!    file's mtime (cheap, cross-platform, no extra crate surface) and
//!    calls [`build_and_reload_keymap`] whenever the mtime changes, then
//!    dispatches any warnings as `BannerStyle::Warning` input events so
//!    the user sees loader/skipped/conflict diagnostics without restart.
//!
//! Polling was chosen over `notify` events because:
//!   - the path often does not exist initially (user has not created
//!     `.vac/keybindings.toml` yet) and notify's semantics around
//!     create-after-watch vary per backend;
//!   - a 1s poll is well under human perception and costs one stat() per
//!     second;
//!   - it avoids dragging the notify runtime into a cross-thread dance
//!     with the tokio-based event loop.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tokio::sync::mpsc::Sender;

use crate::app::InputEvent;
use crate::services::banner::{BannerSeverity, BannerStyle};
use crate::services::keybindings_loader::{load_keybindings, resolve_effective};
use crate::services::keybindings_runtime::{
    ChordKeymap, install_global_keymap, reload_global_keymap,
};

/// Interval between mtime polls. Small enough that edits feel instant but
/// large enough that the stat() cost is negligible.
const POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// Read the keybindings file at `path`, build a [`ChordKeymap`], and
/// collect any diagnostic warnings (loader errors + skipped non-reachable
/// overrides + duplicate chord conflicts). Returns the keymap and the
/// warnings so the caller can decide whether to install-once or reload.
pub fn build_keymap_with_diagnostics(path: &Path) -> (ChordKeymap, Vec<String>) {
    let outcome = load_keybindings(path);
    let effective = resolve_effective(&outcome.overrides);
    let keymap = ChordKeymap::from_effective(&effective);

    let mut diag: Vec<String> = outcome.warnings.clone();
    for (chord, id) in keymap.skipped_bindings() {
        diag.push(format!(
            "keybindings.toml: `{chord}` for `{id:?}` is not reachable via a global chord and was ignored"
        ));
    }
    for (chord, ids) in keymap.conflicts() {
        let names = ids
            .iter()
            .map(|i| format!("{i:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        diag.push(format!(
            "keybindings.toml: `{chord}` is bound to multiple actions ({names}); last wins"
        ));
    }
    (keymap, diag)
}

/// Build the keymap from disk and install it into the process-wide slot
/// (first-write semantics). Returns the diagnostics so the caller can
/// surface them as banners.
pub fn build_and_install_keymap(path: &Path) -> Vec<String> {
    let (keymap, diag) = build_keymap_with_diagnostics(path);
    let _ = install_global_keymap(keymap);
    diag
}

/// Build the keymap from disk and unconditionally replace the global
/// slot. Returns the diagnostics collected during the reload.
pub fn build_and_reload_keymap(path: &Path) -> Vec<String> {
    let (keymap, diag) = build_keymap_with_diagnostics(path);
    reload_global_keymap(keymap);
    diag
}

fn read_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Spawn a tokio task that polls `path` and reloads the global keymap
/// whenever its mtime changes. The watcher keeps running for the
/// lifetime of the process; it exits silently if `tx` is closed.
///
/// A missing file is **not** a reload trigger — the slot keeps the last
/// successfully installed keymap. When the file reappears later, the
/// next mtime change picks it up.
pub fn spawn_keybindings_watcher(path: PathBuf, tx: Sender<InputEvent>) {
    tokio::spawn(async move {
        // Baseline mtime; `None` means the file is currently absent, in
        // which case any future stat() success is itself a change.
        let mut last_mtime = read_mtime(&path);

        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            if tx.is_closed() {
                return;
            }

            let current = read_mtime(&path);
            if current == last_mtime {
                continue;
            }
            last_mtime = current;

            // File missing → don't touch the slot. A previously installed
            // keymap keeps running.
            if current.is_none() {
                continue;
            }

            // Rebuild + reload. Tell the user what happened.
            let diag = build_and_reload_keymap(&path);
            let _ = tx
                .send(InputEvent::ShowBanner(
                    "keybindings.toml reloaded".to_string(),
                    BannerStyle::Info,
                    BannerSeverity::Suggested,
                ))
                .await;
            for w in diag {
                let _ = tx
                    .send(InputEvent::ShowBanner(
                        w,
                        BannerStyle::Warning,
                        BannerSeverity::Suggested,
                    ))
                    .await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::keybindings_runtime::current_global_keymap;
    use tempfile::tempdir;

    // Uses `tokio::fs::write` (async) instead of `std::fs::write` so the
    // sync_io guardrail stays green — see scripts/check_sync_io.sh.
    #[tokio::test]
    async fn build_and_reload_swaps_the_global_keymap() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("keybindings.toml");

        // First config: bind Quit to Ctrl+Q
        tokio::fs::write(&path, "Quit = \"Ctrl+Q\"\n")
            .await
            .expect("write v1");
        let _ = build_and_install_keymap(&path);
        let snap_v1 = current_global_keymap().expect("keymap installed");
        assert!(
            !snap_v1.is_empty(),
            "v1 keymap should contain the Ctrl+Q binding"
        );

        // Second config: different binding (Quit to Ctrl+Shift+Q)
        tokio::fs::write(&path, "Quit = \"Ctrl+Shift+Q\"\n")
            .await
            .expect("write v2");
        let _ = build_and_reload_keymap(&path);
        let snap_v2 = current_global_keymap().expect("keymap still installed");

        // Snapshots are independent Arcs — the reload did not mutate v1.
        assert!(!snap_v2.is_empty());
        // Pointer inequality confirms the slot was replaced, not mutated.
        assert!(
            !std::sync::Arc::ptr_eq(&snap_v1, &snap_v2),
            "reload must swap the Arc, not mutate in place"
        );
    }

    #[tokio::test]
    async fn build_keymap_with_diagnostics_surfaces_conflicts() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("keybindings.toml");
        // Bind Ctrl+P to two globally-reachable actions so conflicts() fires.
        // OpenShortcuts and OpenFileSearch both resolve to chord-dispatchable
        // InputEvents, matching the R1 regression fixture in
        // crates/vac_cli/tests/integration_events.rs.
        tokio::fs::write(
            &path,
            "OpenShortcuts = \"Ctrl+P\"\nOpenFileSearch = \"Ctrl+P\"\n",
        )
        .await
        .expect("write");
        let (_keymap, diag) = build_keymap_with_diagnostics(&path);
        assert!(
            diag.iter().any(|s| s.contains("bound to multiple actions")),
            "duplicate-chord diagnostic should be present, got: {diag:?}"
        );
    }
}
