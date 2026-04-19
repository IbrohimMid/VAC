#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use chrono::Utc;

#[test]
fn autopilot_status_reads_rich_state_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".vac")).unwrap();

    let state = serde_json::json!({
        "state": "polling",
        "mode": "monitor",
        "poll_interval_secs": 3,
        "queue_len": 5,
        "current_job": null,
        "last_event": "task_queued",
        "last_error": null,
        "updated_at": Utc::now().to_rfc3339(),
    });
    std::fs::write(
        root.join(".vac/autopilot.state"),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.args([
        "--project",
        root.to_str().unwrap(),
        "--format",
        "json",
        "autopilot",
        "status",
    ]);
    let out = cmd.assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v.get("status").and_then(|s| s.as_str()), Some("stopped"));
    let internal = v.get("internal_state").unwrap();
    assert_eq!(
        internal.get("state").and_then(|s| s.as_str()),
        Some("polling")
    );
    assert_eq!(
        internal.get("mode").and_then(|s| s.as_str()),
        Some("monitor")
    );
    assert_eq!(
        internal.get("poll_interval_secs").and_then(|s| s.as_u64()),
        Some(3)
    );
    assert_eq!(internal.get("queue_len").and_then(|s| s.as_u64()), Some(5));
}
