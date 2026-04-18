use vac_approvals::{ApprovalState, ApprovalStateMachine, ApprovalStore};

#[test]
fn approval_state_machine_rejects_double_resolution() {
    let sm = ApprovalStateMachine::new("tc-1".to_string());
    let sm = sm
        .apply(vac_core::approval::ApprovalEvent::Approve { reason: None })
        .unwrap();
    let err = sm
        .apply(vac_core::approval::ApprovalEvent::Reject { reason: None })
        .unwrap_err();
    assert_eq!(err.to_string(), "approval already resolved");
}

#[test]
fn approval_store_persists_request_and_decision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join(".vac")).unwrap();

    let store = ApprovalStore::new(root.clone());

    let rec = store
        .record_request(
            "tc-abc".to_string(),
            "bash".to_string(),
            serde_json::json!({"command": "git status"}),
            Some("needs approval".to_string()),
            Some(uuid::Uuid::new_v4()),
            Some(uuid::Uuid::new_v4()),
        )
        .unwrap();

    assert_eq!(rec.tool_call_id, "tc-abc");
    assert_eq!(rec.tool_name, "bash");
    assert_eq!(rec.state, ApprovalState::Pending);
    assert_eq!(rec.scope, "bash::git::status");

    let path = store.approvals_dir().join("tc-abc.json");
    assert!(path.exists());

    let rec2 = store
        .record_decision("tc-abc".to_string(), false, Some("no".to_string()))
        .unwrap();
    assert_eq!(rec2.state, ApprovalState::Rejected);
    assert!(rec2.resolved_at.is_some());
    assert_eq!(rec2.reason.as_deref(), Some("no"));

    let loaded = store.load("tc-abc").unwrap().unwrap();
    assert_eq!(loaded.state, ApprovalState::Rejected);
    assert_eq!(loaded.reason.as_deref(), Some("no"));
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn approval_store_handles_arbitrary_strings(
        ref tc_id in "\\PC+",
        ref tool_name in "\\PC+",
        ref reason in "\\PC*",
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".vac")).unwrap();
        let store = ApprovalStore::new(root.clone());

        // Create request
        let rec = store.record_request(
            tc_id.clone(),
            tool_name.clone(),
            serde_json::json!({}),
            if reason.is_empty() { None } else { Some(reason.clone()) },
            None,
            None
        ).unwrap();

        assert_eq!(rec.tool_call_id, *tc_id);
        assert_eq!(rec.tool_name, *tool_name);

        let loaded = store.load(tc_id).unwrap().unwrap();
        assert_eq!(loaded.tool_call_id, *tc_id);
    }
}
