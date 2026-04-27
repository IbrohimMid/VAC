use vac_shell_contracts::{Severity, ShellActivityKind};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_event_projection::{
    RuntimeEventView, project_runtime_event, record_projected_event,
};
use vac_shell_test_support::assert_activity_log_contains_kind;

#[test]
fn tool_started_projects_to_activity() {
    let entry = project_runtime_event(RuntimeEventView::ToolStarted {
        id: "x".into(),
        ts_unix: 1,
        name: "shell".into(),
        args_summary: Some("ls".into()),
    });
    assert_eq!(entry.kind, ShellActivityKind::ToolCall);
    assert_eq!(entry.title, "shell");
    assert_eq!(entry.detail.as_deref(), Some("ls"));
    assert_eq!(entry.severity, Severity::Info);
}

#[test]
fn tool_finished_error_projects_error_severity() {
    let entry = project_runtime_event(RuntimeEventView::ToolFinished {
        id: "y".into(),
        ts_unix: 2,
        name: "shell".into(),
        severity: Severity::Error,
        summary: Some("nonzero exit".into()),
    });
    assert_eq!(entry.severity, Severity::Error);
    assert_eq!(entry.kind, ShellActivityKind::ToolResult);
}

#[test]
fn approval_requested_projects_activity() {
    let entry = project_runtime_event(RuntimeEventView::ApprovalRequested {
        id: "z".into(),
        ts_unix: 3,
        tool: "shell".into(),
    });
    assert_eq!(entry.kind, ShellActivityKind::ApprovalRequested);
    assert_eq!(entry.severity, Severity::Warn);
}

#[test]
fn approval_resolved_projects_severity_by_outcome() {
    let approved = project_runtime_event(RuntimeEventView::ApprovalResolved {
        id: "1".into(),
        ts_unix: 4,
        tool: "shell".into(),
        approved: true,
    });
    let rejected = project_runtime_event(RuntimeEventView::ApprovalResolved {
        id: "2".into(),
        ts_unix: 5,
        tool: "shell".into(),
        approved: false,
    });
    assert_eq!(approved.severity, Severity::Ok);
    assert_eq!(rejected.severity, Severity::Warn);
    assert_eq!(approved.detail.as_deref(), Some("approved"));
    assert_eq!(rejected.detail.as_deref(), Some("rejected"));
}

#[test]
fn model_changed_projects_activity() {
    let entry = project_runtime_event(RuntimeEventView::ModelChanged {
        id: "m".into(),
        ts_unix: 6,
        provider: "openai".into(),
        model_id: "gpt-4o".into(),
    });
    assert_eq!(entry.kind, ShellActivityKind::ModelChanged);
    assert_eq!(entry.title, "openai / gpt-4o");
    assert_eq!(entry.severity, Severity::Ok);
}

#[test]
fn file_edit_projects_activity() {
    let entry = project_runtime_event(RuntimeEventView::FileEdited {
        id: "f".into(),
        ts_unix: 7,
        path: "src/lib.rs".into(),
    });
    assert_eq!(entry.kind, ShellActivityKind::FileEdit);
    assert_eq!(entry.title, "src/lib.rs");
}

#[test]
fn error_projects_to_error_severity() {
    let entry = project_runtime_event(RuntimeEventView::Error {
        id: "e".into(),
        ts_unix: 8,
        title: "boom".into(),
        detail: Some("bad".into()),
    });
    assert_eq!(entry.severity, Severity::Error);
    assert_eq!(entry.kind, ShellActivityKind::Error);
}

#[test]
fn ingest_tool_event_updates_activity_log() {
    let log = ActivityLog::default();
    record_projected_event(
        &log,
        RuntimeEventView::ToolStarted {
            id: "1".into(),
            ts_unix: 9,
            name: "shell".into(),
            args_summary: None,
        },
    );
    assert_activity_log_contains_kind(&log, ShellActivityKind::ToolCall);
}

#[test]
fn ingest_error_event_visible_in_activity_log() {
    let log = ActivityLog::default();
    record_projected_event(
        &log,
        RuntimeEventView::Error {
            id: "e".into(),
            ts_unix: 10,
            title: "engine error".into(),
            detail: None,
        },
    );
    assert_activity_log_contains_kind(&log, ShellActivityKind::Error);
}

#[test]
fn event_projection_tool_finished_still_uses_tool_result_kind() {
    let entry = project_runtime_event(RuntimeEventView::ToolFinished {
        id: "x".into(),
        ts_unix: 1,
        name: "glob".into(),
        severity: Severity::Ok,
        summary: Some("found 5 files".into()),
    });
    assert_eq!(entry.kind, ShellActivityKind::ToolResult, "true tool execution must stay ToolResult");
}
