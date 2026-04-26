use std::sync::Arc;
use tempfile::tempdir;
use vac_shell_contracts::{Severity, ShellCommandKind};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_commands::ShellCommandExecutor;
use vac_shell_host_doctor::{DoctorConfig, ToolDispatcherMode};
use vac_shell_host_doctor_command::{doctor_command_spec, DoctorCommandExecutor};
use vac_shell_test_support::FakeVacPaths;

#[test]
fn doctor_command_spec_is_correct() {
    let spec = doctor_command_spec();
    assert_eq!(spec.slash, "/doctor");
    assert!(spec.palette_visible);
    assert_eq!(spec.kind, ShellCommandKind::PromptTemplate);
    assert_eq!(spec.category.as_deref(), Some("Diagnostics"));
}

#[test]
fn executor_rejects_other_commands() {
    let tmp = tempdir().unwrap();
    let paths = Arc::new(FakeVacPaths(tmp.path().to_path_buf()));
    let activity_log = Arc::new(ActivityLog::new(100));
    let config = DoctorConfig::default();

    let executor = DoctorCommandExecutor {
        paths,
        activity_log,
        config,
    };

    let mut spec = doctor_command_spec();
    spec.slash = "/other".into();

    let res = executor.execute(&spec);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.to_string(), "command unsupported: /other");
}

#[test]
fn executor_writes_activity_log_on_success() {
    let tmp = tempdir().unwrap();
    let paths = Arc::new(FakeVacPaths(tmp.path().join(".vac")));
    let activity_log = Arc::new(ActivityLog::new(100));

    // We'll intentionally trigger a warning to test mapping
    let config = DoctorConfig {
        tool_dispatcher_mode: ToolDispatcherMode::Inert,
        check_boundary_gates: true,
    };

    let executor = DoctorCommandExecutor {
        paths,
        activity_log: activity_log.clone(),
        config,
    };

    let spec = doctor_command_spec();
    assert!(executor.execute(&spec).is_ok());

    let snapshot = activity_log.snapshot();
    assert!(!snapshot.is_empty(), "Should have written to activity log");

    let mut found_warning = false;
    let mut found_ok = false;

    for entry in snapshot {
        assert!(entry.id.starts_with("doctor-"));
        if entry.severity == Severity::Warn {
            found_warning = true;
        }
        if entry.severity == Severity::Ok {
            found_ok = true;
        }
    }

    assert!(found_warning);
    assert!(found_ok);
}

#[test]
fn secret_env_var_absent_from_activity_log() {
    let tmp = tempdir().unwrap();
    let paths = Arc::new(FakeVacPaths(tmp.path().join(".vac")));
    std::fs::create_dir_all(paths.0.join("model_config.json").parent().unwrap()).unwrap();

    let config_json = r#"{
      "providers": [
        {"id": "anthropic", "credentials_present": false}
      ],
      "models": [
        {
          "provider": "anthropic",
          "id": "claude-sonnet-4.5"
        }
      ],
      "active": {"provider": "anthropic", "id": "claude-sonnet-4.5"}
    }"#;
    std::fs::write(paths.0.join("model_config.json"), config_json).unwrap();

    let activity_log = Arc::new(ActivityLog::new(100));
    let executor = DoctorCommandExecutor {
        paths,
        activity_log: activity_log.clone(),
        config: DoctorConfig::default(),
    };

    std::env::set_var("ANTHROPIC_API_KEY", "super_secret_key_123");
    let spec = doctor_command_spec();
    executor.execute(&spec).unwrap();
    std::env::remove_var("ANTHROPIC_API_KEY");

    let snapshot = activity_log.snapshot();
    let dbg = format!("{:?}", snapshot);
    assert!(!dbg.contains("super_secret_key_123"));
}
