use serde_json;
use vac_shell_contracts::ShellActivityKind;

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
fn all_activity_kind_variants_serialize_and_deserialize() {
    let variants = [
        ShellActivityKind::UserInput,
        ShellActivityKind::AgentThoughtSummary,
        ShellActivityKind::ToolCall,
        ShellActivityKind::ToolResult,
        ShellActivityKind::Diagnostic,
        ShellActivityKind::Status,
        ShellActivityKind::FileEdit,
        ShellActivityKind::ShellCommand,
        ShellActivityKind::ApprovalRequested,
        ShellActivityKind::ApprovalResolved,
        ShellActivityKind::ModelChanged,
        ShellActivityKind::Error,
    ];

    for variant in variants {
        let json = serde_json::to_string(&variant).expect("must serialize");
        let roundtrip: ShellActivityKind = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(variant, roundtrip, "roundtrip must preserve {:?}", variant);
    }
}
