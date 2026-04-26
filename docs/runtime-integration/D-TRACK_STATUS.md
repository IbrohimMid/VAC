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
| **D6**  | `cargo run -p vac_shell_entrypoint --example dogfood` | PASS — post-D7B the example wires the real `vac_shell_host_vac_command_adapter::VacCommandExecutorAdapter` (engine-backed via EchoAdapter) instead of the D5.1 stub; entrypoint command registry includes `/memorize` + `/ultraplan` so the dogfood preset reaches the adapter end-to-end (manual checklist) |
| **D7A** | `vac_shell_host_vac_engine_probe` — host-side `VacConfig` → `.vac/model_config.json` projection (first ADR-sanctioned `vac_core` exception, allowlist-shaped, denylist-swept) | PASS after hardening — `EnvPresence::present_non_empty` + `ProcessEnvPresence`, `api_key_env=None` ⇒ ready (parity with `vil_llm::LlmConfig::provider_ready`), parent-dir fsync on Unix + Windows replace fallback |
| **D7B** | `vac_shell_host_vac_command_adapter` — host-side `ShellCommandExecutor` impl that bridges custom palette slashes to `vac_session_engine::submit_one` (second ADR-sanctioned exception). Sync trait preserved via `block_in_place` / current-thread fallback. EchoAdapter LLM stub for v1; transcript durability proven. | PASS after hardening — entrypoint `default_commands()` registers `/memorize` + `/ultraplan` so the registry stays in sync with `AdapterConfig::dogfood`; `dogfood_entrypoint_registry_and_adapter_are_synchronized` end-to-end test pins it |
| **D7C** | Same crate, real provider routing. `AdapterLlm::{Echo, Custom}` enum, `AdapterConfig::with_llm` + `with_vil_llm_router` helpers, `VilLlmRouterAdapter` bridge wrapping `vil_llm::LlmRouter` as `vac_session_engine::LlmAdapter`. Echo stays default. Custom-adapter failures surface as `ShellCommandError::Failed`. | PASS after hardening — bridge now uses `LlmRouter::complete_with_provider` so the actual provider that satisfied the request lands in the engine response and transcript, even under fallback; pinned by fake-provider unit tests in `vil_llm` and integration tests in the adapter crate |
| **D7D** | Same crate, tool-use round-tripping. `VilLlmRouterAdapter` translates each `vil_llm::ToolCall` into `vac_session_engine::ToolCallRequest` (`reason: None`, `estimated_tokens: 0`). Tool dispatch stays inert when the host has not attached a `ToolDispatcher` — the engine's `UnsupportedDispatcher` writes an error envelope without aborting the submit. | PASS after hardening — `submit_one` event-channel tests prove the engine emits `ToolRequested` → `ToolResult(error: "no ToolDispatcher attached…")` → `Finished` in order, including under provider fallback and for multi-tool batches |
| **D7E** | `vac_session_engine::submit.rs` now appends `TranscriptKind::ToolCall` before dispatch and `TranscriptKind::ToolResult` after the envelope is built, so tool-use is observable from the transcript file alone — no event subscription required. Operator audit trail is honest end-to-end: unsupported-dispatcher path, gate-deny path, and dispatcher-success path all leave durable rows. | PASS after review (SHA `84fc3688cb51e7eb23afbbb7337d3af2a2bbc930`) |
| **D8** | `vac_shell_host_vac_tool_dispatcher` (third ADR-sanctioned host-side exception) ships `VacToolDispatcher` over `vac_tools::ToolRegistry`. Adapter gains `with_tool_dispatcher(dispatcher, gate)` + `try_with_tool_dispatcher(...)` (pre-flight rejects dispatcher without `CompositeGate`). Default stays `UnsupportedDispatcher`; opt-in dogfood example `dogfood_tool_dispatch` wires a real registry. D8E `read_tool_use_rows` reads paired `tool_call`/`tool_result` views from the transcript. | PASS after hardening — dispatcher now goes through `ToolRegistry::execute` (inheriting result-spill semantics); panic-catch claims removed from docs; new `dogfood_tool_dispatch_smoke` example + integration test demonstrably produce a `tool_result.kind=ok` envelope from a real `GlobTool` call |
| **D9** | `vac_shell_host_transcript_projection` (fourth ADR-sanctioned host-side exception) projects D7E/D8 `tool_call`/`tool_result` rows into operator-safe `ShellActivityEntry` and a `ToolUseActivitySummary` for the session browser tile. Read-only — never mutates the transcript. Severity map: `Ok→Info/Ok`, `Warning→Warn`, `Error→Error`, `Cancelled→Warn`, missing result→`Pending`/Warn. Operator redaction: only tool name / status / envelope summary / duration / transcript path land in activity rows; raw `payload` and `arguments` stay in the JSONL file on disk. | PASS after review (SHA `2ce754dfe8809bccf2fccf9a20df0682cc233832`) |
| **D10** | Session browser tile badge (`SessionTileView` / `SessionToolSummary` in contracts); live activity feed bridge (`spawn_activity_feed_bridge` in `vac_shell_host_event_projection`, fifth ADR-sanctioned exception); approval detail enrichment (`DefaultApprovalDetailProvider` heuristic in `vac_shell_host_approval`). Hardening commit (`b6e3b572`): boundary leak fixed — `ShellApp` injects `session_tool_summary_provider` callback, no direct dep on D9 projection; live feed redacts raw args (args never reach ActivityLog); Warning/Cancelled ToolResult → Severity::Warn; approval preview bounded (500 chars) + sensitive-key redaction. | PASS after hardening (SHA `b6e3b5720b94a16d4ccd3a74dc7b67ffd784c43a`) |
| **D10.5** | Final consolidation — net -414 LOC, `vac_shell_test_support` adopted, boundary gates clean. | PASS after final consolidation (SHA `5765416d606ad7ed82a10c7cdf45a408dc8d95ff`) |
| **D11** | Session Browser Tool-Use Summary Surface. Extends right-hand preview panel to display tool call status, duration, and summary. No raw payload/arguments exposed. Preserves strict boundaries (widget uses only ratatui + vac_shell_contracts). | PASS after hardening (SHA `dfb5df5c91827f938a79651a2b9bb84aac94df76`) |
| **D12** | VAC Dogfood Doctor / Readiness Command. A read-only diagnostic engine (`vac_shell_host_doctor`) to verify model config, path accessibility, credential presence, and dispatcher mode without leaking secrets or mutating state. | PASS after hardening (core engine only, `/doctor` wiring deferred to D12B) |
| **D12B** | Wire Doctor Report into Operator Surface. Implementation of the `/doctor` palette slash via `vac_shell_host_doctor_command`. Registry updated to include `/doctor` in `vac_shell_entrypoint`. Safe report rows recorded into `ActivityLog`. | PASS after final seal nits (SHA `53120fafd329dce174b88abf2a592bf13dc3d1a2`) |
| **D13** | Unified Status & Readiness Command. A cockpit-visible `/status` command that aggregates model, session, and approval state with doctor readiness summaries via a host-injected provider. | PASS (pending review) |
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
crates/vac_shell_host_vac_command_adapter D7B + D7C (host-side, vac_session_engine + vil_llm exceptions)
crates/vac_shell_host_vac_tool_dispatcher D8 (host-side, vac_session_engine + vac_tools exception)
crates/vac_shell_host_transcript_projection D9 (host-side, vac_session_engine read-only exception)
crates/vac_shell_host_event_projection     D10 (fifth ADR exception: SubmitStream bridge; not reachable from UI/app graph)
crates/vac_shell_host_approval             D10 (heuristic risk classification; contracts-only, no engine dep)
crates/vac_shell_session_browser           D10/D11 (SessionTileView badge rendering; contracts-only)
crates/vac_shell_host_doctor               D12 (read-only diagnostic engine; no app/widget/engine dep)
crates/vac_shell_host_doctor_command       D12B (palette slash bridge; records to ActivityLog)
crates/vac_shell_host_status_command       D13 (/status palette readiness bridge; provider-injected; no app/engine dep)
```

## Activity Log semantics for D12B

`DoctorReport` rows currently reuse `ShellActivityKind::ToolResult` as a temporary operator activity kind until a dedicated `Diagnostic` kind is introduced.

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
  shell stack — **with four named exceptions**:
  * `vac_shell_host_vac_engine_probe` (D7A) is allowed to
    depend on `vac_core` because it is a host-side *producer
    crate* that writes the read-only
    `.vac/model_config.json` snapshot.
  * `vac_shell_host_vac_command_adapter` (D7B + D7C) is
    allowed to depend on `vac_session_engine` and `vil_llm`
    because it is a host-side *executor crate* that submits
    custom palette slashes through `submit_one` and (D7C)
    optionally routes those submits through a real provider
    via `vil_llm::LlmRouter`.
  * `vac_shell_host_vac_tool_dispatcher` (D8) is allowed to
    depend on `vac_session_engine` and `vac_tools` because it
    is the host-side bridge from `ToolDispatcher` into the
    existing tool registry. Hosts opt in via the adapter's
    `with_tool_dispatcher(dispatcher, gate)`; default stays
    inert.
  * `vac_shell_host_transcript_projection` (D9) is allowed to
    depend on `vac_session_engine` (`read_tool_use_rows` +
    `ToolUseTranscriptView`) because it is a read-only
    host-side projection from the transcript JSONL into
    `ShellActivityEntry` rows + a `ToolUseActivitySummary`. It
    never mutates the transcript and never renders raw
    `payload` / `arguments` into the projected text.
  * `vac_shell_host_event_projection` (D10) is allowed to
    depend on `vac_session_engine` (`SubmitStream` /
    `SubmitChunk`) and `vac_tool_core` (`ToolResultEnvelope`)
    for `spawn_activity_feed_bridge`. The bridge maps live
    stream chunks into `ShellActivityEntry` rows with strict
    redaction: raw `arguments` are never forwarded (always
    `None`); `ToolResultKind` maps faithfully to `Severity`
    (Warning/Cancelled → Warn). `ShellApp` does NOT depend on
    this crate; it is wired by the host entrypoint only.
    `ShellApp` receives D9 summaries via an injected
    `session_tool_summary_provider` callback — never via a
    direct dep on any projection crate.

  Neither crate is reachable from any UI / widget / bridge /
  app / entrypoint / runtime-loop runtime graph; see the ADR
  appendices.

## Active navigation

- **Current active state** → [BLUEPRINT_CURRENT_STATE.md](BLUEPRINT_CURRENT_STATE.md)
- **Crate boundary matrix** → [CRATE_BOUNDARY_MATRIX.md](CRATE_BOUNDARY_MATRIX.md)
- **Consolidation backlog** → [CONSOLIDATION_BACKLOG.md](CONSOLIDATION_BACKLOG.md)
- **Review checklist** → [DTRACK_REVIEW_CHECKLIST.md](DTRACK_REVIEW_CHECKLIST.md)
- **Workspace map** → [WORKSPACE_MAP.md](WORKSPACE_MAP.md)
- **LOC audit** → [LOC_AUDIT.md](LOC_AUDIT.md)
- **Historical ledger** → [archive/DTRACK_HISTORY.md](archive/DTRACK_HISTORY.md)

## What is still deferred

* `vac_cli`-grade dispatch reuse — the D7B adapter goes
  directly to `vac_session_engine::submit_one`. If `vac_cli`
  exposes a reusable library entry point in a future slice,
  the adapter can switch to it without changing call sites.
* Live event bus → `record_projected_event` adapter — hosts
  currently call the ingest helper manually.
* Diff/review live integration.
* Replacement of `vac_tui_runtime` — pending operator
  validation against the dogfood checklist.
* **D11 candidate — recursive secret redaction**: current
  `redact_and_preview` in `DefaultApprovalDetailProvider` only
  redacts top-level object keys. Nested secrets like
  `{ "config": { "token": "..." } }` are not yet redacted.
  D11 should add a recursive redaction helper (reusable across
  approval + bridge), with tests for: nested secret, array of
  objects, mixed safe/secret fields, non-object root.

## Boundary tripwire checks

Run these after any change to `vac_shell_app`,
`vac_shell_session_browser`, or `vac_shell_runtime_loop` to
confirm no engine crates have crept back in:

```bash
# vac_shell_app must not reach engine/tools/LLM crates
cargo tree -p vac_shell_app -e normal --depth 4 \
  | grep -E 'vac_session_engine|vac_tools|vil_llm|vac_cli|stakpak|stakai' \
  || echo "CLEAN"

# vac_shell_session_browser must not reach engine/tools/LLM crates
cargo tree -p vac_shell_session_browser -e normal --depth 3 \
  | grep -E 'vac_session_engine|vac_tools|vil_llm|vac_cli|stakpak|stakai' \
  || echo "CLEAN"

# vac_shell_runtime_loop must not reach engine/tools/LLM crates
cargo tree -p vac_shell_runtime_loop -e normal --depth 4 \
  | grep -E 'vac_session_engine|vac_tools|vil_llm|vac_cli|stakpak|stakai' \
  || echo "CLEAN"
```

All three must print `CLEAN`. Any match is a boundary violation
that blocks the next D-slice review.

## How to run the dogfood loop

See `docs/runtime-integration/DOGFOOD_CHECKLIST.md`.
