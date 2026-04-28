use std::sync::Arc;
use tempfile::tempdir;
use vac_shell_app::ShellApp;
use vac_shell_contracts::{
    Severity, ShellActivityKind, ShellCommandKind, ShellCommandSpec, VacPaths,
};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_commands::{route_palette_command, ShellCommandExecutor};
use vac_shell_host_doctor::{DoctorCheckStatus, DoctorConfig, DoctorReport, ToolDispatcherMode};
use vac_shell_host_doctor_command::{
    doctor_command_spec, record_doctor_report, CompositeExecutor, DoctorCommandExecutor,
};
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

    assert!(snapshot.iter().any(|e| e.id.starts_with("doctor-")));
}

#[test]
fn severity_mapping_is_correct() {
    let activity_log = ActivityLog::new(100);
    let report = DoctorReport {
        overall_status: DoctorCheckStatus::Warning,
        checks: vec![
            vac_shell_host_doctor::DoctorCheck {
                id: "check-ok".into(),
                label: "Check Ok".into(),
                status: DoctorCheckStatus::Ok,
                summary: "ok".into(),
                detail: None,
            },
            vac_shell_host_doctor::DoctorCheck {
                id: "check-warn".into(),
                label: "Check Warn".into(),
                status: DoctorCheckStatus::Warning,
                summary: "warn".into(),
                detail: None,
            },
            vac_shell_host_doctor::DoctorCheck {
                id: "check-err".into(),
                label: "Check Err".into(),
                status: DoctorCheckStatus::Error,
                summary: "err".into(),
                detail: None,
            },
            vac_shell_host_doctor::DoctorCheck {
                id: "check-skipped".into(),
                label: "Check Skipped".into(),
                status: DoctorCheckStatus::Skipped,
                summary: "skipped".into(),
                detail: None,
            },
        ],
    };

    record_doctor_report(&activity_log, &report, 12345);
    let snap = activity_log.snapshot();
    assert_eq!(snap.len(), 4);

    for entry in &snap {
        assert!(entry.id.contains("12345"));
        assert_eq!(entry.kind, ShellActivityKind::Diagnostic);
    }

    let ok = snap.iter().find(|e| e.id.contains("check-ok")).unwrap();
    assert_eq!(ok.severity, Severity::Ok);

    let warn = snap.iter().find(|e| e.id.contains("check-warn")).unwrap();
    assert_eq!(warn.severity, Severity::Warn);

    let err = snap.iter().find(|e| e.id.contains("check-err")).unwrap();
    assert_eq!(err.severity, Severity::Error);

    let skipped = snap
        .iter()
        .find(|e| e.id.contains("check-skipped"))
        .unwrap();
    assert_eq!(skipped.severity, Severity::Warn);
}

#[test]
fn doctor_report_rows_use_diagnostic_kind() {
    let activity_log = ActivityLog::new(100);
    let report = DoctorReport {
        overall_status: DoctorCheckStatus::Ok,
        checks: vec![vac_shell_host_doctor::DoctorCheck {
            id: "model-config".into(),
            label: "Model Config".into(),
            status: DoctorCheckStatus::Ok,
            summary: "loaded".into(),
            detail: None,
        }],
    };

    record_doctor_report(&activity_log, &report, 99999);
    let snap = activity_log.snapshot();

    for entry in &snap {
        assert_eq!(
            entry.kind,
            ShellActivityKind::Diagnostic,
            "every doctor row must be Diagnostic"
        );
        assert!(
            entry.id.starts_with("doctor-"),
            "id must start with doctor-"
        );
        assert!(
            entry.title.starts_with("doctor:"),
            "title must start with doctor:"
        );
    }
}

#[test]
fn route_palette_integration_works() {
    use vac_shell_composition::ShellCompositionBuilder;

    let tmp = tempdir().unwrap();
    let paths = Arc::new(FakeVacPaths(tmp.path().to_path_buf()));
    let activity_log = Arc::new(ActivityLog::new(100));

    let comp = Arc::new(
        ShellCompositionBuilder::new(paths.clone())
            .with_commands(vec![doctor_command_spec()])
            .boot()
            .unwrap(),
    );

    let mut app = ShellApp {
        composition: Some(comp),
        activity_log: Some(activity_log.clone()),
        ..Default::default()
    };

    let executor: Arc<dyn ShellCommandExecutor> = Arc::new(DoctorCommandExecutor {
        paths,
        activity_log: activity_log.clone(),
        config: DoctorConfig::default(),
    });

    route_palette_command(&mut app, Some(&executor), "/doctor").expect("route should succeed");

    let snap = activity_log.snapshot();
    assert!(!snap.is_empty());
    assert!(snap.iter().any(|e| e.title.contains("doctor:")));
}

#[test]
fn composite_executor_fallback_works() {
    struct UnsupportedExecutor;
    impl ShellCommandExecutor for UnsupportedExecutor {
        fn execute(
            &self,
            _cmd: &ShellCommandSpec,
        ) -> Result<(), vac_shell_host_commands::ShellCommandError> {
            Err(vac_shell_host_commands::ShellCommandError::Unsupported(
                "any".into(),
            ))
        }
    }

    let tmp = tempdir().unwrap();
    let paths = Arc::new(FakeVacPaths(tmp.path().to_path_buf()));
    let activity_log = Arc::new(ActivityLog::new(100));

    let doctor_executor = Arc::new(DoctorCommandExecutor {
        paths,
        activity_log: activity_log.clone(),
        config: DoctorConfig::default(),
    });

    let composite = CompositeExecutor {
        executors: vec![Arc::new(UnsupportedExecutor), doctor_executor],
    };

    let spec = doctor_command_spec();
    composite
        .execute(&spec)
        .expect("composite should delegate to doctor");

    assert!(!activity_log.is_empty());
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
          "id": "claude-sonnet-4.5",
          "label": "Claude Sonnet 4.5",
          "reasoning": false,
          "cost_label": "N/A"
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
