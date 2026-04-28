//! Slice 7 — read-only session enumeration proof.
//!
//! Builds a fake project tree, writes a couple of `.jsonl` session
//! transcripts plus some noise files, and checks that
//! `enumerate_sessions` returns the right entries in newest-first
//! order. Path remap is asserted in the unit tests inside the crate;
//! this file focuses on the disk side.

use std::fs;
use std::time::{Duration, SystemTime};

use vac_shell_contracts::VacPaths;
use vac_shell_host_paths::{VacPathsImpl, enumerate_sessions};

fn touch(path: &std::path::Path, when: SystemTime) {
    fs::write(path, b"").unwrap();
    fs::File::open(path)
        .and_then(|f| f.set_modified(when))
        .unwrap();
}

#[test]
fn missing_sessions_dir_yields_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    assert!(enumerate_sessions(&paths).is_empty());
}

#[test]
fn enumerate_returns_only_jsonl_entries_sorted_newest_first() {
    let tmp = tempfile::tempdir().unwrap();
    let sessions = tmp.path().join(".vac").join("sessions");
    fs::create_dir_all(&sessions).unwrap();

    let now = SystemTime::now();
    let one_hour_ago = now - Duration::from_secs(3600);
    let yesterday = now - Duration::from_secs(86_400);

    touch(&sessions.join("first.jsonl"), yesterday);
    touch(&sessions.join("second.jsonl"), one_hour_ago);
    touch(&sessions.join("third.jsonl"), now);
    // Noise that must be skipped.
    touch(&sessions.join("notes.txt"), now);
    fs::create_dir_all(sessions.join("subdir")).unwrap();

    let paths = VacPathsImpl::new(tmp.path());
    let entries = enumerate_sessions(&paths);
    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    assert_eq!(ids, vec!["third", "second", "first"]);
    assert!(entries.iter().all(|e| !e.id.contains(".jsonl")));
    assert!(
        entries
            .windows(2)
            .all(|w| w[0].last_active_unix >= w[1].last_active_unix),
        "must be sorted newest-first"
    );
}

#[test]
fn paths_target_dot_vac_not_dot_stakpak() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let s = paths.sessions_dir().to_string_lossy().to_string();
    assert!(
        s.contains(".vac"),
        "sessions_dir should sit under .vac, got {s}"
    );
    assert!(
        !s.contains(".stakpak"),
        "donor path leaked into sessions_dir: {s}"
    );
}
