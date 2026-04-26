use std::sync::Arc;
use vac_shell_contracts::{
    Severity, ShellActivityEntry, ShellActivityKind, ShellCommandKind, ShellCommandSpec,
};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_commands::{ShellCommandError, ShellCommandExecutor};
use vac_shell_host_doctor::DoctorCheckStatus;

#[derive(Clone)]
pub struct StatusReport {
    pub cockpit_status: DoctorCheckStatus,
    pub active_model_label: Option<String>,
    pub sessions_count: usize,
    pub approvals_count: usize,
    pub doctor_status: DoctorCheckStatus,
    pub next_action: String,
}

pub trait StatusReportProvider: Send + Sync {
    fn report(&self) -> StatusReport;
}

pub struct StatusCommandExecutor {
    pub activity_log: Arc<ActivityLog>,
    pub report_provider: Arc<dyn StatusReportProvider>,
}

impl ShellCommandExecutor for StatusCommandExecutor {
    fn execute(&self, command: &ShellCommandSpec) -> Result<(), ShellCommandError> {
        if command.slash != "/status" {
            return Err(ShellCommandError::Unsupported(command.slash.clone()));
        }

        let report = self.report_provider.report();

        let ts_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        record_status_report(&self.activity_log, &report, ts_unix);

        Ok(())
    }
}

pub fn record_status_report(activity_log: &ActivityLog, report: &StatusReport, ts_unix: u64) {
    let map_status = |status: DoctorCheckStatus| match status {
        DoctorCheckStatus::Ok => (Severity::Ok, "ok"),
        DoctorCheckStatus::Warning => (Severity::Warn, "warning"),
        DoctorCheckStatus::Error => (Severity::Error, "error"),
        DoctorCheckStatus::Skipped => (Severity::Warn, "skipped"),
    };

    let (cockpit_sev, cockpit_label) = map_status(report.cockpit_status.clone());
    let (doctor_sev, doctor_label) = map_status(report.doctor_status.clone());

    let entries = [
        (
            "cockpit",
            format!("status: cockpit {}", cockpit_label),
            cockpit_sev,
            None,
        ),
        (
            "model",
            format!(
                "status: model {}",
                report.active_model_label.as_deref().unwrap_or("missing")
            ),
            if report.active_model_label.is_some() {
                Severity::Ok
            } else {
                Severity::Warn
            },
            None,
        ),
        (
            "sessions",
            format!("status: sessions {}", report.sessions_count),
            Severity::Ok,
            None,
        ),
        (
            "approvals",
            format!("status: approvals {}", report.approvals_count),
            if report.approvals_count == 0 {
                Severity::Ok
            } else {
                Severity::Warn
            },
            None,
        ),
        (
            "doctor",
            format!("status: doctor {}", doctor_label),
            doctor_sev,
            None,
        ),
        (
            "next-action",
            format!("status: next action {}", report.next_action),
            Severity::Info,
            None,
        ),
    ];

    for (id_part, title, severity, detail) in entries {
        activity_log.record(ShellActivityEntry {
            id: format!("status-{ts_unix}-{id_part}"),
            ts_unix,
            kind: ShellActivityKind::ToolResult, // temporary reuse per D12B pattern
            title,
            detail,
            severity,
        });
    }
}

pub fn status_command_spec() -> ShellCommandSpec {
    ShellCommandSpec {
        id: "status".to_string(),
        slash: "/status".to_string(),
        title: "Status".to_string(),
        description: "Show cockpit status and readiness summary".to_string(),
        kind: ShellCommandKind::PromptTemplate,
        palette_visible: true,
        category: Some("Diagnostics".to_string()),
        ..Default::default()
    }
}

/// Simple delegating executor that tries a list of executors in order.
/// Duplicated from D12B to avoid cross-crate dependency tangle.
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
