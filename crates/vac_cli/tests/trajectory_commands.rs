#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::path::Path;

fn trace_record(
    ts: i64,
    record_type: vac_trace::RecordType,
    content: serde_json::Value,
) -> vac_trace::recorder::TraceRecord {
    vac_trace::recorder::TraceRecord {
        id: uuid::Uuid::new_v4(),
        timestamp: Utc.timestamp_opt(ts, 0).unwrap(),
        record_type,
        agent_id: None,
        content,
    }
}

fn write_session(root: &Path) {
    let session = json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "created_at": "2026-04-21T00:00:00Z",
        "updated_at": "2026-04-22T00:00:00Z",
        "tasks": [
            {
                "id": "11111111-1111-1111-1111-111111111111",
                "description": "edit docs",
                "constraints": { "target_paths": ["src/lib.rs"] }
            }
        ],
        "results": {
            "11111111-1111-1111-1111-111111111111": {
                "summary": "updated docs",
                "modified_files": ["src/lib.rs"],
                "created_files": [],
                "total_tokens_used": 42,
                "elapsed_ms": 10
            }
        },
        "metadata": {
            "total_tokens_used": 42,
            "total_tasks_completed": 1,
            "total_tasks_failed": 0,
            "total_files_modified": 1
        }
    });
    std::fs::create_dir_all(root.join(".vac/sessions")).unwrap();
    std::fs::write(
        root.join(".vac/sessions/session-a.json"),
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .unwrap();
}

fn write_trace(root: &Path) {
    let trace = vec![
        trace_record(
            1,
            vac_trace::RecordType::TaskStart,
            json!({"task_id": "task-1", "description": "investigate parser"}),
        ),
        trace_record(
            2,
            vac_trace::RecordType::ToolCall,
            json!({"tool": "file_write", "arguments": {"path": "src/lib.rs"}}),
        ),
        trace_record(
            3,
            vac_trace::RecordType::TaskComplete,
            json!({"task_id": "task-1", "summary": "completed parser fix"}),
        ),
    ];
    std::fs::create_dir_all(root.join(".vac/traces")).unwrap();
    std::fs::write(
        root.join(".vac/traces/trace-a.json"),
        serde_json::to_string_pretty(&trace).unwrap(),
    )
    .unwrap();
}

fn vac_command(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("--project").arg(root);
    cmd.args(args);
    cmd
}

#[test]
fn observe_lists_trace_and_session_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_session(root);
    write_trace(root);

    vac_command(root, &["observe", "--limit", "5"])
        .assert()
        .success()
        .stdout(predicates::str::contains("VAC Trajectories"))
        .stdout(predicates::str::contains("[session] session-a"))
        .stdout(predicates::str::contains("[trace] trace-a"));
}

#[test]
fn explain_defaults_to_latest_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_session(root);
    write_trace(root);

    vac_command(root, &["explain"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Trajectory: session: session-a"))
        .stdout(predicates::str::contains("updated docs"));
}

#[test]
fn why_reports_file_match_from_session() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_session(root);
    write_trace(root);

    vac_command(root, &["why", "src/lib.rs"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Why: src/lib.rs"))
        .stdout(predicates::str::contains("modified file"))
        .stdout(predicates::str::contains("updated docs"));
}
