# D11 Proposal: Session Browser Tool-Use Summary Surface

## Goal
Expand the Session Browser to display a detailed, host-visible tool-use summary surface for the currently selected session. While D10 introduced a high-level badge (`tools: N ok / M err`), D11 will allow operators to inspect the specific tools invoked, their statuses, and durations without needing to open the raw JSONL transcript.

## User/Operator UX Impact
- **Visibility:** Operators can review the exact tool execution history of a session directly within the CLI before deciding to resume, archive, or delete it.
- **Safety:** Summaries remain redacted (raw `payload` and `arguments` are never displayed), maintaining the security boundaries established in D9/D10.
- **UI Rendering:** The detailed summary will be rendered in the existing right preview panel when a session is selected.
  - Maximum visible tool rows (e.g., 5 to 8).
  - Displays: status indicator (e.g., `✓`, `!`, `~`, `…`), tool name, summary, and duration.
  - If no tools: "no tool calls recorded".
  - If pending: "… [tool_name] pending".
  - **No raw payload or arguments.**

## Crates Touched
- `vac_shell_contracts`: Extend session DTOs to carry the detailed tool execution list.
- `vac_shell_session_browser`: Update the widget rendering logic to include the new detail panel for the selected session.
- `vac_shell_host_sessions`: Update the state mapping to populate the new detail fields using the injected provider callback.
- `vac_shell_app`: Must be updated to provide the new callback DTOs.
  - **Constraint:** `vac_shell_app` may consume callback DTOs only. It MUST NOT depend on `vac_shell_host_transcript_projection` or `vac_session_engine`.

## DTO Shape
The following DTOs will be added/modified in `vac_shell_contracts`:

```rust
pub struct SessionToolUseDetail {
    pub call_id: String,
    pub tool_name: String,
    pub status: ToolUseUiStatus,
    pub summary: String,
    pub duration_ms: u64,
}

pub struct SessionToolUseSurface {
    pub summary: SessionToolSummary,
    pub calls: Vec<SessionToolUseDetail>,
}

pub struct SessionTileView {
    pub entry: SessionEntry,
    pub tool_summary: Option<SessionToolSummary>,
    pub tool_details: Vec<SessionToolUseDetail>,
}
```

## Provider Contract
To maintain strict dependency boundaries, `host_sessions` and `ShellApp` will not depend on D9 projection directly.
- `host_sessions` will receive a closure:
  ```rust
  pub fn list_with_tool_use(
      &self,
      paths: &dyn VacPaths,
      project: impl Fn(&Path) -> Option<SessionToolUseSurface>,
  ) -> Vec<SessionTileView>
  ```
- `ShellAppProviders` will be updated to inject the new callback:
  ```rust
  pub session_tool_use_provider: Option<Arc<dyn Fn(&Path) -> Option<SessionToolUseSurface> + Send + Sync>>
  ```
  *(This may replace the current summary-only callback).*

## Dependency Boundary
- **STRICTLY ENFORCED:** The widget crate (`vac_shell_session_browser`) will continue to depend ONLY on `ratatui` and `vac_shell_contracts`.
- No direct dependencies on `vac_session_engine`, `vac_tools`, `vil_llm`, or `vac_shell_host_transcript_projection` will be introduced in `ShellApp`, `host_sessions`, or the widget.
- The boundary gates (`scripts/check-dtrack-gates.sh`) must remain CLEAN.

## Tests Required
- **Widget Rendering:** Widget renders selected tool details in the right panel.
- **Redaction Verification:** Widget hides raw payload/arguments (sentinel check).
- **State Mapping:** `host_sessions` maps provider output into tile details correctly.
- **App Wiring:** `ShellApp` uses the provider callback, ensuring no direct projection crate dependency.
- **Boundary Gates:** `bash scripts/check-dtrack-gates.sh` must PASS.

## Docs Required
- Update `docs/runtime-integration/D-TRACK_STATUS.md` with the D11 row upon completion.
- Update `docs/runtime-integration/BLUEPRINT_CURRENT_STATE.md` to reflect the new active UX flow for the Session Browser.

## Explicit Non-Goals
- **No execution:** The session browser will not gain the ability to execute or dispatch tools. It remains a read-only management surface.
- **No raw data exposure:** We will NOT expose raw `payload` or `arguments` in this surface; the D9 redaction contract remains absolute.
- **No new ADR exceptions:** This feature will be built entirely on the existing data pipelines and callbacks established in D9 and D10.
- **No recursive redaction work:** Recursive redaction follow-up was completed previously and will not be repeated here.