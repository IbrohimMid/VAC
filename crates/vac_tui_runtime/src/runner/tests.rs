//! Unit tests for runner.rs session restore helpers.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use tokio::time::timeout;

#[tokio::test]
async fn switch_to_session_invalid_uuid_emits_error() {
    let dir = tempfile::tempdir().unwrap();
    let engine = VacEngine::new(dir.path().to_path_buf()).await.unwrap();
    let engine = Arc::new(Mutex::new(engine));
    let active_update_tx: ActiveUpdateTx = Arc::new(Mutex::new(None));

    let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(8);
    resume_session_into_tui(engine, active_update_tx, input_tx, "not-a-uuid".to_string()).await;

    let ev = timeout(std::time::Duration::from_secs(1), input_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        InputEvent::Error(msg) => assert!(msg.contains("Invalid session id")),
        _ => panic!("unexpected event"),
    }
}

#[tokio::test]
async fn switch_to_session_emits_session_restored() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut engine = VacEngine::new(root.clone()).await.unwrap();
    engine.init().await.unwrap();
    let engine = Arc::new(Mutex::new(engine));
    let active_update_tx: ActiveUpdateTx = Arc::new(Mutex::new(None));

    let session = vac_core::session::Session::new(root.clone());
    let id = session.id.to_string();
    session.save().unwrap();

    let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(16);
    resume_session_into_tui(engine, active_update_tx, input_tx, id.clone()).await;

    let first = timeout(std::time::Duration::from_secs(1), input_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match first {
        InputEvent::SessionRestored { id: got, .. } => assert_eq!(got, id),
        _ => panic!("unexpected first event"),
    }
}
