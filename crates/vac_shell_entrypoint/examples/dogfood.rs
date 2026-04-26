//! D6 — opt-in dogfood entry. Run with:
//!
//! ```bash
//! cargo run -p vac_shell_entrypoint --example dogfood
//! ```
//!
//! Boots the new ShellApp cockpit against the current working
//! directory, drives the crossterm event loop via
//! `vac_shell_runtime_loop`, exits on plain `q` (no overlay
//! open).
//!
//! See `docs/runtime-integration/DOGFOOD_CHECKLIST.md` for the
//! manual operator script.

use std::process::ExitCode;
use std::sync::Arc;
use vac_shell_host_status_command::{StatusReport, StatusReportProvider};

struct DogfoodStatusProvider {
    sessions: Option<Arc<vac_shell_host_sessions::SessionsState>>,
    composition: Option<Arc<vac_shell_composition::ShellComposition>>,
    paths: Arc<dyn vac_shell_contracts::VacPaths>,
}

impl StatusReportProvider for DogfoodStatusProvider {
    fn report(&self) -> StatusReport {
        let sessions_count = self
            .sessions
            .as_ref()
            .map(|s| s.list(self.paths.as_ref()).len())
            .unwrap_or(0);
        let approvals_count = self
            .composition
            .as_ref()
            .map(|c| c.approval_queue.snapshot().len())
            .unwrap_or(0);
        let active_model_label = self
            .composition
            .as_ref()
            .and_then(|c| c.model_state.active_model())
            .map(|(_p, id)| id);

        // Simple doctor check for example
        let doctor_status = if self.paths.project_state_dir().exists() {
            vac_shell_host_doctor::DoctorCheckStatus::Ok
        } else {
            vac_shell_host_doctor::DoctorCheckStatus::Warning
        };

        StatusReport {
            cockpit_status: doctor_status.clone(),
            active_model_label,
            sessions_count,
            approvals_count,
            doctor_status,
            next_action: "Ready for commands. Try /chat or /doctor.".into(),
        }
    }
}

fn main() -> ExitCode {
    let root = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("dogfood: cannot resolve cwd: {e}");
            return ExitCode::FAILURE;
        }
    };
    let app = vac_shell_entrypoint::build_shell_app(&root);
    // D7B — attach the real engine-backed executor adapter via
    // the example/dev path. The library `vac_shell_entrypoint`
    // itself stays free of `vac_session_engine`; only this
    // example pulls it in through the new
    // `vac_shell_host_vac_command_adapter` crate.
    let adapter = Arc::new(
        vac_shell_host_vac_command_adapter::VacCommandExecutorAdapter::new(
            vac_shell_host_vac_command_adapter::AdapterConfig::dogfood(root.clone()),
        ),
    );

    // D12B — wire the doctor command executor.
    let doctor = Arc::new(vac_shell_host_doctor_command::DoctorCommandExecutor {
        paths: Arc::new(vac_shell_host_paths::VacPathsImpl::new(root.clone())),
        activity_log: app
            .activity_log
            .clone()
            .expect("ActivityLog must be attached"),
        config: vac_shell_host_doctor::DoctorConfig::default(),
    });

    // D13 — wire the status command executor.
    let status_provider = Arc::new(DogfoodStatusProvider {
        sessions: app.sessions.clone(),
        composition: app.composition.clone(),
        paths: Arc::new(vac_shell_host_paths::VacPathsImpl::new(root.clone())),
    });
    let status = Arc::new(vac_shell_host_status_command::StatusCommandExecutor {
        activity_log: app.activity_log.clone().unwrap(),
        report_provider: status_provider,
    });

    let composite = Arc::new(vac_shell_host_doctor_command::CompositeExecutor {
        executors: vec![adapter, doctor, status],
    });

    let ctx = vac_shell_runtime_loop::ShellRuntimeContext::new(app).with_executor(composite);
    match vac_shell_runtime_loop::run_shell_loop(
        ctx,
        vac_shell_runtime_loop::ShellLoopOptions::default(),
    ) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("dogfood: shell loop failed: {e}");
            ExitCode::FAILURE
        }
    }
}
