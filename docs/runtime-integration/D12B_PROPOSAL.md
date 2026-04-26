# D12B Proposal: Wire Doctor Report into Operator Surface

## Goal
Wire the `vac_shell_host_doctor` diagnostic engine into the operator surface so users can easily view the readiness report within the shell cockpit or via the CLI, bridging the remaining gap in the product lifecycle.

## Crate Plan
To keep the D12 core pristine and isolate command wiring, a new crate will be created:
- **New Crate:** `crates/vac_shell_host_doctor_command`
- **Depends on:**
  - `vac_shell_host_doctor`
  - `vac_shell_host_commands`
  - `vac_shell_host_activity`
  - `vac_shell_contracts`
- **MUST NOT depend on:**
  - `vac_shell_app`
  - `vac_session_engine`
  - `vac_tools`
  - `vil_llm`
  - Any widget crates

## Public API
The new crate will expose the following:
```rust
pub struct DoctorCommandExecutor {
    pub paths: Arc<dyn VacPaths>,
    pub activity_log: Arc<ActivityLog>,
    pub config: DoctorConfig,
}

impl vac_shell_host_commands::ShellCommandExecutor for DoctorCommandExecutor {
    // Rejects non-"/doctor" commands.
    // Runs run_doctor_checks and calls record_doctor_report.
}

pub fn record_doctor_report(activity_log: &ActivityLog, report: &DoctorReport) {
    // Maps each DoctorCheck into an ActivityLog row.
}

pub fn doctor_command_spec() -> ShellCommandSpec {
    // Returns the command spec for /doctor
}
```

## /doctor Registration
- Add `/doctor` to `vac_shell_entrypoint::default_commands()`.
- **Category:** "Diagnostics"
- **Kind:** `ShellCommandKind::Custom` (or best existing, like `PromptTemplate` if custom doesn't exist but custom is preferred).
- **Palette Visible:** `true`

## ActivityLog Row Contract
For each `DoctorCheck` in the report, an activity log entry will be created:
- **ID:** `doctor-<check.id>-<stable_suffix_or_timestamp>`
- **Title:** `"doctor: <label> — <status>"` (e.g., `"doctor: .vac paths metadata — Ok"`)
- **Detail:** `"<summary>\n<detail>"` (if detail is safe and present).
- **Severity Mapping:**
  - `DoctorCheckStatus::Ok` -> `Severity::Info` (or `Ok` if available)
  - `DoctorCheckStatus::Warning` -> `Severity::Warn`
  - `DoctorCheckStatus::Error` -> `Severity::Error`
  - `DoctorCheckStatus::Skipped` -> `Severity::Info` (skipped checks are informational).

## Redaction & Safety
- **No env var values:** Values must never be collected or printed.
- **No secret sentinels:** Ensure tests verify that secret values (e.g., dummy API keys used in tests) are absent.
- **Detail safety:** Provider names can be included in the detail, but not secret values.
- `ShellApp` remains entirely doctor-free. The invocation happens via the `ShellCommandExecutor` pipeline, and the result is pushed to `ActivityLog`, which `ShellApp` simply renders.

## Tests Required
- `doctor_command_spec` correctly defines and registers `/doctor`.
- `DoctorCommandExecutor` successfully executes `/doctor` and writes expected rows to the `ActivityLog`.
- `DoctorCommandExecutor` cleanly rejects any command slash other than `/doctor`.
- Severity mapping correctly translates `DoctorCheckStatus` to `vac_shell_contracts::Severity`.
- **Redaction:** `ActivityLog` entries must be asserted to NOT contain any secret env var values.
- `route_palette_command` with `/doctor` successfully hits the executor from the registry.
- `check-dtrack-gates.sh` passes (verifying no forbidden dependencies leaked).