use std::sync::Arc;
use vac_shell_contracts::{
    Severity, ShellActivityEntry, ShellActivityKind, ShellCommandKind, ShellCommandSpec, VacPaths,
};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_commands::{ShellCommandError, ShellCommandExecutor};
use vac_shell_host_doctor::{run_doctor_checks, DoctorCheckStatus, DoctorConfig, DoctorReport};

pub struct DoctorCommandExecutor {
    pub paths: Arc<dyn VacPaths>,
    pub activity_log: Arc<ActivityLog>,
    pub config: DoctorConfig,
}

impl ShellCommandExecutor for DoctorCommandExecutor {
    fn execute(&self, command: &ShellCommandSpec) -> Result<(), ShellCommandError> {
        if command.slash != "/doctor" {
            return Err(ShellCommandError::Unsupported(command.slash.clone()));
        }

        let report = run_doctor_checks(self.paths.as_ref(), &self.config);

        let ts_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        record_doctor_report(&self.activity_log, &report, ts_unix);

        Ok(())
    }
}

pub fn record_doctor_report(activity_log: &ActivityLog, report: &DoctorReport, ts_unix: u64) {
    for check in &report.checks {
        let severity = match check.status {
            DoctorCheckStatus::Ok => Severity::Ok,
            DoctorCheckStatus::Warning => Severity::Warn,
            DoctorCheckStatus::Error => Severity::Error,
            DoctorCheckStatus::Skipped => Severity::Warn,
        };

        let status_str = match check.status {
            DoctorCheckStatus::Ok => "Ok",
            DoctorCheckStatus::Warning => "Warning",
            DoctorCheckStatus::Error => "Error",
            DoctorCheckStatus::Skipped => "Skipped",
        };

        let title = format!("doctor: {} — {}", check.label, status_str);

        let mut detail = check.summary.clone();
        if let Some(ref d) = check.detail {
            detail.push('\n');
            detail.push_str(d);
        }

        let id = format!("doctor-{}-{ts_unix}", check.id);

        activity_log.record(ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::ToolResult, // Reusing existing kind or we could add DoctorReport? Using ToolResult to fit nicely into activity
            title,
            detail: Some(detail),
            severity,
        });
    }
}

pub fn doctor_command_spec() -> ShellCommandSpec {
    ShellCommandSpec {
        id: "doctor".to_string(),
        slash: "/doctor".to_string(),
        title: "Doctor".to_string(),
        description: "Run readiness checks for local environment".to_string(),
        kind: ShellCommandKind::PromptTemplate,
        palette_visible: true,
        shortcut: None,
        category: Some("Diagnostics".to_string()),
        aliases: vec![],
        keywords: vec![
            "doctor".to_string(),
            "readiness".to_string(),
            "check".to_string(),
        ],
        disabled_reason: None,
    }
}

/// Simple delegating executor that tries a list of executors in order.
pub struct CompositeExecutor {
    pub executors: Vec<Arc<dyn ShellCommandExecutor>>,
}

impl ShellCommandExecutor for CompositeExecutor {
    fn execute(&self, command: &ShellCommandSpec) -> Result<(), ShellCommandError> {
        for executor in &self.executors {
            match executor.execute(command) {
                Err(ShellCommandError::Unsupported(_)) => continue,
                other => return other,
            }
        }
        Err(ShellCommandError::Unsupported(command.slash.clone()))
    }
}
