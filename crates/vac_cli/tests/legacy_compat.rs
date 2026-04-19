#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

#[test]
fn legacy_v0_session_loads_and_upgrades() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/legacy/v0_session.json");
    let content = std::fs::read_to_string(&fixture).unwrap();
    let session: vac_core::Session = serde_json::from_str(&content).unwrap();

    // v0 fixtures have no schema_version field → serde default = 0
    assert_eq!(session.schema_version, 0);
    assert_eq!(
        session.id,
        uuid::Uuid::parse_str("a1b2c3d4-e5f6-7890-abcd-ef1234567890").unwrap()
    );
    assert_eq!(session.metadata.total_tokens_used, 1000);
    assert_eq!(session.metadata.total_tasks_completed, 2);
}

#[test]
fn legacy_v0_queue_loads_and_upgrades() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/legacy/v0_queue.json");
    let content = std::fs::read_to_string(&fixture).unwrap();
    let jobs: Vec<vac_runtime::Job> = serde_json::from_str(&content).unwrap();

    assert_eq!(jobs.len(), 1);
    // v0 fixtures have no schema_version → default = 0
    assert_eq!(jobs[0].schema_version, 0);
    assert_eq!(
        jobs[0].id,
        uuid::Uuid::parse_str("b2c3d4e5-f6a7-8901-bcde-f12345678901").unwrap()
    );
}

#[test]
fn current_session_has_schema_version_1() {
    let session = vac_core::Session::new(std::path::PathBuf::from("/tmp/test"));
    assert_eq!(session.schema_version, 1);
}

#[test]
fn current_job_has_schema_version_1() {
    let job = vac_runtime::Job::new(vac_runtime::JobKind::DiagnosticSweep);
    assert_eq!(job.schema_version, 1);
}
