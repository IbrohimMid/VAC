use std::path::{Path, PathBuf};
use tempfile::tempdir;
use uuid::Uuid;

fn seed_source_project(summary: Option<String>) -> (tempfile::TempDir, PathBuf, Uuid) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    std::fs::create_dir_all(root.join(".vac/checkpoints")).unwrap();
    std::fs::create_dir_all(root.join(".vac/sessions")).unwrap();

    let session = vac_core::Session::new(root.clone());
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

    let state_path = root
        .join(".vac/checkpoints")
        .join(format!("{session_id}_state.json"));
    state
        .save_checkpoint(&state_path, Some(session_id))
        .unwrap();

    let store = vac_core::ApprovalStore::new(root.clone());
    store
        .record_request(
            "tc-1".to_string(),
            "file_write".to_string(),
            serde_json::json!({"secret_key":"supersecret","path":"x.txt"}),
            Some("needs approval".to_string()),
            Some(session_id),
            Some(Uuid::new_v4()),
        )
        .unwrap();
    store
        .record_decision("tc-1".to_string(), true, Some("ok".to_string()))
        .unwrap();

    if let Some(summary) = summary {
        std::fs::write(root.join("summary.md"), summary).unwrap();
    }

    (dir, root, session_id)
}

fn export_bundle(root: &Path, session_id: Uuid, sign: bool) -> PathBuf {
    let out = root
        .join(".vac/exports")
        .join(format!("{session_id}.bundle.json"));
    vac_core::bundle::export_bundle_to_path_with_options(
        root,
        Some(session_id),
        Some(&out),
        vac_core::bundle::BundleExportOptions {
            redact_secrets: true,
            sign,
        },
    )
    .unwrap()
}

#[test]
fn bundle_export_uses_requested_session_not_latest() {
    let (_dir, root, first_session_id) = seed_source_project(None);

    let second_session = vac_core::Session::new(root.to_path_buf());
    second_session.save().unwrap();

    let out_path = root
        .join(".vac/exports")
        .join(format!("{first_session_id}.bundle.json"));
    vac_core::bundle::export_bundle_to_path_with_options(
        &root,
        Some(first_session_id),
        Some(&out_path),
        vac_core::bundle::BundleExportOptions {
            redact_secrets: true,
            sign: false,
        },
    )
    .unwrap();

    let bundle: vac_core::VacBundle =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    assert_eq!(bundle.metadata.session_id, first_session_id);
}

#[test]
fn bundle_redaction_and_round_trip() {
    let (_dir1, root1, session_id) = seed_source_project(Some(
        "Context summary with password=hunter2hunter2".to_string(),
    ));

    let out1_path = export_bundle(&root1, session_id, false);
    let raw1 = std::fs::read_to_string(&out1_path).unwrap();
    assert!(!raw1.contains("sk_test_1234567890abcdefghij"));
    assert!(!raw1.contains("hunter2hunter2"));
    assert!(!raw1.contains("supersecret"));

    let dir2 = tempdir().unwrap();
    let root2 = dir2.path();
    vac_core::bundle::import_bundle_from_path(root2, &out1_path).unwrap();

    let out2_path = export_bundle(root2, session_id, false);

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

#[test]
fn signed_bundle_round_trips_and_requires_explicit_trust_for_approvals() {
    let (_dir1, root1, session_id) = seed_source_project(Some(
        "Context summary with password=hunter2hunter2".to_string(),
    ));

    let out1_path = export_bundle(&root1, session_id, true);
    let bundle: vac_core::VacBundle =
        serde_json::from_str(&std::fs::read_to_string(&out1_path).unwrap()).unwrap();
    assert!(bundle.metadata.signature.is_some());

    let dir2 = tempdir().unwrap();
    let root2 = dir2.path();
    vac_core::bundle::import_bundle_from_path_with_options(
        root2,
        &out1_path,
        vac_core::bundle::BundleImportOptions {
            require_signed: true,
            overwrite_session: false,
            trust_approvals: false,
            redact_on_import: true,
        },
    )
    .unwrap();

    let state_path = root2
        .join(".vac/checkpoints")
        .join(format!("{session_id}_state.json"));
    let state = vil_swarm::run_state::AgentRunState::from_checkpoint(&state_path).unwrap();
    assert!(state.approved_tools.is_empty());

    let out2_path = export_bundle(root2, session_id, false);
    let raw2 = std::fs::read_to_string(&out2_path).unwrap();
    let b2: vac_core::VacBundle = serde_json::from_str(&raw2).unwrap();
    assert_eq!(bundle.transcript.len(), b2.transcript.len());
}

#[test]
fn import_rejects_unsigned_bundle_when_signature_is_required() {
    let (_dir1, root1, session_id) = seed_source_project(None);
    let out1_path = export_bundle(&root1, session_id, false);

    let dir2 = tempdir().unwrap();
    let root2 = dir2.path();
    let err = vac_core::bundle::import_bundle_from_path_with_options(
        root2,
        &out1_path,
        vac_core::bundle::BundleImportOptions {
            require_signed: true,
            overwrite_session: false,
            trust_approvals: false,
            redact_on_import: true,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("required but missing"));
}

#[test]
fn import_rejects_tampered_signed_bundle() {
    let (_dir1, root1, session_id) = seed_source_project(None);
    let out1_path = export_bundle(&root1, session_id, true);

    let tampered_dir = tempdir().unwrap();
    let tampered_path = tampered_dir
        .path()
        .join(format!("{session_id}.tampered.bundle.json"));
    let mut bundle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out1_path).unwrap()).unwrap();
    bundle["metadata"]["redacted"] = serde_json::Value::Bool(false);
    std::fs::write(
        &tampered_path,
        serde_json::to_string_pretty(&bundle).unwrap(),
    )
    .unwrap();

    let dir2 = tempdir().unwrap();
    let root2 = dir2.path();
    let err = vac_core::bundle::import_bundle_from_path_with_options(
        root2,
        &tampered_path,
        vac_core::bundle::BundleImportOptions {
            require_signed: true,
            overwrite_session: false,
            trust_approvals: false,
            redact_on_import: true,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("verification failed"));
}

#[test]
fn import_rejects_session_collision_without_overwrite() {
    let (_dir1, root1, session_id) = seed_source_project(None);
    let out1_path = export_bundle(&root1, session_id, false);

    let dir2 = tempdir().unwrap();
    let root2 = dir2.path();
    vac_core::bundle::import_bundle_from_path(root2, &out1_path).unwrap();

    let err = vac_core::bundle::import_bundle_from_path(root2, &out1_path).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("already exists"));
}

