#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use serde_json::Value;

#[test]
fn test_doctor_json_output() {
    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("doctor").arg("--format").arg("json");

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Ensure it's valid JSON
    let parsed: Value = serde_json::from_str(&stdout).expect("Failed to parse JSON output");

    // It should have "checks" and "ready"
    assert!(
        parsed.get("checks").is_some(),
        "Missing 'checks' in doctor JSON"
    );
    assert!(
        parsed.get("ready").is_some(),
        "Missing 'ready' in doctor JSON"
    );
}

#[test]
fn test_runtime_jobs_json_output() {
    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("runtime").arg("jobs").arg("--format").arg("json");

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let parsed: Value = serde_json::from_str(&stdout).expect("Failed to parse JSON output");
    assert!(
        parsed.get("jobs").is_some(),
        "Missing 'jobs' in runtime jobs JSON"
    );
}

#[test]
fn test_autopilot_status_json_output() {
    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("autopilot")
        .arg("status")
        .arg("--format")
        .arg("json");

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let parsed: Value = serde_json::from_str(&stdout).expect("Failed to parse JSON output");
    assert!(
        parsed.get("status").is_some(),
        "Missing 'status' in autopilot status JSON"
    );
}

// Disable status test because it depends on the local config file parsing
// which might be invalid in the workspace.
// #[test]
// fn test_status_json_output() {
// ...
