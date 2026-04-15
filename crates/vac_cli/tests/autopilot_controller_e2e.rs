use assert_cmd::Command;
use std::time::{Duration, Instant};

use serde_json::json;
use vac_runtime::{Job, JobKind, JobStatus};

fn write_autopilot_toml(root: &std::path::Path, mode: &str, poll_interval_secs: u64) {
    let content = format!(
        "poll_interval_secs = {poll_interval_secs}\nmax_concurrent = 1\nmode = \"{mode}\"\n"
    );
    std::fs::write(root.join("autopilot.toml"), content).unwrap();
}

fn write_queue(root: &std::path::Path, jobs: Vec<Job>) {
    std::fs::create_dir_all(root.join(".vac")).unwrap();
    let json = serde_json::to_string_pretty(&jobs).unwrap();
    std::fs::write(root.join(".vac/queue.json"), json).unwrap();
}

fn write_vac_config_toml(root: &std::path::Path) {
    std::fs::create_dir_all(root.join(".vac")).unwrap();
    let content = r#"
[llm]
default_provider = "anthropic"
providers = {}

[tools]
default_policy = "deny"
allow = {}
deny = {}

[memory]
[context]
[swarm]
[trace]
"#;
    std::fs::write(root.join(".vac/config.toml"), content.trim_start()).unwrap();
}

fn read_queue(root: &std::path::Path) -> Vec<Job> {
    let content =
        std::fs::read_to_string(root.join(".vac/queue.json")).unwrap_or_else(|_| "[]".to_string());
    serde_json::from_str(&content).unwrap()
}

fn wait_until<F: FnMut() -> bool>(timeout: Duration, mut f: F) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn run_vac(root: &std::path::Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("--project").arg(root).args(args);
    cmd.assert()
}

#[test]
fn autopilot_monitor_mode_does_not_execute_job_and_reports_queue() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_autopilot_toml(root, "monitor", 1);
    let job = Job::new(JobKind::DiagnosticSweep);
    let id = job.id;
    write_queue(root, vec![job]);

    run_vac(root, &["autopilot", "up"]).success();

    let state_ok = wait_until(Duration::from_secs(2), || {
        root.join(".vac/autopilot.state").exists()
    });
    assert!(state_ok);

    let queued_ok = wait_until(Duration::from_secs(2), || {
        read_queue(root)
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.status == JobStatus::Queued)
            .unwrap_or(false)
    });
    assert!(queued_ok);

    let state_content = std::fs::read_to_string(root.join(".vac/autopilot.state")).unwrap();
    let state: vac_runtime::AutopilotStateFile = serde_json::from_str(&state_content).unwrap();
    assert_eq!(state.mode, "monitor");
    assert!(matches!(
        state.state,
        vac_runtime::AutopilotState::Polling | vac_runtime::AutopilotState::Idle
    ));
    assert_eq!(state.queue_len, 1);

    run_vac(root, &["autopilot", "down"]).success();

    assert!(!root.join(".vac/autopilot.pid").exists());
}

#[test]
fn autopilot_auto_mode_executes_job_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_autopilot_toml(root, "auto", 1);
    let job = Job::new(JobKind::DiagnosticSweep);
    let id = job.id;
    write_queue(root, vec![job]);

    run_vac(root, &["autopilot", "up"]).success();

    let completed_ok = wait_until(Duration::from_secs(4), || {
        read_queue(root)
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.status == JobStatus::Completed || matches!(j.status, JobStatus::Failed(_)))
            .unwrap_or(false)
    });
    assert!(completed_ok);

    run_vac(root, &["autopilot", "down"]).success();
}

#[test]
fn autopilot_waiting_approval_state_is_observable_and_unblocks_on_store_intent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_autopilot_toml(root, "auto", 1);
    write_vac_config_toml(root);
    let job = Job::new(JobKind::ToolCall {
        tool_name: "file_write".to_string(),
        arguments: json!({
            "path": "autopilot_approval_test.txt",
            "content": "hello"
        }),
    });
    let id = job.id;
    write_queue(root, vec![job]);

    run_vac(root, &["autopilot", "up"]).success();

    let mut tool_call_id: Option<String> = None;
    let waiting_ok = wait_until(Duration::from_secs(4), || {
        if let Ok(content) = std::fs::read_to_string(root.join(".vac/autopilot.state")) {
            if let Ok(sf) = serde_json::from_str::<vac_runtime::AutopilotStateFile>(&content) {
                if let vac_runtime::AutopilotState::WaitingApproval { tool_call_id: id } = sf.state
                {
                    tool_call_id = Some(id);
                    return true;
                }
            }
        }
        false
    });
    assert!(waiting_ok);
    let tool_call_id = tool_call_id.unwrap();

    let store = vac_core::ApprovalStore::new(root.to_path_buf());
    let record_ok = wait_until(Duration::from_secs(4), || {
        store.load(&tool_call_id).ok().flatten().is_some()
    });
    assert!(record_ok);
    store
        .record_intent(tool_call_id.clone(), true, Some("test".to_string()))
        .unwrap();

    let done_ok = wait_until(Duration::from_secs(4), || {
        read_queue(root)
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.status == JobStatus::Completed || matches!(j.status, JobStatus::Failed(_)))
            .unwrap_or(false)
    });
    assert!(done_ok);

    run_vac(root, &["autopilot", "down"]).success();
}
