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

// D10-HARDENING: sensitive key values must be redacted from command_preview.
#[test]
fn secret_keys_are_redacted_in_command_preview() {
    let req = ApprovalRequest::new("id14", "api_call").with_arguments(
        serde_json::json!({"token": "gh_super_secret_abc123", "url": "https://api.example.com"}),
    );
    let detail = provider().detail_for(&req);
    let preview = detail.command_preview.expect("command_preview should be set");
    assert!(
        !preview.contains("gh_super_secret_abc123"),
        "secret token must be redacted, got: {preview}"
    );
    assert!(
        preview.contains("[REDACTED]"),
        "expected [REDACTED] placeholder, got: {preview}"
    );
    // Non-sensitive key value should still be present.
    assert!(
        preview.contains("https://api.example.com"),
        "non-sensitive value must be visible, got: {preview}"
    );
}

#[test]
fn long_arguments_are_truncated_in_command_preview() {
    let long_val = "x".repeat(1000);
    let req = ApprovalRequest::new("id15", "bash_exec")
        .with_arguments(serde_json::json!({"cmd": long_val}));
    let detail = provider().detail_for(&req);
    let preview = detail.command_preview.expect("command_preview should be set");
    assert!(
        preview.len() <= 500,
        "command_preview must be capped at 500 chars, got {} chars",
        preview.len()
    );
}

#[test]
fn normal_command_argument_visible_in_preview() {
    let req = ApprovalRequest::new("id16", "bash_exec")
        .with_arguments(serde_json::json!({"cmd": "git status"}));
    let detail = provider().detail_for(&req);
    let preview = detail.command_preview.expect("command_preview should be set");
    assert!(
        preview.contains("git status"),
        "safe command must be visible in preview, got: {preview}"
    );
}
