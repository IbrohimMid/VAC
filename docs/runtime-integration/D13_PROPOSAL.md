# D13 Proposal: Unified Status & Readiness Command

## Goal
Implement a cockpit-visible `/status` palette command that provides a concise readiness and status report in the `ActivityLog`. This command will aggregate data from a host-provided `StatusReportProvider` to answer the operator's primary question: "Is my project ready, and what should I do next?"

## User/Operator UX Flow
1. **Trigger:** Operator opens the palette (`Ctrl+P`) and types `/status`.
2. **Execution:** The host-side `StatusCommandExecutor` calls the injected `StatusReportProvider`.
3. **Feedback:** The `ActivityLog` is populated with compact status rows:
   - `status: cockpit <ready|warning|error>` (Overall status)
   - `status: model <label>` (Active model or "missing")
   - `status: sessions <N>` (Count of existing sessions)
   - `status: approvals <N>` (Count of pending approvals)
   - `status: doctor <ok|warn|error>` (Summary of last doctor check)
   - `status: next action <text>` (Suggested next step for the operator)

## Crate Plan
- **New Crate:** `crates/vac_shell_host_status_command`
- **Dependency Boundary (Runtime):**
  - **Allowed:** `vac_shell_contracts`, `vac_shell_host_activity`, `vac_shell_host_commands`, `vac_shell_host_doctor`.
  - **Optional (if needed):** `vac_shell_host_status`, `vac_shell_host_sessions`.
  - **Forbidden:** `vac_shell_app` (runtime), `vac_shell_host_approval`, `vac_shell_host_vac_config`, `vac_shell_composition`, `vac_core`, `vac_session_engine`, `vac_tools`, `vil_llm`, `vac_cli`, `stakpak`, `stakai`.
- **ShellApp:** Remains an orchestrator only; it does not handle `/status` semantics.

## Public API
```rust
pub struct StatusReport {
    pub cockpit_status: DoctorCheckStatus, // Reusing status enum from vac_shell_host_doctor
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
    // Rejects non-"/status" as ShellCommandError::Unsupported.
    // Calls report_provider.report() and then record_status_report.
}

pub fn status_command_spec() -> ShellCommandSpec;
pub fn record_status_report(activity_log: &ActivityLog, report: &StatusReport, ts_unix: u64);
```

## /status Registration
- **Location:** `vac_shell_entrypoint::default_commands()`.
- **Metadata:**
  - `id`: "status"
  - `slash`: "/status"
  - `title`: "Status"
  - `category`: "Diagnostics"
  - `kind`: `ShellCommandKind::PromptTemplate` (matches `/doctor` pattern)
  - `palette_visible`: `true`
  - `description`: "Show cockpit status and readiness summary"

## Known Risks & Constraints
- **Engine Isolation:** Strictly avoids direct dependencies on engine/host-state crates by using the `StatusReportProvider` trait. Implementation of the provider happens host-side (e.g. in entrypoint example).
- **No Side Effects:** Strictly read-only; no `mkdir`, no `vac init`, no configuration changes.
- **Redaction:** Secret values (e.g., API keys) MUST NOT appear in the `ActivityLog`, `StatusReport`, or any operator-facing text.

## Validation Plan (Required before PASS)
1. **Registry Sync:** Test that entrypoint registry exactly matches `status_command_spec` for all fields: `id`, `slash`, `title`, `description`, `category`, `palette_visible`, `kind`.
2. **Executor Logic:** 
   - Verify `/status` triggers the executor via `route_palette_command`.
   - Verify non-`/status` commands are rejected as `Unsupported`.
   - Verify `record_status_report` writes exact row titles and severities.
3. **Severity Mapping:** Synthetic test asserting:
   - `Ok -> Severity::Ok`
   - `Warning -> Severity::Warn`
   - `Error -> Severity::Error`
   - `Skipped -> Severity::Warn`
4. **CompositeExecutor:** Verify fallback delegation works when paired with other executors.
5. **Redaction:** Verify no secret-shaped strings appear in the generated `ActivityLog` debug dump.
6. **Boundary Gates:** `bash scripts/check-dtrack-gates.sh` must pass.
