use std::sync::Arc;
use vac_shell_app::ShellApp;
use vac_shell_contracts::{Severity, ShellActivityKind, ShellCommandKind, ShellCommandSpec};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_commands::{route_palette_command, ShellCommandExecutor};
use vac_shell_host_doctor::DoctorCheckStatus;
use vac_shell_host_status_command::*;
use vac_shell_test_support::FakeVacPaths;

struct FakeStatusProvider {
    report: StatusReport,
}

impl StatusReportProvider for FakeStatusProvider {
    fn report(&self) -> StatusReport {
        self.report.clone()
    }
}

fn dummy_report() -> StatusReport {
    StatusReport {
        cockpit_status: DoctorCheckStatus::Ok,
        active_model_label: Some("test-model".into()),
        sessions_count: 5,
        approvals_count: 0,
        doctor_status: DoctorCheckStatus::Ok,
        next_action: "test action".into(),
    }
}

#[test]
fn status_command_spec_is_correct() {
    let spec = status_command_spec();
    assert_eq!(spec.id, "status");
    assert_eq!(spec.slash, "/status");
    assert_eq!(spec.title, "Status");
    assert_eq!(spec.category.as_deref(), Some("Diagnostics"));
    assert_eq!(spec.kind, ShellCommandKind::PromptTemplate);
    assert!(spec.palette_visible);
    assert_eq!(spec.description, "Show cockpit status and readiness summary");
}

#[test]
fn executor_rejects_other_commands() {
    let log = Arc::new(ActivityLog::new(10));
    let provider = Arc::new(FakeStatusProvider {
        report: dummy_report(),
    });
    let executor = StatusCommandExecutor {
        activity_log: log,
        report_provider: provider,
    };

    let mut spec = status_command_spec();
    spec.slash = "/other".into();

    let res = executor.execute(&spec);
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("unsupported"));
}

#[test]
fn executor_writes_activity_log_on_success() {
    let log = Arc::new(ActivityLog::new(10));
    let provider = Arc::new(FakeStatusProvider {
        report: dummy_report(),
    });
    let executor = StatusCommandExecutor {
        activity_log: log.clone(),
        report_provider: provider,
    };

    let spec = status_command_spec();
    executor.execute(&spec).expect("execute must succeed");

    let snap = log.snapshot();
    assert_eq!(snap.len(), 6);
    assert!(snap.iter().any(|e| e.title.contains("status: cockpit ok")));
}

#[test]
fn severity_mapping_is_correct() {
    let test_map = |status: DoctorCheckStatus, expected: Severity| {
        let log = ActivityLog::new(10);
        let report = StatusReport {
            cockpit_status: status.clone(),
            active_model_label: None,
            sessions_count: 0,
            approvals_count: 0,
            doctor_status: status,
            next_action: "".into(),
        };
        record_status_report(&log, &report, 123);
        let snap = log.snapshot();
        let cockpit = snap.iter().find(|e| e.id.contains("cockpit")).unwrap();
        let doctor = snap.iter().find(|e| e.id.contains("doctor")).unwrap();
        assert_eq!(cockpit.severity, expected);
        assert_eq!(doctor.severity, expected);
    };

    test_map(DoctorCheckStatus::Ok, Severity::Ok);
    test_map(DoctorCheckStatus::Warning, Severity::Warn);
    test_map(DoctorCheckStatus::Error, Severity::Error);
    test_map(DoctorCheckStatus::Skipped, Severity::Warn);
}

#[test]
fn record_status_report_writes_exact_rows() {
    let log = ActivityLog::new(10);
    let report = StatusReport {
        cockpit_status: DoctorCheckStatus::Warning,
        active_model_label: Some("claude".into()),
        sessions_count: 2,
        approvals_count: 1,
        doctor_status: DoctorCheckStatus::Ok,
        next_action: "fix it".into(),
    };

    let ts = 999;
    record_status_report(&log, &report, ts);
    let snap = log.snapshot();

    assert_eq!(snap.len(), 6);
    for entry in &snap {
        assert!(entry.id.contains(&ts.to_string()));
        assert_eq!(entry.kind, ShellActivityKind::ToolResult);
    }

    let find_title = |id: &str| {
        snap.iter()
            .find(|e| e.id.contains(id))
            .unwrap()
            .title
            .clone()
    };

    assert_eq!(find_title("cockpit"), "status: cockpit warning");
    assert_eq!(find_title("model"), "status: model claude");
    assert_eq!(find_title("sessions"), "status: sessions 2");
    assert_eq!(find_title("approvals"), "status: approvals 1");
    assert_eq!(find_title("doctor"), "status: doctor ok");
    assert_eq!(find_title("next-action"), "status: next action fix it");
}

#[test]
fn route_palette_integration_works() {
    use vac_shell_composition::ShellCompositionBuilder;
    let tmp = tempfile::tempdir().unwrap();
    let paths = Arc::new(FakeVacPaths(tmp.path().to_path_buf()));
    let log = Arc::new(ActivityLog::new(10));

    let comp = Arc::new(
        ShellCompositionBuilder::new(paths)
            .with_commands(vec![status_command_spec()])
            .boot()
            .unwrap(),
    );

    let mut app = ShellApp {
        composition: Some(comp),
        activity_log: Some(log.clone()),
        ..Default::default()
    };

    let provider = Arc::new(FakeStatusProvider {
        report: dummy_report(),
    });
    let executor: Arc<dyn ShellCommandExecutor> = Arc::new(StatusCommandExecutor {
        activity_log: log.clone(),
        report_provider: provider,
    });

    route_palette_command(&mut app, Some(&executor), "/status").expect("route should work");
    assert!(!log.is_empty());
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

    let log = Arc::new(ActivityLog::new(10));
    let provider = Arc::new(FakeStatusProvider {
        report: dummy_report(),
    });
    let status_executor = Arc::new(StatusCommandExecutor {
        activity_log: log.clone(),
        report_provider: provider,
    });

    let composite = CompositeExecutor {
        executors: vec![Arc::new(UnsupportedExecutor), status_executor],
    };

    let spec = status_command_spec();
    composite
        .execute(&spec)
        .expect("composite should delegate to status");
    assert!(!log.is_empty());
}

#[test]
fn no_secret_leak_proof() {
    let log = Arc::new(ActivityLog::new(10));
    let provider = Arc::new(FakeStatusProvider {
        report: dummy_report(),
    });
    let executor = StatusCommandExecutor {
        activity_log: log.clone(),
        report_provider: provider,
    };

    std::env::set_var("VAC_SECRET_PROBE", "super-secret-123");
    let spec = status_command_spec();
    executor.execute(&spec).unwrap();

    let dbg = format!("{:?}", log.snapshot());
    assert!(!dbg.contains("super-secret-123"));
    std::env::remove_var("VAC_SECRET_PROBE");
}
