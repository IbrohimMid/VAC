//! D10 — ApprovalDetailProvider contract tests.

use vac_shell_contracts::RiskLevel;
use vac_shell_host_approval::{ApprovalDetailProvider, ApprovalRequest, DefaultApprovalDetailProvider};

fn provider() -> DefaultApprovalDetailProvider {
    DefaultApprovalDetailProvider
}

#[test]
fn classify_bash_exec_as_high() {
    let req = ApprovalRequest::new("id1", "bash_exec");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::High);
}

#[test]
fn classify_shell_run_as_high() {
    let req = ApprovalRequest::new("id2", "shell_run");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::High);
}

#[test]
fn classify_file_write_as_high() {
    let req = ApprovalRequest::new("id3", "file_write");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::High);
}

#[test]
fn classify_file_delete_as_critical() {
    let req = ApprovalRequest::new("id4", "file_delete");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::Critical);
}

#[test]
fn classify_rm_files_as_critical() {
    let req = ApprovalRequest::new("id5", "rm_files");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::Critical);
}

#[test]
fn classify_drop_table_as_critical() {
    let req = ApprovalRequest::new("id6", "drop_table");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::Critical);
}

#[test]
fn classify_glob_find_as_low() {
    let req = ApprovalRequest::new("id7", "glob_find");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::Low);
}

#[test]
fn classify_ls_dir_as_low() {
    let req = ApprovalRequest::new("id8", "ls_dir");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::Low);
}

#[test]
fn classify_file_read_as_low() {
    let req = ApprovalRequest::new("id9", "file_read");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::Low);
}

#[test]
fn unknown_tool_defaults_to_medium() {
    let req = ApprovalRequest::new("id10", "some_unknown_tool");
    let detail = provider().detail_for(&req);
    assert_eq!(detail.risk_level, RiskLevel::Medium);
}

#[test]
fn arguments_appear_as_command_preview() {
    let req = ApprovalRequest::new("id11", "bash_exec")
        .with_arguments(serde_json::json!({"cmd": "ls -la /tmp"}));
    let detail = provider().detail_for(&req);
    let preview = detail.command_preview.expect("command_preview should be set");
    assert!(
        preview.contains("ls -la /tmp"),
        "expected cmd in preview, got: {preview}"
    );
}

#[test]
fn no_arguments_yields_no_command_preview() {
    let req = ApprovalRequest::new("id12", "bash_exec");
    let detail = provider().detail_for(&req);
    assert!(
        detail.command_preview.is_none(),
        "expected None when no arguments, got: {:?}",
        detail.command_preview
    );
}

#[test]
fn reason_contains_tool_name() {
    let req = ApprovalRequest::new("id13", "my_custom_tool");
    let detail = provider().detail_for(&req);
    assert!(
        detail.reason.contains("my_custom_tool"),
        "expected tool name in reason, got: {}",
        detail.reason
    );
}
