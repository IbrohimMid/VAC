# D12 Proposal: VAC Dogfood Doctor / Readiness Command

## Goal
Introduce a `doctor` or `readiness` command (`vac dogfood doctor` or an equivalent palette slash `/doctor`) to verify that the local environment, configuration, and dependencies are correctly set up for the VIL-native semantic execution plane. This bridges the product maturity gap with Stakpak by providing a clear, operator-friendly readiness report before they begin working.

## User/Operator UX Impact
- **Confidence:** Operators can easily verify if their setup is healthy (e.g., API keys are present, `.vac` directory is readable) before starting a session.
- **Troubleshooting:** Provides immediate feedback on what is missing or misconfigured, reducing the "silent failure" UX when attempting to run tools or contact the LLM.
- **Safety:** The report explicitly verifies the presence of credentials *without* ever printing or leaking the actual secret values.
- **No Side Effects:** It is a read-only command. It will not mutate the `.vac` directory, configuration, or environment. No `mkdir`, no temp file writes.

## Core DTO
The following DTOs will define the report structure:

```rust
pub enum DoctorCheckStatus {
    Ok,
    Warning,
    Error,
    Skipped,
}

pub struct DoctorCheck {
    pub id: String,
    pub label: String,
    pub status: DoctorCheckStatus,
    pub summary: String,
    pub detail: Option<String>,
}

pub struct DoctorReport {
    pub overall_status: DoctorCheckStatus,
    pub checks: Vec<DoctorCheck>,
}

pub struct DoctorConfig {
    pub dispatcher_mode: String,
    // Add other relevant context here
}
```

## Approved Checks (D12 v1)
The `doctor` command should verify and report on the following (strictly read-only semantics):

1. **Paths:**
   - Verify `project_root` exists.
   - Verify `project_state_dir` exists.
   - Verify `sessions_dir` exists.
   - Verify `model_config_file` parent exists.
   - *Constraint:* No directory creation. No writing temporary files to test writability.

2. **Model Config Snapshot:**
   - Verify `model_config_file` exists.
   - Verify parsing succeeds.
   - Verify active model is valid (after `sanitize_active_model`).

3. **Provider Credentials:**
   - Use `credentials_present` from the snapshot.
   - Optionally verify standard env var presence (e.g., `ANTHROPIC_API_KEY.is_some()`).
   - *Constraint:* NEVER print or include the secret values in the report or logs.

4. **Tool Dispatcher Mode:**
   - Report the configured mode from the passed `DoctorConfig` (e.g., `inert` or `live`).
   - Defaults to `inert`.

5. **Boundary Gates:**
   - Verify that the `scripts/check-dtrack-gates.sh` script exists.
   - Verify that it is executable/readable.
   - *Constraint:* Do NOT run the script by default.

## Crate Plan
- **New Crate:** `crates/vac_shell_host_doctor`
  - **Role:** Pure read-only doctor engine.
  - **Input:** `VacPaths` + `DoctorConfig`.
  - **Output:** `DoctorReport` DTO.
  - **Constraints:** No `ShellApp` dependency, no widget dependency, no `vac_session_engine` dependency, no `vac_tools` dependency, no `vil_llm` dependency, no new ADR exception.
- **Optional Wrappers:** 
  - `vac_cli` (optional wrapper, only if low blast radius)
  - `vac_shell_host_commands` (optional `/doctor` bridge)
- **Shared DTOs:** `vac_shell_contracts` (only if shared DTO is needed by multiple layers).

## Tests Required
- **Validation Logic:** 
  - Missing sessions dir -> warning/error.
  - Corrupt model config -> error.
  - Missing credential -> warning/error without value leak.
  - Inert dispatcher mode reported correctly.
  - Boundary script missing -> warning.
- **Redaction Verification:** Strict tests asserting that secret env values NEVER appear in the `DoctorReport` Debug/string output.
- **Safety:** Test proving no filesystem mutation occurs.

## Explicit Non-Goals
- **No auto-fixing:** The doctor command will *diagnose* issues, but it will not attempt to automatically fix them.
- **No execution of side-effects:** No config mutation, no env mutation. Boundary gates script not run by default.