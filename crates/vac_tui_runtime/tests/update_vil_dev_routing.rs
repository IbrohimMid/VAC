//! Integration tests for VilDevEvent routing through handle_backend_event
//! (Task-10 / Wave-2.5 / Gate-T3 — extracted from update.rs inline test module).

use tokio::sync::mpsc;
use vac_runtime::JobStatus;
use vac_tui_runtime::app::{ActivityKind, AppState, InputEvent, OutputEvent};
use vac_tui_runtime::services::vil_dev_runner::RunnerEvent;
use vac_tui_runtime::update::handle_backend_event;

fn make_state_and_tx() -> (AppState, mpsc::Sender<OutputEvent>) {
    let state = AppState::default();
    let (tx, _rx) = mpsc::channel(64);
    (state, tx)
}

#[test]
fn vil_dev_started_emits_activity_and_task_tray_entry() {
    let (mut state, tx) = make_state_and_tx();
    handle_backend_event(&mut state, &tx, InputEvent::VilDevEvent(RunnerEvent::Started { pid: 42 }));
    assert_eq!(state.vil_dev.pid, Some(42));
    assert!(state.vil_dev.job_id.is_some(), "task tray job must be created");
    assert!(
        state.runtime.jobs.iter().any(|j| matches!(
            &j.kind,
            vac_runtime::JobKind::RunTask { description } if description == "vil dev"
        )),
        "a vil dev job must appear in runtime.jobs"
    );
    assert!(
        state
            .activity
            .iter()
            .any(|a| a.kind == ActivityKind::Status && a.message.contains("vil dev: started")),
        "activity log must contain started entry"
    );
}

#[test]
fn vil_dev_exited_marks_task_tray_entry_done() {
    let (mut state, tx) = make_state_and_tx();
    handle_backend_event(&mut state, &tx, InputEvent::VilDevEvent(RunnerEvent::Started { pid: 99 }));
    let job_id = state.vil_dev.job_id.expect("job id must be set after Started");
    handle_backend_event(
        &mut state,
        &tx,
        InputEvent::VilDevEvent(RunnerEvent::Exited { code: Some(0), signal: None }),
    );
    assert!(state.vil_dev.pid.is_none(), "pid must be cleared");
    assert!(state.vil_dev.job_id.is_none(), "job id must be cleared after exit");
    let job = state.runtime.jobs.iter().find(|j| j.id == job_id).expect("job must exist");
    assert_eq!(job.status, JobStatus::Completed, "job status must be Completed");
}

#[test]
fn vil_dev_stderr_first_line_pushed_to_activity() {
    let (mut state, tx) = make_state_and_tx();
    handle_backend_event(
        &mut state,
        &tx,
        InputEvent::VilDevEvent(RunnerEvent::Stderr("some error".to_string())),
    );
    assert!(
        state
            .activity
            .iter()
            .any(|a| a.kind == ActivityKind::Error && a.message.contains("some error")),
        "stderr first line must appear as Error activity"
    );
}

#[test]
fn vil_dev_error_pushes_error_activity_and_task_tray_error() {
    let (mut state, tx) = make_state_and_tx();
    handle_backend_event(&mut state, &tx, InputEvent::VilDevEvent(RunnerEvent::Started { pid: 7 }));
    let job_id = state.vil_dev.job_id.expect("job must exist");
    handle_backend_event(
        &mut state,
        &tx,
        InputEvent::VilDevEvent(RunnerEvent::Error("spawn failed".to_string())),
    );
    assert!(state.vil_dev.job_id.is_none(), "job id cleared after Error");
    let job = state.runtime.jobs.iter().find(|j| j.id == job_id).expect("job must still exist");
    assert!(
        matches!(&job.status, JobStatus::Failed(msg) if msg.contains("spawn failed")),
        "job must be Failed after Error event"
    );
    assert!(
        state
            .activity
            .iter()
            .any(|a| a.kind == ActivityKind::Error && a.message.contains("spawn failed")),
        "Error event must produce an Error activity entry"
    );
}