#[test]
fn import_rejects_malformed_json_and_oversized_summary() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let malformed = root.join("malformed.bundle.json");
    std::fs::write(&malformed, "{not json").unwrap();
    assert!(vac_core::bundle::import_bundle_from_path(root, &malformed).is_err());

    let huge_summary = "x".repeat(10 * 1024 * 1024 + 1);
    let bundle = vac_core::VacBundle {
        metadata: vac_core::BundleMetadata {
            version: "0.1.0".to_string(),
            session_id: Uuid::new_v4(),
            created_at: chrono::Utc::now(),
            exported_at: chrono::Utc::now(),
            redacted: true,
            session_metadata: Default::default(),
            signature: None,
        },
        transcript: Vec::new(),
        approvals: Vec::new(),
        context_summary: Some(huge_summary),
    };
    let oversized = root.join("oversized.bundle.json");
    std::fs::write(&oversized, serde_json::to_string_pretty(&bundle).unwrap()).unwrap();
    let err = vac_core::bundle::import_bundle_from_path(root, &oversized).unwrap_err();
    assert!(err.to_string().contains("byte cap"));
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn bundle_roundtrips_arbitrary_summary(ref summary in "\\PC*") {
        let (_dir1, root1, session_id) = seed_source_project(Some(summary.clone()));

        let out1_path = export_bundle(&root1, session_id, false);

        let dir2 = tempdir().unwrap();
        let root2 = dir2.path();
        vac_core::bundle::import_bundle_from_path(root2, &out1_path).unwrap();

        let out2_path = export_bundle(root2, session_id, false);

        let raw1 = std::fs::read_to_string(&out1_path).unwrap();
        let b1: vac_core::VacBundle = serde_json::from_str(&raw1).unwrap();

        let raw2 = std::fs::read_to_string(&out2_path).unwrap();
        let b2: vac_core::VacBundle = serde_json::from_str(&raw2).unwrap();

        assert_eq!(
            serde_json::to_value(&b1.context_summary).unwrap(),
            serde_json::to_value(&b2.context_summary).unwrap()
        );
    }

    #[test]
    fn secret_substitution_idempotence(input in ".*") {
        let mut sub = vac_core::security::SecretSubstitution::new();
        let first = sub.substitute(&input);
        let second = sub.substitute(&first);
        // After first substitution, a second pass must not change the output
        // (placeholders like [SECRET_N] must not themselves be detected as secrets).
        prop_assert_eq!(&first, &second,
            "secret substitution must be idempotent: input={:?}", input);
    }
}
