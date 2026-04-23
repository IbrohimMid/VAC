#![allow(clippy::unwrap_used, clippy::expect_used)]

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
    let content = std::fs::read_to_string(root.join(".vac/queue.json")).unwrap_or_default();
    if content.trim().is_empty() {
        return vec![];
    }
    serde_json::from_str(&content).unwrap_or_default()
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

    run_vac(root, &["autopilot", "up", "--execute"]).success();

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

    run_vac(root, &["autopilot", "up", "--execute"]).success();

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

    run_vac(root, &["autopilot", "up", "--execute"]).success();

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

    let store = vac_approvals::ApprovalStore::new(root.to_path_buf());
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
    if !done_ok {
        println!(
            "LOGS:\n{}",
            std::fs::read_to_string(root.join(".vac/autopilot.log")).unwrap_or_default()
        );
    }
    assert!(done_ok);
    let status = read_queue(root)
        .iter()
        .find(|j| j.id == id)
        .map(|j| j.status.clone())
        .unwrap();
    assert_eq!(status, JobStatus::Completed);
    assert!(root.join("autopilot_approval_test.txt").exists());

    run_vac(root, &["autopilot", "down"]).success();
}

#[test]
fn autopilot_toolcall_reject_flow_blocks_execution() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_autopilot_toml(root, "auto", 1);
    write_vac_config_toml(root);
    let job = Job::new(JobKind::ToolCall {
        tool_name: "file_write".to_string(),
        arguments: json!({
            "path": "autopilot_reject_test.txt",
            "content": "hello"
        }),
    });
    let id = job.id;
    write_queue(root, vec![job]);

    run_vac(root, &["autopilot", "up", "--execute"]).success();

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

    let store = vac_approvals::ApprovalStore::new(root.to_path_buf());
    let record_ok = wait_until(Duration::from_secs(4), || {
        store.load(&tool_call_id).ok().flatten().is_some()
    });
    assert!(record_ok);
    store
        .record_intent(tool_call_id.clone(), false, Some("no".to_string()))
        .unwrap();

    let done_ok = wait_until(Duration::from_secs(4), || {
        read_queue(root)
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.status == JobStatus::Completed || matches!(j.status, JobStatus::Failed(_)))
            .unwrap_or(false)
    });
    if !done_ok {
        println!(
            "LOGS:\n{}",
            std::fs::read_to_string(root.join(".vac/autopilot.log")).unwrap_or_default()
        );
    }
    assert!(done_ok);
    let status = read_queue(root)
        .iter()
        .find(|j| j.id == id)
        .map(|j| j.status.clone())
        .unwrap();
    assert!(matches!(status, JobStatus::Failed(_)));
    assert!(!root.join("autopilot_reject_test.txt").exists());

    run_vac(root, &["autopilot", "down"]).success();
}

#[test]
fn autopilot_toolcall_stale_approval_errors_and_does_not_resolve_record() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_autopilot_toml(root, "auto", 1);
    write_vac_config_toml(root);
    let job = Job::new(JobKind::ToolCall {
        tool_name: "file_write".to_string(),
        arguments: json!({
            "path": "autopilot_stale_test.txt",
            "content": "hello"
        }),
    });
    write_queue(root, vec![job]);

    run_vac(root, &["autopilot", "up", "--execute"]).success();

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

    let store = vac_approvals::ApprovalStore::new(root.to_path_buf());
    let record_ok = wait_until(Duration::from_secs(4), || {
        store.load(&tool_call_id).ok().flatten().is_some()
    });
    assert!(record_ok);

    run_vac(root, &["autopilot", "down"]).success();

    let approvals = vac_approvals::ApprovalHandle::new(
        root.to_path_buf(),
        vac_approvals::ActiveApprovalRegistry::new(),
    );
    let err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { approvals.approve(tool_call_id.clone()).await })
        .unwrap_err();
    assert!(err.to_string().contains("No active approval channel"));

    let rec = store.load(&tool_call_id).unwrap().unwrap();
    assert_eq!(rec.state, vac_approvals::ApprovalState::Pending);
}

#[test]
fn autopilot_toolcall_wrong_target_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_autopilot_toml(root, "auto", 1);
    write_vac_config_toml(root);
    let job = Job::new(JobKind::ToolCall {
        tool_name: "file_write".to_string(),
        arguments: json!({
            "path": "autopilot_wrong_target_test.txt",
            "content": "hello"
        }),
    });
    let id = job.id;
    write_queue(root, vec![job]);

    run_vac(root, &["autopilot", "up", "--execute"]).success();

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

    let store = vac_approvals::ApprovalStore::new(root.to_path_buf());
    let record_ok = wait_until(Duration::from_secs(4), || {
        store.load(&tool_call_id).ok().flatten().is_some()
    });
    assert!(record_ok);

    let path = store.approvals_dir().join(format!("{tool_call_id}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["task_id"] = serde_json::Value::String(uuid::Uuid::new_v4().to_string());
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    store
        .record_intent(tool_call_id.clone(), true, Some("ok".to_string()))
        .unwrap();

    let done_ok = wait_until(Duration::from_secs(4), || {
        read_queue(root)
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.status == JobStatus::Completed || matches!(j.status, JobStatus::Failed(_)))
            .unwrap_or(false)
    });
    if !done_ok {
        println!(
            "LOGS:\n{}",
            std::fs::read_to_string(root.join(".vac/autopilot.log")).unwrap_or_default()
        );
    }
    assert!(done_ok);

    let status = read_queue(root)
        .iter()
        .find(|j| j.id == id)
        .map(|j| j.status.clone())
        .unwrap();
    assert!(matches!(status, JobStatus::Failed(_)));
    assert!(!root.join("autopilot_wrong_target_test.txt").exists());

    let rec = store.load(&tool_call_id).unwrap().unwrap();
    assert_eq!(rec.state, vac_approvals::ApprovalState::Pending);

    run_vac(root, &["autopilot", "down"]).success();
}
