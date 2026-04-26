# Blueprint — Current Active State

> This is the **active** state document. It describes where VAC is _right now_.
> Historical implementation ledger → [archive/DTRACK_HISTORY.md](archive/DTRACK_HISTORY.md)
> Full status table → [D-TRACK_STATUS.md](D-TRACK_STATUS.md)

## Current phase

ShellApp dogfood cockpit integration — consolidation.

## Completed baseline

D1–D12B are sealed.

| Last sealed SHA | Description |
|---|---|
| `dfb5df5c` | D11 PASS after hardening (Session browser tool details + provider wiring) |
| `fcd55c60` | D12B closeout (docs: final seal) |
| `53120faf` | D12B PASS after final seal nits (read-only diagnostic engine + /doctor slash) |
| `5765416d` | D10.5 final consolidation accepted (net -414 LOC) |
| `b6e3b572` | D10 PASS after hardening (boundary leak, arg redaction, severity, approval preview) |
| `3ca3bb57` | Docs update: D10 ADR fifth exception + boundary tripwires |

## Current Backlog / Next Steps

- **D13 — Unified Status & Readiness Command**: next active candidate.
- **Unified init/status/logs UX**: product maturity alignment.
- **Resumability/checkpoint UX**: operator recovery flows.

## What is NOT allowed (explicit freeze)

- New runtime capability or dispatcher behavior
- New approval UI capability
- UI/widget engine dependency expansion
- `vac_session_engine` dep in `vac_shell_app`
- Raw `arguments`/`payload` in ActivityLog or approval preview
- Merging widget crates
- Rewriting `vac_cli` or `vac_tui_runtime`
- New D-track host exception without ADR entry

## Boundary rules (enforced by scripts/check-dtrack-gates.sh)

```text
vac_shell_app           → no vac_session_engine/vac_tools/vil_llm
vac_shell_session_browser → no vac_session_engine/vac_tools/vil_llm
vac_shell_runtime_loop  → no vac_session_engine/vac_tools/vil_llm
widget crates           → ratatui + vac_shell_contracts only
```

ADR-sanctioned exceptions (must not expand without review):
1. `vac_shell_host_vac_engine_probe` → `vac_core`
2. `vac_shell_host_vac_command_adapter` → `vac_session_engine` + `vil_llm`
3. `vac_shell_host_vac_tool_dispatcher` → `vac_session_engine` + `vac_tools`
4. `vac_shell_host_transcript_projection` → `vac_session_engine` (read-only)
5. `vac_shell_host_event_projection` → `vac_session_engine` + `vac_tool_core` (bridge only)

## Active UX flows (verified end-to-end)

```text
Session Browser
  → host injects session_tool_use_provider callback
  → ShellApp receives DTO surface (SessionToolUseSurface)
  → tile shows "tools: N ok / M err"
  → selecting a session renders detailed tool calls (status, name, summary, duration) in right panel
  → no raw arguments or payloads are exposed
  → app graph remains engine-free

Live Activity Feed
  → ToolRequested shows tool name only (args never forwarded)
  → ToolResult severity: Ok/Warn/Error (Warning/Cancelled → Warn)
  → bridge in vac_shell_host_event_projection (ADR exception 5)

Approval Detail
  → DefaultApprovalDetailProvider: heuristic risk from tool name
  → command preview: bounded 500 chars, top-level sensitive keys redacted
  → D11 target: recursive redaction for nested objects

Doctor / Readiness Command
  → vac_shell_host_doctor: pure read-only engine
  → vac_shell_host_doctor_command: palette slash (/doctor) wiring
  → inputs: VacPaths + DoctorConfig
  → outputs: DoctorReport DTO -> ActivityLog rows
  → checks: .vac paths, model config snapshot, credentials, dispatcher mode, boundary gates
  → never exposes secret values or mutates filesystem
  → accessible via palette / in-cockpit diagnostics
```
