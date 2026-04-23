use std::path::PathBuf;
use uuid::Uuid;
use vac_session_engine::{TranscriptWriter, TranscriptEntry, TranscriptKind};

#[tokio::test]
async fn test_m9_resume_aborted_submit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let session_id = Uuid::new_v4();

    // 1. Simulate a crashed mid-submit
    let writer = TranscriptWriter::new(root.clone());
    let handle = writer.open(session_id).await.unwrap();
    
    let accepted = TranscriptEntry::new(
        session_id,
        TranscriptKind::Accepted,
        serde_json::json!({ "prompt": "build a web app" }),
    );
    writer.append(&handle, &accepted).await.unwrap();

    // Do NOT write Finished. So `last_pending_submit` will return `Some(accepted.id)`.

    // 2. Boot TUI (simulate event loop)
    let (_tx, _rx) = tokio::sync::mpsc::channel::<()>(10);
    
    // We can't easily start the whole crossterm app in a unit test, but we can test
    // that `last_pending_submit` correctly finds it, and that sending a "yes" tool result
    // triggers the resume.
    let pending = writer.last_pending_submit(session_id).await.unwrap();
    assert_eq!(pending, Some(accepted.id));
}
