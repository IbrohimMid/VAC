# D12 Proposal: VAC Dogfood Doctor / Readiness Command

## Goal
Introduce a `doctor` or `readiness` command (`vac dogfood doctor` or an equivalent palette slash `/doctor`) to verify that the local environment, configuration, and dependencies are correctly set up for the VIL-native semantic execution plane. This bridges the product maturity gap with Stakpak by providing a clear, operator-friendly readiness report before they begin working.

## User/Operator UX Impact
- **Confidence:** Operators can easily verify if their setup is healthy (e.g., API keys are present, `.vac` directory is writable, model config is correct) before starting a session.
- **Troubleshooting:** Provides immediate feedback on what is missing or misconfigured, reducing the "silent failure" UX when attempting to run tools or contact the LLM.
- **Safety:** The report must explicitly verify the presence of credentials *without* ever printing or leaking the actual secret values.

## Capabilities to Check
The `doctor` command should verify and report on the following:
1. **Paths:** Ensure the `.vac` directory structure (sessions, memory, transcripts) exists and is writable.
2. **Model Config Snapshot:** Check if `.vac/model_config.json` exists and is readable (produced by D7A).
3. **Provider Credentials:** Verify that the environment variables required by the active model configuration are present (e.g., `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`). **Must not print the secret values.**
4. **Tool Dispatcher Mode:** Report whether the runtime is configured for `inert` (safe/unsupported) or `live` (real tool execution) tool dispatch.
5. **Boundary Gates:** Confirm that the D-track boundary gate scripts (`scripts/check-dtrack-gates.sh`) are executable and pass (optional but highly recommended for the dogfood build).

## Crates Touched
- `vac_cli` or `vac_shell_host_commands`: Depending on whether this is exposed as a root CLI command (`vac dogfood doctor`) or an in-cockpit palette command (`/doctor`). A dual approach is best: implement the core logic in a reusable host or core crate, and wire it to both entrypoints.
- `vac_shell_host_vac_config`: To verify model configuration and credential presence.
- `vac_shell_host_paths`: To verify directory writability.

## Dependency Boundary
- The core doctor logic should be placed in a crate that is allowed to see the host configuration and paths. If wired as a `/doctor` command inside the cockpit, it must not violate the existing ShellApp or widget boundaries.
- No new ADR exceptions should be required if the logic relies on existing `VacPaths` and `VacConfig` abstractions.

## Tests Required
- **Validation Logic:** Tests to ensure each check correctly identifies success and failure states (e.g., missing directory, missing env var).
- **Redaction Verification:** Strict tests asserting that credential values are NEVER included in the generated report output.
- **Command Routing:** If implemented as `/doctor`, verify that the palette correctly routes the command and displays the report in the activity log or a dedicated overlay.

## Docs Required
- Update `docs/runtime-integration/D-TRACK_STATUS.md` with the D12 row upon completion.
- Add an entry to the `DOGFOOD_CHECKLIST.md` instructing operators to run the doctor command as the first step.
- Document the command in the CLI reference.

## Explicit Non-Goals
- **No auto-fixing:** The doctor command will *diagnose* issues, but it will not attempt to automatically fix them (e.g., it will not prompt the user to input a missing API key).
- **No execution of side-effects:** It is a read-only command. It must not mutate the `.vac` directory, configuration, or environment.