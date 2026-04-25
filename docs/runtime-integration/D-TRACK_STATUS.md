# D-track Status — Dogfood Runtime Integration

| Slice | Crate(s) added | Status |
|---|---|---|
| **D1** | `vac_shell_entrypoint` | PASS |
| **D2**  | `vac_shell_keymap` (mapping) | PASS |
| **D2.1**| `vac_shell_keymap` (dispatch) | PASS |
| **D2.2**| `vac_shell_runtime_loop` | PASS (after hardening: `TerminalGuard` RAII + `ShellRuntimeContext`) |
| **D3**  | `vac_shell_host_vac_config` + `VacPaths::model_config_file` | PASS |
| **D3.1**| Entrypoint config fallback | PASS (after hardening: `sanitize_active_model` drops no-creds / unknown-model active before composition) |
| **D4**  | `vac_shell_host_event_projection` | PASS |
| **D4.1**| `record_projected_event` ingest helper | PASS |
| **D5**  | `vac_shell_host_commands` (`ShellCommandExecutor` + `route_palette_command`) | PASS (after hardening: actually wired into `handle_key_event_once` via `ShellRuntimeContext`) |
| **D5.1**| `VacCommandExecutorAdapter` stub | PASS |
| **D6**  | `cargo run -p vac_shell_entrypoint --example dogfood` | PASS — example now constructs `ShellRuntimeContext` with the adapter stub (manual checklist) |
| **D7A** | `vac_shell_host_vac_engine_probe` — host-side `VacConfig` → `.vac/model_config.json` projection (first ADR-sanctioned `vac_core` exception, allowlist-shaped, denylist-swept) | PASS after hardening — `EnvPresence::present_non_empty` + `ProcessEnvPresence`, `api_key_env=None` ⇒ ready (parity with `vil_llm::LlmConfig::provider_ready`), parent-dir fsync on Unix + Windows replace fallback |
| **RC gate** | This doc + `DOGFOOD_CHECKLIST.md` + map update | PASS (post-hardening) |

## Crate inventory after the batch

```
crates/vac_shell_entrypoint           D1 + D3.1 + D6 (example)
crates/vac_shell_keymap               D2 + D2.1
crates/vac_shell_runtime_loop         D2.2
crates/vac_shell_host_vac_config      D3
crates/vac_shell_host_event_projection D4 + D4.1
crates/vac_shell_host_commands        D5 + D5.1
crates/vac_shell_host_vac_engine_probe D7A (host-side, vac_core exception)
```

Plus the cockpit layer landed before the D-track:

```
crates/vac_shell_contracts            DTO + path/registry/overlay
crates/vac_shell_bridge               ShellAction / ShellHost / controllers
crates/vac_shell_app                  ShellApp orchestrator + apply_event
crates/vac_shell_palette              palette widget
crates/vac_shell_popup                shell-popup widget
crates/vac_shell_approval_bar         approval-bar widget
crates/vac_shell_approval_detail      approval-detail widget
crates/vac_shell_shortcuts            shortcuts/sessions/commands popup
crates/vac_shell_model_switcher       model-switcher widget
crates/vac_shell_session_browser      session-browser widget
crates/vac_shell_diff_view            diff-review widget
crates/vac_shell_plan_view            plan-view widget
crates/vac_shell_activity             activity-stream widget
crates/vac_shell_status_bar           status-bar widget
crates/vac_shell_overlay              OverlayStack
crates/vac_shell_composition          ShellComposition + builder
crates/vac_shell_host_paths           VacPaths impl
crates/vac_shell_host_model           ModelSource + selection state
crates/vac_shell_host_approval        ApprovalQueue + controller
crates/vac_shell_host_surface         SurfaceState + controller
crates/vac_shell_host_sessions        SessionsState (resume/archive/delete)
crates/vac_shell_host_status          status projection
crates/vac_shell_host_activity        ActivityLog
crates/vac_shell_plan                 plan parser (DTOs in contracts)
crates/vac_shell_host_plan            plan reader
crates/vac_shell_diff_view…
crates/vac_shell_host_diff            simple line-diff projector
```

## Boundary invariants (verified by `cargo tree -e normal --depth 1`)

* **UI widget crates** stay on `ratatui + vac_shell_contracts`
  (or narrower).
* **Bridge** stays on `vac_shell_contracts` only.
* **D-track crates** add `crossterm` (for `vac_shell_keymap` /
  `vac_shell_runtime_loop`) and `ratatui` (for the loop's
  `Terminal`) — explicitly allowed by the ADR.
* No `vac_core`, `vac_session_engine`, `vac_tui_runtime`,
  `stakai`, donor crates, `SecretManager`, `AutoApproveManager`,
  or `.stakpak` path composition appears anywhere in the new
  shell stack — **with one named exception**:
  `vac_shell_host_vac_engine_probe` (D7A) is allowed to depend
  on `vac_core` because it is a host-side *producer crate* that
  writes the read-only `.vac/model_config.json` snapshot. It is
  not reachable from any UI / widget / bridge / app /
  entrypoint runtime graph; see the ADR appendix.

## What is still deferred

* `VacCommandExecutorAdapter` real impl — bridge into
  `vac_session_engine` / `vac_cli::commands` for non-built-in
  palette slashes.
* Live event bus → `record_projected_event` adapter — hosts
  currently call the ingest helper manually.
* Diff/review live integration.
* Approval detail content from a real `ApprovalDetailProvider`
  (today the drawer synthesises a placeholder from the compact
  bar row).
* Replacement of `vac_tui_runtime` — pending operator
  validation against the dogfood checklist.

## How to run the dogfood loop

See `docs/runtime-integration/DOGFOOD_CHECKLIST.md`.
