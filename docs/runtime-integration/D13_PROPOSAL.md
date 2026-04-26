# D13 Proposal: Unified Status & Readiness Command

## Goal
Implement a cockpit-visible `/status` palette command that provides a concise readiness and status report in the `ActivityLog`. This command will aggregate existing read-only data (model state, sessions, approvals, doctor readiness) to answer the operator's primary question: "Is my project ready, and what should I do next?"

## User/Operator UX Flow
1. **Trigger:** Operator opens the palette (`Ctrl+P`) and types `/status`.
2. **Execution:** The host-side `StatusCommandExecutor` runs read-only projections.
3. **Feedback:** The `ActivityLog` is populated with compact status rows:
   - `status: cockpit <ready|warning|error>` (Overall status)
   - `status: model <provider/id>` (Active model or "missing")
   - `status: sessions <N>` (Count of existing sessions)
   - `status: approvals <N>` (Count of pending approvals)
   - `status: doctor <ok|warn|error>` (Summary of last doctor check)
   - `status: next action <text>` (Suggested next step for the operator)

## Crate Plan
- **New Crate:** `crates/vac_shell_host_status_command`
- **Dependency Boundary (Runtime):**
  - **Allowed:** `vac_shell_contracts`, `vac_shell_host_activity`, `vac_shell_host_commands`, `vac_shell_host_doctor`, `vac_shell_host_status`, `vac_shell_host_sessions`.
  - **Forbidden:** `vac_shell_app` (runtime), `vac_core`, `vac_session_engine`, `vac_tools`, `vil_llm`, `vac_cli`, `stakpak`, `stakai`.
- **ShellApp:** Remains an orchestrator only; it does not handle `/status` semantics.

## Public API
```rust
pub struct StatusCommandExecutor {
    pub paths: Arc<dyn VacPaths>,
    pub activity_log: Arc<ActivityLog>,
    pub doctor_config: DoctorConfig,
    pub session_state: Arc<SessionsState>,
    pub model_state: Arc<dyn VacModelConfigSnapshot>, // or similar read-only source
    pub approval_queue: Arc<ApprovalQueue>,
}

impl ShellCommandExecutor for StatusCommandExecutor {
    // Rejects non-"/status".
    // Aggregates readiness data and calls record_status_report.
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
- **Engine Isolation:** Explicitly avoids `VacEngine::status` to prevent pulling `vac_core` into the UI graph. Engine status aggregation is deferred to a future host-side exception slice.
- **No Side Effects:** Strictly read-only; no `mkdir`, no `vac init`, no configuration changes.
- **Redaction:** Must not leak secrets or raw environment variable values into `ActivityLog`.

## Validation Plan
1. **Registry Sync:** Test that entrypoint registry exactly matches `status_command_spec`.
2. **Executor Logic:** Test that `/status` correctly aggregates data and converts it to `ActivityLog` rows with proper severity.
3. **Redaction:** Verify no secret-shaped strings appear in the generated log rows.
4. **Boundary Gates:** `bash scripts/check-dtrack-gates.sh` must pass.
5. **Integration:** Verify `route_palette_command` triggers the executor and updates the log.
