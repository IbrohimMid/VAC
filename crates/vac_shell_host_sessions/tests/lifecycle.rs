use vac_shell_contracts::{SessionAction, VacPaths};
use vac_shell_host_sessions::SessionsState;
use vac_shell_test_support::{FakeVacPaths, temp_session_transcript};

fn seed(paths: &dyn VacPaths) {
    temp_session_transcript(
        paths.sessions_dir().join("alpha.jsonl"),
        "operator: hi\nagent: hello\n",
    );
    temp_session_transcript(
        paths.sessions_dir().join("beta.jsonl"),
        "operator: refactor\nagent: ok\n",
    );
}

#[test]
fn list_returns_all_jsonl_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().to_path_buf());
    seed(&paths);
    let state = SessionsState::new();
    assert_eq!(state.list(&paths).len(), 2);
}

#[test]
fn preview_loads_first_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().to_path_buf());
    seed(&paths);
    let state = SessionsState::new();
    let preview = state.preview(&paths, "alpha", 5).unwrap();
    assert_eq!(preview.id, "alpha");
    assert_eq!(preview.title.as_deref(), Some("operator: hi"));
    assert_eq!(preview.lines, vec!["agent: hello".to_string()]);
}

#[test]
fn resume_records_pending_id() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().to_path_buf());
    seed(&paths);
    let state = SessionsState::new();
    state
        .apply(&paths, SessionAction::Resume { id: "alpha".into() })
        .unwrap();
    assert_eq!(state.take_resume_request().as_deref(), Some("alpha"));
    assert!(state.take_resume_request().is_none());
}

#[test]
fn delete_removes_transcript_file() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().to_path_buf());
    seed(&paths);
    let state = SessionsState::new();
    state
        .apply(&paths, SessionAction::Delete { id: "beta".into() })
        .unwrap();
    let remaining: Vec<String> = state.list(&paths).into_iter().map(|s| s.id).collect();
    assert_eq!(remaining, vec!["alpha".to_string()]);
    assert_eq!(state.deleted_ids(), vec!["beta".to_string()]);
}

#[test]
fn archive_moves_transcript_under_archive_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().to_path_buf());
    seed(&paths);
    let state = SessionsState::new();
    state
        .apply(&paths, SessionAction::Archive { id: "beta".into() })
        .unwrap();
    let live: Vec<String> = state.list(&paths).into_iter().map(|s| s.id).collect();
    assert_eq!(live, vec!["alpha".to_string()]);
    assert!(
        paths
            .sessions_dir()
            .join("archive")
            .join("beta.jsonl")
            .exists()
    );
}

#[test]
fn unknown_id_resume_errors_without_recording() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().to_path_buf());
    seed(&paths);
    let state = SessionsState::new();
    let err = state
        .apply(&paths, SessionAction::Resume { id: "ghost".into() })
        .unwrap_err();
    assert!(format!("{err}").contains("ghost"));
    assert!(state.take_resume_request().is_none());
}
