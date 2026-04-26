//! Dogfood doctor example. Run with:
//!
//! ```bash
//! cargo run -p vac_shell_entrypoint --example dogfood_doctor
//! ```
//!
//! Runs the doctor diagnostic engine against the current working
//! directory and prints the report to stdout.

use std::process::ExitCode;
use vac_shell_contracts::VacPaths;
use vac_shell_host_doctor::{DoctorCheckStatus, DoctorConfig, run_doctor_checks};
use vac_shell_host_paths::VacPathsImpl;

fn main() -> ExitCode {
    let root = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("doctor: cannot resolve cwd: {e}");
            return ExitCode::FAILURE;
        }
    };

    let paths = VacPathsImpl::new(root);
    let config = DoctorConfig {
        tool_dispatcher_mode: vac_shell_host_doctor::ToolDispatcherMode::Inert,
        check_boundary_gates: true,
    };

    println!("Running VAC Dogfood Doctor...");
    println!("Root: {}\n", paths.project_root().display());

    let report = run_doctor_checks(&paths, &config);

    for check in &report.checks {
        let status_icon = match check.status {
            DoctorCheckStatus::Ok => "✓",
            DoctorCheckStatus::Warning => "!",
            DoctorCheckStatus::Error => "✗",
            DoctorCheckStatus::Skipped => "-",
        };
        println!("{} {:<25} — {:?}", status_icon, check.label, check.status);
        println!("  Summary: {}", check.summary);
        if let Some(ref detail) = check.detail {
            println!("  Detail:  {}", detail);
        }
        println!();
    }

    println!("Overall Status: {:?}", report.overall_status);

    if report.overall_status == DoctorCheckStatus::Error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
