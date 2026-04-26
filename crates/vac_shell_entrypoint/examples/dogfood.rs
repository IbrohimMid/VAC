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

    let composite = Arc::new(vac_shell_host_doctor_command::CompositeExecutor {
        executors: vec![adapter, doctor],
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
