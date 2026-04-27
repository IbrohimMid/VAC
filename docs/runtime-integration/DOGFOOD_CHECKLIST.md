# Dogfood Checklist — VAC ShellApp Cockpit

**Status:** opt-in. The legacy `vac_tui_runtime` remains the
default operator entrypoint until D7+ ships. Use this checklist
to dogfood the new shell stack and report findings against the
slice it broke.

## Prerequisites

* Rust toolchain pinned by `rust-toolchain.toml`.
* `cargo` reachable.
* Repo built once: `cargo check -p vac_shell_entrypoint`.

## Run

```bash
cargo run -p vac_shell_entrypoint --example dogfood
```

The cockpit boots against the current working directory's
`.vac/` state. A missing `.vac/model_config.json` is fine — the
entrypoint falls back to the fixture model
(`anthropic / claude-sonnet-4.5`) and continues.

## Manual checklist

| # | Step | Pass criteria |
|---|---|---|
| 0 | Pre-flight | Run `cargo run -p vac_shell_entrypoint --example dogfood_doctor` (or equivalent) to get a readiness report. Alternatively, type `/doctor` in the palette once the cockpit is launched. Confirm `.vac` path metadata is reported and missing directories/config are surfaced. |
| 1 | Cockpit launches | Title row + status bar visible; no panic |
| 2 | `Ctrl+P` | Palette overlay appears with `/chat`, `/runtime`, `/model`, `/sessions`, `/doctor`, `/status` |
| 3 | Type `/runtime` `Enter` | Status bar surface chip flips to `RUNTIME` |
| 4 | `Ctrl+M` | Model switcher overlay opens populated from live `ModelSelectionState` |
| 5 | Pick a model with Up/Down + Enter | Status bar `model` value updates; persistor writes `<cwd>/.vac/state/model_selection.json` |
| 6 | Quit (`q`), relaunch | Status bar `model` reflects the previously-selected model |
| 7 | `Ctrl+S` | Session browser opens listing any `.vac/sessions/*.jsonl` |
| 8 | `Ctrl+Y` | Approval detail drawer opens (placeholder content if no pending approvals) |
| 9 | ``Ctrl+\``` | Shell popup overlay opens |
| 10 | `Ctrl+L` | Plan view overlay opens (empty-state placeholder until plan file written) |
| 11 | `Esc` once | Top overlay closes |
| 12 | `Esc` repeatedly with overlays stacked | Each press pops one overlay |
| 13 | Plain `q` (no overlay) | Loop exits cleanly; terminal restored |
| 14 | Force a panic / `Ctrl+C` mid-session | Terminal raw mode + alternate screen restored automatically (`TerminalGuard` Drop) — operator's shell prompt usable without `reset` |
| 15 | Type `/memorize` `Enter` (mapped in D7B dogfood preset) | A transcript file lands at `<cwd>/.vac/sessions/<uuid>.jsonl`; first row's `metadata.source` is `shell_palette`. Confirms the D7B real engine adapter is wired through `ShellRuntimeContext`. |
| 16 | Type `/unknown` `Enter` (no adapter mapping) | Activity log records "no adapter mapping for command id `unknown`" — confirms the `Unsupported` path still surfaces operator-visible errors. |
| 17 | Run `cargo run -p vac_shell_entrypoint --example dogfood_tool_dispatch_smoke`, then call `vac_shell_host_transcript_projection::summarize_tool_use(transcript_path)` against the printed JSONL path. | Returns `ToolUseActivitySummary { total_calls: 1, ok_count: 1, .. }`; `project_tool_use_activity` yields a `ShellActivityEntry` whose `title` is `"glob ok"` and whose `detail` contains `summary: glob ok` + `duration_ms: …` + the transcript path, with NO raw payload or arguments. Confirms D9 projection is operator-safe. |
| 18 | Type `/doctor` `Enter` | ActivityLog displays diagnostic rows with "diag" label. No secrets exposed. |
| 19 | Type `/status` `Enter` | ActivityLog displays six rows with "status" label: cockpit, model, sessions, approvals, doctor, next-action. No secrets exposed. |
| 20 | Run tool smoke (e.g. `glob *` via `/memorize` if dispatcher attached) | ActivityLog displays tool result rows with "tool·ok" label. Confirms true tool execution rows remain `ToolResult`. |

## Reporting issues

Tag the report by slice:
* `D2 / D2.1` — key routing / dispatch.
* `D2.2` — loop / repaint / quit.
* `D3 / D3.1` — config snapshot or fallback warning behaviour.
* `D4 / D4.1` — event projection / activity log.
* `D5 / D5.1` — palette commands beyond the four built-ins.
* `D6` — example wiring itself.
* `D12 / D12B` — doctor diagnostics.
* `D13` — status readiness summary.
* `D14` — dedicated Diagnostic/Status activity kinds.

## Known limitations (April 2026)

* Live engine event bus is **not** connected — activity
  projections must be ingested by hosts manually
  (`vac_shell_host_event_projection::record_projected_event`).
* The dogfood example wires
  `vac_shell_host_vac_command_adapter::VacCommandExecutorAdapter`
  with `AdapterConfig::dogfood(...)`. Mapped slashes
  (`/memorize`, `/ultraplan`) submit through
  `vac_session_engine::submit_one` with the `EchoAdapter` LLM
  stub by default and write a durable transcript under
  `<cwd>/.vac/sessions/<uuid>.jsonl`.
* **D7C** — hosts can opt into a real provider with
  `AdapterConfig::with_vil_llm_router(router)` (or any
  `LlmAdapter` via `with_llm(...)`). `EchoAdapter` is still the
  default so dogfood operators do not need provider credentials
  to exercise the cockpit. Provider-level errors (auth, network,
  rate limit) surface as `ShellCommandError::Failed` and land
  in the activity log.
* **D7D** — `VilLlmRouterAdapter` translates each
  `vil_llm::ToolCall` into the engine's `ToolCallRequest`
  (`reason: None`, `estimated_tokens: 0`). The translated calls
  flow into the engine's existing dispatch loop, so tool-use is
  observable end-to-end through the `SubmitEvent` stream
  (`ToolRequested` → `ToolResult` → `Finished`).
  **Dogfood default still does not attach a real
  `ToolDispatcher`**: the engine routes each translated call
  through `UnsupportedDispatcher`, which produces a
  `ToolResult` whose envelope is `kind=error` with summary
  `"dispatch error for '<tool>'"` and message
  `"no ToolDispatcher attached — cannot run tool '<tool>'"`.
  After **D8** hosts can opt into live tool execution by
  attaching a `vac_shell_host_vac_tool_dispatcher::VacToolDispatcher`
  + `CompositeGate` via
  `AdapterConfig::with_tool_dispatcher(...)`. The default
  dogfood example keeps the unsupported path. Two opt-in
  example binaries demonstrate the live path:
  * `cargo run -p vac_shell_entrypoint --example
    dogfood_tool_dispatch` — runs the full TUI loop with the
    dispatcher attached. **Note:** the default `EchoAdapter`
    LLM does not emit tool calls, so observing live dispatch
    here also requires the operator to swap in a tool-calling
    LLM (e.g. via `AdapterConfig::with_vil_llm_router(...)`).
  * `cargo run -p vac_shell_entrypoint --example
    dogfood_tool_dispatch_smoke` — self-contained: drives one
    `submit_one` with a tool-emitting `LlmAdapter` and prints
    the transcript path plus the paired tool-use view. This
    is the canonical proof that live dispatch produces a
    `tool_result.kind = ok` envelope end-to-end without
    needing a real provider.

  Dispatcher without gate is rejected pre-flight by
  `try_with_tool_dispatcher`, so a host cannot accidentally
  turn on real dispatch with no policy/hook enforcement path
  attached.

  After **D7E** these envelopes are also persisted to the
  transcript file: every tool call emits a
  `TranscriptKind::ToolCall` row before dispatch and a
  `TranscriptKind::ToolResult` row after the envelope is
  built. The dogfood operator path (`ShellCommandExecutor::execute`,
  `submit_one(..., None)`) can now read tool-use outcomes
  straight from `<cwd>/.vac/sessions/<uuid>.jsonl` without
  subscribing to an event sink. Real tool dispatch goes live
  only when the host explicitly wires a `ToolDispatcher` +
  `CompositeGate` on `CompactConfig`.
* The legacy `vac_shell_host_commands::VacCommandExecutorAdapter`
  D5.1 stub is retained for tests and hosts that want the
  explicit "no engine wired" failure mode; the dogfood
  example no longer uses it.
* Model config snapshot at `.vac/model_config.json` is
  host-written. **D7A** ships
  `vac_shell_host_vac_engine_probe` — call
  `write_snapshot(&vac_config, &paths)` from the host (CLI
  bootstrap, `vac config probe` subcommand, or the dogfood
  example) before `build_shell_app` to keep the snapshot in
  sync with `~/.config/vac/config.toml` /
  `<project>/.vac/config.toml`. The probe never serializes API
  keys, `api_key_env` names, `base_url`, or any other
  `ProviderConfig` field — only `credentials_present: bool`
  per provider. Post-hardening public API:
  `write_snapshot(&vac_config, &paths)` /
  `write_snapshot_with_env(&vac_config, &paths, &ProcessEnvPresence)`,
  trait method `EnvPresence::present_non_empty`. **Wiring into
  a host call site is deferred** — the dogfood example
  currently still relies on the fixture fallback / a
  hand-edited snapshot until D7B lands.
* No diff/review live integration; `DiffReviewView` only renders
  what the host populates.
