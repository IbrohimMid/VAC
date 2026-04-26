# D12B Proposal: Wire Doctor Report into Operator Surface

## Goal
Wire the `vac_shell_host_doctor` diagnostic engine into the operator surface so users can easily view the readiness report within the shell cockpit or via the CLI, bridging the remaining gap in the product lifecycle.

## Preferred Route
- Start by implementing the `/doctor` palette slash through the host command pipeline (`vac_shell_host_commands` or the command adapter).
- When `/doctor` is invoked, run `vac_shell_host_doctor::run_doctor_checks` and format the `DoctorReport` into safe, operator-visible summary rows.
- Output these summary rows into the `ActivityLog` so they appear directly in the live feed.
- Defer modifying the root `vac_cli` binary unless the implementation is trivial and has a low blast radius.

## Constraints & Boundary Rules
- **Dependency Isolation:** `ShellApp` must have NO direct dependency on `vac_shell_host_doctor`. The doctor invocation must happen either via a callback or behind the existing host command pipeline (`ShellCommandExecutor`).
- **Widget Purity:** Widget crates must maintain their strict dependency on `ratatui` and `vac_shell_contracts` only.
- **No New Exceptions:** No new ADR exception is permitted unless wiring the command strictly requires pulling in forbidden dependencies (which it shouldn't, as the doctor engine is already safe).
- **Absolute Redaction:** Secret values (e.g., API keys) MUST NOT appear in the `ActivityLog`, `DoctorReport`, or any operator-facing text.

## Tests Required
- Ensure the `/doctor` slash command is properly registered and routed to the executor.
- Ensure the executor calls the doctor engine and successfully writes to the `ActivityLog`.
- Ensure the written activity entries correctly summarize the doctor report.
- Verify through automated assertions that NO secrets leak into the `ActivityLog` output.
- Verify `check-dtrack-gates.sh` passes after the wiring is complete.