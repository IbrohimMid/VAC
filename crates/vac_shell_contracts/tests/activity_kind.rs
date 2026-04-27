use vac_shell_contracts::ShellActivityKind;
use serde_json;

#[test]
fn activity_kind_serializes_diagnostic_and_status() {
    let kinds = vec![
        ShellActivityKind::Diagnostic,
        ShellActivityKind::Status,
        ShellActivityKind::ToolResult,
    ];

    for kind in kinds {
        let json = serde_json::to_string(&kind).expect("must serialize");
        let roundtrip: ShellActivityKind = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(kind, roundtrip, "roundtrip must preserve {:?}", kind);
    }
}

#[test]
fn all_activity_kinds_have_known_names() {
    let known = [
        "UserInput",
        "AgentThoughtSummary",
        "ToolCall",
        "ToolResult",
        "Diagnostic",
        "Status",
        "FileEdit",
        "ShellCommand",
        "ApprovalRequested",
        "ApprovalResolved",
        "ModelChanged",
        "Error",
    ];

    let json_all = serde_json::to_string(&known).expect("must serialize");
    assert!(json_all.contains("Diagnostic"));
    assert!(json_all.contains("Status"));
}