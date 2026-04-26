use std::fs;
use tempfile::tempdir;
use vac_shell_contracts::VacPaths;
use vac_shell_host_doctor::*;
use vac_shell_test_support::FakeVacPaths;

#[test]
fn missing_dirs_report_warning_or_error_no_mkdir() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));

    // .vac doesn't exist
    let report = run_doctor_checks(&paths, &DoctorConfig::default());

    let p_check = report.checks.iter().find(|c| c.id == "paths").unwrap();
    assert!(
        p_check.status == DoctorCheckStatus::Warning || p_check.status == DoctorCheckStatus::Error
    );

    // Ensure no mkdir happened
    assert!(!paths.project_state_dir().exists());
    assert!(!paths.sessions_dir().exists());
}

#[test]
fn valid_model_config_parses_as_ok() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));
    fs::create_dir_all(paths.model_config_file().parent().unwrap()).unwrap();

    let config_json = r#"{
      "providers": [
        {"id": "anthropic", "credentials_present": true}
      ],
      "models": [
        {
          "provider": "anthropic",
          "id": "claude-sonnet-4.5",
          "label": "Claude Sonnet 4.5",
          "reasoning": true,
          "cost_label": "$3 / $15 per M"
        }
      ],
      "active": {"provider": "anthropic", "id": "claude-sonnet-4.5"}
    }"#;
    fs::write(paths.model_config_file(), config_json).unwrap();

    let report = run_doctor_checks(&paths, &DoctorConfig::default());
    let c = report
        .checks
        .iter()
        .find(|c| c.id == "model_config")
        .unwrap();
    assert_eq!(c.status, DoctorCheckStatus::Ok);

    let cred = report
        .checks
        .iter()
        .find(|c| c.id == "credentials")
        .unwrap();
    assert_eq!(cred.status, DoctorCheckStatus::Ok);
    // ensure no secret leak
    let dbg = format!("{:?}", report);
    assert!(!dbg.contains("ANTHROPIC_API_KEY"));
}

#[test]
fn corrupt_model_config_reports_error() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));
    fs::create_dir_all(paths.model_config_file().parent().unwrap()).unwrap();
    fs::write(paths.model_config_file(), "{ invalid json").unwrap();

    let report = run_doctor_checks(&paths, &DoctorConfig::default());
    let c = report
        .checks
        .iter()
        .find(|c| c.id == "model_config")
        .unwrap();
    assert_eq!(c.status, DoctorCheckStatus::Error);
}

#[test]
fn missing_model_config_reports_warning() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));
    fs::create_dir_all(paths.project_state_dir()).unwrap();

    let report = run_doctor_checks(&paths, &DoctorConfig::default());
    let c = report
        .checks
        .iter()
        .find(|c| c.id == "model_config")
        .unwrap();
    assert_eq!(c.status, DoctorCheckStatus::Warning);
}

#[test]
fn env_var_present_reports_ok_without_secret_value() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));
    fs::create_dir_all(paths.model_config_file().parent().unwrap()).unwrap();

    let config_json = r#"{
      "providers": [
        {"id": "anthropic", "credentials_present": false}
      ],
      "models": [
        {
          "provider": "anthropic",
          "id": "claude-sonnet-4.5",
          "label": "Claude Sonnet 4.5",
          "reasoning": false,
          "cost_label": "N/A"
        }
      ],
      "active": {"provider": "anthropic", "id": "claude-sonnet-4.5"}
    }"#;
    fs::write(paths.model_config_file(), config_json).unwrap();

    std::env::set_var("ANTHROPIC_API_KEY", "secret_sk_12345");
    let report = run_doctor_checks(&paths, &DoctorConfig::default());
    std::env::remove_var("ANTHROPIC_API_KEY");

    let cred = report
        .checks
        .iter()
        .find(|c| c.id == "credentials")
        .unwrap();
    assert_eq!(cred.status, DoctorCheckStatus::Ok);

    let dbg = format!("{:?}", report);
    assert!(!dbg.contains("secret_sk_12345"));
}

#[test]
fn env_var_missing_reports_warning() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));
    fs::create_dir_all(paths.model_config_file().parent().unwrap()).unwrap();

    let config_json = r#"{
      "providers": [
        {"id": "openai", "credentials_present": false}
      ],
      "models": [
        {
          "provider": "openai",
          "id": "gpt-4",
          "label": "GPT-4",
          "reasoning": false,
          "cost_label": "N/A"
        }
      ],
      "active": {"provider": "openai", "id": "gpt-4"}
    }"#;
    fs::write(paths.model_config_file(), config_json).unwrap();

    std::env::remove_var("OPENAI_API_KEY");
    let report = run_doctor_checks(&paths, &DoctorConfig::default());

    let cred = report
        .checks
        .iter()
        .find(|c| c.id == "credentials")
        .unwrap();
    assert_eq!(cred.status, DoctorCheckStatus::Warning);
}

#[test]
fn dispatcher_mode_reported_inert_or_live() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));

    let r1 = run_doctor_checks(
        &paths,
        &DoctorConfig {
            tool_dispatcher_mode: ToolDispatcherMode::Inert,
            check_boundary_gates: false,
        },
    );
    let c1 = r1
        .checks
        .iter()
        .find(|c| c.id == "dispatcher_mode")
        .unwrap();
    assert!(c1.summary.contains("inert"));

    let r2 = run_doctor_checks(
        &paths,
        &DoctorConfig {
            tool_dispatcher_mode: ToolDispatcherMode::Live,
            check_boundary_gates: false,
        },
    );
    let c2 = r2
        .checks
        .iter()
        .find(|c| c.id == "dispatcher_mode")
        .unwrap();
    assert!(c2.summary.contains("live"));
}

#[test]
fn boundary_gate_script_missing_reports_warning() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));

    let r = run_doctor_checks(
        &paths,
        &DoctorConfig {
            tool_dispatcher_mode: ToolDispatcherMode::Inert,
            check_boundary_gates: true,
        },
    );
    let c = r.checks.iter().find(|c| c.id == "boundary_gates").unwrap();
    assert_eq!(c.status, DoctorCheckStatus::Warning);
}

#[test]
fn doctor_run_does_not_mutate_filesystem() {
    let tmp = tempdir().unwrap();
    let paths = FakeVacPaths(tmp.path().join(".vac"));
    let _ = run_doctor_checks(&paths, &DoctorConfig::default());
    // tmp should still be mostly empty, checking metadata hasn't changed.
    assert!(!paths.project_state_dir().exists());
}
