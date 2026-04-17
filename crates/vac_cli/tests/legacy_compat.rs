use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_vac_migrate_legacy_schema() {
    let dir = tempdir().unwrap();
    let project_root = dir.path();
    let vac_dir = project_root.join(".vac");
    fs::create_dir_all(&vac_dir).unwrap();

    // Create legacy schema files
    // Let's assume an old version didn't have schema_version file
    let legacy_config = vac_dir.join("legacy_config.json");
    fs::write(&legacy_config, r#"{"old_key": "old_value"}"#).unwrap();

    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("-C")
        .arg(project_root)
        .arg("migrate")
        .assert()
        .success();

    let schema_file = vac_dir.join("schema_version");
    assert!(schema_file.exists());
    assert_eq!(fs::read_to_string(schema_file).unwrap().trim(), "1.0.0");
}
