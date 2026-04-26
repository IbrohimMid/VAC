# D11 Proposal: Session Browser Tool-Use Summary Surface

## Goal
Expand the Session Browser to display a detailed, host-visible tool-use summary surface for the currently selected session. While D10 introduced a high-level badge (`tools: N ok / M err`), D11 will allow operators to inspect the specific tools invoked, their statuses, and durations without needing to open the raw JSONL transcript.

## User/Operator UX Impact
- **Visibility:** Operators can review the exact tool execution history of a session directly within the CLI before deciding to resume, archive, or delete it.
- **Safety:** Summaries remain redacted (raw `payload` and `arguments` are never displayed), maintaining the security boundaries established in D9/D10.

## Crates Touched
- `vac_shell_contracts`: Extend session DTOs (e.g., `SessionTileView` or an associated detail view) to carry the detailed tool execution list.
- `vac_shell_session_browser`: Update the widget rendering logic to include a new detail panel or expanded view for the selected session's tool-use summary.
- `vac_shell_host_sessions`: Update the state mapping to populate the new detail fields using the existing injected provider callbacks.

## Dependency Boundary
- **STRICTLY ENFORCED:** The widget crate (`vac_shell_session_browser`) will continue to depend ONLY on `ratatui` and `vac_shell_contracts`.
- No dependencies on `vac_session_engine`, `vac_tools`, or `vil_llm` will be introduced.
- The boundary gates (`scripts/check-dtrack-gates.sh`) must remain CLEAN.

## Tests Required
- **Widget Rendering:** Tests to ensure the new summary detail panel renders correctly when a session with tool usage is selected.
- **Redaction Verification:** Tests to assert that raw `payload` and `arguments` are never rendered in the expanded view.
- **State Mapping:** Tests in `vac_shell_host_sessions` to verify that projection data is correctly mapped into the new contracts without leaking engine types.

## Docs Required
- Update `docs/runtime-integration/D-TRACK_STATUS.md` with the D11 row upon completion.
- Update `docs/runtime-integration/BLUEPRINT_CURRENT_STATE.md` to reflect the new active UX flow for the Session Browser.

## Explicit Non-Goals
- **No execution:** The session browser will not gain the ability to execute or dispatch tools. It remains a read-only management surface.
- **No raw data exposure:** We will NOT expose raw `payload` or `arguments` in this surface; the D9 redaction contract remains absolute.
- **No new ADR exceptions:** This feature will be built entirely on the existing data pipelines and callbacks established in D9 and D10.
- **No recursive redaction work:** Recursive redaction follow-up was completed previously and will not be repeated here.