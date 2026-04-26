# Crate Boundary Matrix

> Authoritative boundary table for the VAC shell cockpit.
> Run `bash scripts/check-dtrack-gates.sh` to validate.

## Layer definitions

| Layer | Description |
|---|---|
| `semantic` | VIL semantic plane (vil_*) |
| `engine` | VAC session engine, tool core, session primitives |
| `tooling` | vac_tools, vac_trace, vac_cli |
| `shell_contract` | vac_shell_contracts — pure DTO/enum |
| `shell_widget` | Ratatui widgets (render + on_key only) |
| `shell_app` | ShellApp orchestrator |
| `shell_host_state` | Host-side mutable state (approval queue, sessions, activity log) |
| `shell_host_exception` | Host-side ADR-excepted crates (engine access allowed) |
| `runtime_loop` | Shell runtime loop + keymap |
| `entrypoint` | Binary entrypoint |

## Boundary matrix

| Crate | Layer | Allowed deps | Forbidden deps | ADR exception? |
|---|---|---|---|---|
| `vac_shell_contracts` | `shell_contract` | std, serde, serde_json | everything else | No — `serde_json` added in D10.5 for recursive redaction helper (`redact_json_value`). Acceptable: no engine dep, no runtime behavior, pure JSON value transformation. All shell crates already transitively consumed serde_json; adding it as direct dep in contracts does not widen the transitive closure. |
| `vac_shell_palette` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools, host_state | No |
| `vac_shell_popup` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools, host_state | No |
| `vac_shell_approval_bar` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_approval_detail` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_shortcuts` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_model_switcher` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_session_browser` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_diff_view` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_plan_view` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_activity` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_status_bar` | `shell_widget` | ratatui, vac_shell_contracts | engine, tools | No |
| `vac_shell_overlay` | `shell_widget` | vac_shell_contracts | engine, tools | No |
| `vac_shell_bridge` | `shell_widget` | vac_shell_contracts | engine, tools | No |
| `vac_shell_composition` | `shell_app` | vac_shell_contracts, host_state crates | engine directly | No |
| `vac_shell_app` | `shell_app` | widget crates, host_state crates, vac_shell_contracts | vac_session_engine, vac_tools, vil_llm | No — uses callbacks only |
| `vac_shell_host_approval` | `shell_host_state` | vac_shell_contracts, serde_json | engine | No |
| `vac_shell_host_sessions` | `shell_host_state` | vac_shell_contracts | engine | No |
| `vac_shell_host_activity` | `shell_host_state` | vac_shell_contracts | engine | No |
| `vac_shell_host_model` | `shell_host_state` | vac_shell_contracts | engine | No |
| `vac_shell_host_status` | `shell_host_state` | vac_shell_contracts | engine | No |
| `vac_shell_host_paths` | `shell_host_state` | vac_shell_contracts | engine | No |
| `vac_shell_host_vac_engine_probe` | `shell_host_exception` | vac_core | — | **Yes — D7A** |
| `vac_shell_host_vac_command_adapter` | `shell_host_exception` | vac_session_engine, vil_llm | — | **Yes — D7B/C** |
| `vac_shell_host_vac_tool_dispatcher` | `shell_host_exception` | vac_session_engine, vac_tools | — | **Yes — D8** |
| `vac_shell_host_transcript_projection` | `shell_host_exception` | vac_session_engine (read-only) | write paths | **Yes — D9** |
| `vac_shell_host_event_projection` | `shell_host_exception` | vac_session_engine, vac_tool_core | — | **Yes — D10** |
| `vac_shell_keymap` | `runtime_loop` | crossterm, vac_shell_contracts | engine | No |
| `vac_shell_runtime_loop` | `runtime_loop` | crossterm, ratatui, vac_shell_contracts | engine | No |
| `vac_shell_entrypoint` | `entrypoint` | all above + host_exception | — | Wires exceptions |

## Rule summary

1. Widget crates never depend on engine/tooling.
2. `vac_shell_app` uses injected callbacks — no direct engine dep.
3. Exceptions are listed above and are sealed. A new exception requires ADR entry + reviewer approval.
4. `cargo tree -p <crate> -e normal --depth 4` must show no engine crates for non-exception crates.
