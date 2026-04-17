use tempfile::tempdir;

#[test]
fn bundle_redaction_and_round_trip() {
    let dir1 = tempdir().unwrap();
    let root1 = dir1.path();

    std::fs::create_dir_all(root1.join(".vac/checkpoints")).unwrap();
    std::fs::create_dir_all(root1.join(".vac/sessions")).unwrap();

    let session = vac_core::Session::new(root1.to_path_buf());
    session.save().unwrap();
    let session_id = session.id;

    let mut state = vil_swarm::run_state::AgentRunState::new(
        vec![
            vil_llm::provider::Message::system("sys".to_string()),
            vil_llm::provider::Message::user("api_key=sk_test_1234567890abcdefghij".to_string()),
            vil_llm::provider::Message::assistant_with_tool_calls(
                "running tool".to_string(),
                vec![vil_llm::provider::ToolCall {
                    id: "tc-1".to_string(),
                    name: "file_write".to_string(),
                    arguments: serde_json::json!({
                        "path": "x.txt",
                        "content": "password=hunter2hunter2",
                        "token": "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"
                    }),
                }],
            ),
        ],
        None,
    );
    state.approved_tools.insert("tc-1".to_string());
    state.stage = vil_swarm::run_state::RunStage::Completed;

    let state_path = root1
        .join(".vac/checkpoints")
        .join(format!("{session_id}_state.json"));
    state
        .save_checkpoint(&state_path, Some(session_id))
        .unwrap();

    let store = vac_core::ApprovalStore::new(root1.to_path_buf());
    store
        .record_request(
            "tc-1".to_string(),
            "file_write".to_string(),
            serde_json::json!({"secret_key":"supersecret","path":"x.txt"}),
            Some("needs approval".to_string()),
            Some(session_id),
            Some(uuid::Uuid::new_v4()),
        )
        .unwrap();
    store
        .record_decision("tc-1".to_string(), true, Some("ok".to_string()))
        .unwrap();

    std::fs::write(
        root1.join("summary.md"),
        "Context summary with password=hunter2hunter2",
    )
    .unwrap();

    let out1 = root1.join(".vac/exports/b1.bundle.json");
    let out1_path =
        vac_core::bundle::export_bundle_to_path(root1, Some(session_id), Some(&out1), true)
            .unwrap();
    let raw1 = std::fs::read_to_string(&out1_path).unwrap();
    assert!(!raw1.contains("sk_test_1234567890abcdefghij"));
    assert!(!raw1.contains("hunter2hunter2"));
    assert!(!raw1.contains("supersecret"));

    let dir2 = tempdir().unwrap();
    let root2 = dir2.path();
    vac_core::bundle::import_bundle_from_path(root2, &out1_path).unwrap();

    let out2 = root2.join(".vac/exports/b2.bundle.json");
    let out2_path =
        vac_core::bundle::export_bundle_to_path(root2, Some(session_id), Some(&out2), true)
            .unwrap();

    let b1: vac_core::VacBundle = serde_json::from_str(&raw1).unwrap();
    let raw2 = std::fs::read_to_string(&out2_path).unwrap();
    let b2: vac_core::VacBundle = serde_json::from_str(&raw2).unwrap();

    let mut meta1 = serde_json::to_value(&b1.metadata).unwrap();
    let mut meta2 = serde_json::to_value(&b2.metadata).unwrap();
    meta1.as_object_mut().unwrap().remove("exported_at");
    meta2.as_object_mut().unwrap().remove("exported_at");

    assert_eq!(meta1, meta2);
    assert_eq!(
        serde_json::to_value(&b1.transcript).unwrap(),
        serde_json::to_value(&b2.transcript).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&b1.approvals).unwrap(),
        serde_json::to_value(&b2.approvals).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&b1.context_summary).unwrap(),
        serde_json::to_value(&b2.context_summary).unwrap()
    );
}
