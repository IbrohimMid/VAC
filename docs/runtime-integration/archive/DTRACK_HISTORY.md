# D-track Historical Implementation Ledger

> This file archives detailed planning notes and historical context
> for completed D-slices. Active state → [../BLUEPRINT_CURRENT_STATE.md](../BLUEPRINT_CURRENT_STATE.md)
> Current status table → [../D-TRACK_STATUS.md](../D-TRACK_STATUS.md)

## D1–D6: Foundation slices

D1: `vac_shell_entrypoint` — bootstraps the TUI binary.
D2/D2.1/D2.2: `vac_shell_keymap`, `vac_shell_runtime_loop` — crossterm event loop + RAII terminal guard.
D3/D3.1: `vac_shell_host_vac_config` — config → `.vac/model_config.json` projection.
D4/D4.1: `vac_shell_host_event_projection` — RuntimeEventView DTO + ingest helper.
D5/D5.1: `vac_shell_host_commands` + `VacCommandExecutorAdapter` stub.
D6: dogfood example — real adapter replacing stub.

## D7A–D7E: LLM + tool-use + transcript

D7A: `vac_shell_host_vac_engine_probe` — first ADR exception. Writes `.vac/model_config.json`.
D7B/C: `vac_shell_host_vac_command_adapter` — real LLM routing via `vil_llm::LlmRouter`.
D7D: Tool-use round-tripping. `ToolDispatcher` inert by default.
D7E: Transcript durability — `tool_call`/`tool_result` JSONL rows written before+after dispatch.

## D8: Tool dispatcher

`vac_shell_host_vac_tool_dispatcher` — third exception. `VacToolDispatcher` over `ToolRegistry`.
`with_tool_dispatcher(dispatcher, gate)` requires `CompositeGate` (pre-flight check).
Dogfood example `dogfood_tool_dispatch_smoke` proves `GlobTool` returns `tool_result.kind=ok`.

## D9: Transcript projection (read-only)

`vac_shell_host_transcript_projection` — fourth exception. Read-only JSONL → `ShellActivityEntry`.
Severity map: Ok→Ok, Warning→Warn, Error→Error, Cancelled→Warn, missing→Pending/Warn.
Redaction discipline: only name/status/summary/duration in projected rows.

## D10: Session intelligence + live feed + approval enrichment

Three independent areas:
1. Session browser tile badge (SessionTileView/SessionToolSummary in contracts).
2. Live activity feed bridge (spawn_activity_feed_bridge, ADR exception 5).
3. Approval detail enrichment (DefaultApprovalDetailProvider heuristic).

D10-HARDENING (b6e3b572):
- Boundary leak fixed: ShellApp uses injected session_tool_summary_provider callback.
- Raw args redacted from ActivityLog.
- Warning/Cancelled → Severity::Warn (not Error).
- Approval preview bounded 500 chars + top-level key redaction.
