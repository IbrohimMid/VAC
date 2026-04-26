# Workspace Map

> Quick orientation for new contributors and agents.
> Full boundary rules → [CRATE_BOUNDARY_MATRIX.md](CRATE_BOUNDARY_MATRIX.md)

## Crate groups

### VIL Semantic Plane
`vil_ir`, `vil_knowledge`, `vil_rag`, `vil_inference`, `vil_trust`,
`vil_validate`, `vil_context`, `vil_memory`, `vil_swarm`, `vil_llm`,
`vil_metrics`, `vil_vwfd`, `vil_expr`

Core language/semantic reasoning layer. Does not depend on VAC runtime.

### VAC Core / Runtime / Engine
`vac_core`, `vac_tools`, `vac_trace`, `vac_cli`,
`vac_session_engine`, `vac_session_primitives`, `vac_session_control`,
`vac_runtime`, `vac_approvals`, `vac_changeset`, `vac_signal`,
`vac_tool_core`, `vac_memory`, `vac_mcp_core`, `vac_bridge`,
`vac_skill`, `vac_trajectory`, `vac_ingest`, `vac_shell`

VAC execution plane: task runner, LLM adapter, tool dispatch, session
transcripts, signal buffers.

### Shell Contracts (DTO layer)
`vac_shell_contracts`

Pure data types visible to all shell layers. No logic, no engine dep.

### Shell Widget Crates
`vac_shell_palette`, `vac_shell_popup`, `vac_shell_approval_bar`,
`vac_shell_approval_detail`, `vac_shell_shortcuts`,
`vac_shell_model_switcher`, `vac_shell_session_browser`,
`vac_shell_diff_view`, `vac_shell_plan_view`, `vac_shell_activity`,
`vac_shell_status_bar`, `vac_shell_overlay`, `vac_shell_bridge`,
`vac_shell_plan`

Ratatui render + on_key only. Dep: `ratatui` + `vac_shell_contracts`.

### Shell Host State
`vac_shell_host_approval`, `vac_shell_host_sessions`,
`vac_shell_host_activity`, `vac_shell_host_model`,
`vac_shell_host_status`, `vac_shell_host_paths`,
`vac_shell_host_surface`, `vac_shell_host_diff`,
`vac_shell_host_plan`, `vac_shell_host_commands`,
`vac_shell_host_vac_config`, `vac_shell_composition`

Mutable state controllers. Dep: contracts + std. No engine dep.

### Shell App Orchestrator
`vac_shell_app`

Owns all widget view state. Wires overlays, key dispatch, approval bar,
session browser, etc. Uses injected callbacks for engine access.

### Shell Host Exceptions (ADR-sanctioned engine access)
`vac_shell_host_vac_engine_probe` (D7A, vac_core)
`vac_shell_host_vac_command_adapter` (D7B/C, vac_session_engine+vil_llm)
`vac_shell_host_vac_tool_dispatcher` (D8, vac_session_engine+vac_tools)
`vac_shell_host_transcript_projection` (D9, vac_session_engine read-only)
`vac_shell_host_event_projection` (D10, vac_session_engine+vac_tool_core)

These 5 crates are the ONLY allowed engine access points. Not reachable
from widget/app/runtime-loop graphs.

### Runtime Loop + Entrypoint
`vac_shell_keymap`, `vac_shell_runtime_loop`, `vac_shell_entrypoint`

TUI event loop, keymap translation, binary entry. Wires everything together.

## Dependency flow

```
VIL semantic ──→ VAC engine ──→ shell_host_exception crates
                                        │
                     injected callbacks │
                                        ▼
vac_shell_contracts ──→ widget crates ──→ vac_shell_app ──→ runtime_loop ──→ entrypoint
```
