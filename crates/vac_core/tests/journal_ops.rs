#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Tests for vac_tools::journal reversible file operations.

use tempfile::tempdir;
use uuid::Uuid;
use vac_tools::journal::{list_snapshots, restore_snapshot, snapshot_before_write};

#[test]
fn snapshot_new_file_returns_none() {
    let dir = tempdir().unwrap();
    let session_id = Uuid::new_v4();
    // File doesn't exist yet
    let result = snapshot_before_write(dir.path(), session_id, "nonexistent.rs");
    assert!(result.is_none(), "new file should not produce snapshot");
}

#[test]
fn snapshot_existing_file_creates_bak() {
    let dir = tempdir().unwrap();
    let session_id = Uuid::new_v4();

    // Create a file
    std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

    let snap = snapshot_before_write(dir.path(), session_id, "main.rs");
    assert!(snap.is_some(), "existing file should produce snapshot");
    assert!(snap.unwrap().exists(), "snapshot file should exist");
}

#[test]
fn list_snapshots_returns_original_paths() {
    let dir = tempdir().unwrap();
    let session_id = Uuid::new_v4();

    std::fs::write(dir.path().join("lib.rs"), "pub fn foo() {}").unwrap();
    snapshot_before_write(dir.path(), session_id, "lib.rs");

    let snapshots = list_snapshots(dir.path(), session_id);
    assert!(
        snapshots.contains(&"lib.rs".to_string()),
        "should list lib.rs: {:?}",
        snapshots
    );
}

#[test]
fn restore_snapshot_recovers_original_content() {
    let dir = tempdir().unwrap();
    let session_id = Uuid::new_v4();
    let file_path = dir.path().join("handler.rs");

    let original = "async fn handler() -> VilResponse<()> { VilResponse::ok(()) }";
    std::fs::write(&file_path, original).unwrap();

    // Snapshot before mutation
    snapshot_before_write(dir.path(), session_id, "handler.rs");

    // Mutate
    std::fs::write(&file_path, "async fn handler() -> Json<()> { Json(()) }").unwrap();

    // Restore
    restore_snapshot(dir.path(), session_id, "handler.rs").unwrap();

    let restored = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(restored, original, "restored content should match original");
}

#[test]
fn restore_nonexistent_snapshot_returns_error() {
    let dir = tempdir().unwrap();
    let session_id = Uuid::new_v4();
    let result = restore_snapshot(dir.path(), session_id, "ghost.rs");
    assert!(
        result.is_err(),
        "restoring non-existent snapshot should fail"
    );
}
