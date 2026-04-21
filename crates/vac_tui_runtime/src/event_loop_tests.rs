// Tests for event_loop.rs — production code is in event_loop.rs
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::app::{AppState, AppStateOptions};

fn make_state(project_root: std::path::PathBuf, session_id: uuid::Uuid) -> AppState {
    AppState::new(AppStateOptions {
        model: None,
        session_id: Some(session_id.to_string()),
        checkpoint_path: Some(project_root.join(".vac/checkpoints")),
        project_root,
    })
}

#[path = "event_loop_tests/approval.rs"]
mod approval;
#[path = "event_loop_tests/runtime.rs"]
mod runtime;
#[path = "event_loop_tests/session.rs"]
mod session;
#[path = "event_loop_tests/shell.rs"]
mod shell;
