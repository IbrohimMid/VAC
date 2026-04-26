# ADR — ShellApp Dogfood Runtime Integration Strategy

**Status:** Accepted (D1, 2026-04-25)
**Track:** D-track (dogfood runtime integration)

## Context

Slices 8 — 20.3 landed a 26-crate **shell stack** (the "cockpit
layer"): pure UI widget crates on `ratatui + vac_shell_contracts`,
host crates that own state behind narrow trait/DTO seams, a
`ShellApp` orchestrator that wires composition + overlays + status
+ approvals + sessions + plan + diff + activity + production error
reporting. RC gate is closed: 239/239 tests, denylist sweep clean,
boundary matrix verified.

The legacy `vac_tui_runtime` crate is still the *real* operator
TUI. It is large, has known UX debt (the work that motivated the
shell-transplant project in the first place), and is still where
operators actually run today.

The next track is **dogfood runtime integration**: getting the new
shell stack in front of a real operator without ripping the legacy
TUI out from under in-progress work.

## Decision

Run the new `ShellApp` cockpit **side-by-side with the legacy
`vac_tui_runtime`, behind an opt-in entrypoint**, until each
runtime-integration slice (D2 — D6) lands and is reviewed.

Concretely:

- A new thin crate, `crates/vac_shell_entrypoint/`, exposes a
  single public function:

  ```rust
  pub fn run_shell_app(project_root: impl AsRef<Path>) -> ExitCode
  ```

- The function instantiates `ShellComposition` + `ShellApp` from
  the supplied project root, attaches `ActivityLog` and
  `SessionsState`, calls `prepare_frame()` once, and returns
  `ExitCode::SUCCESS`. **No interactive crossterm loop yet** —
  the loop is a D2 / D2.1 concern.
- A future CLI flag or subcommand (settled in D6) will route into
  `run_shell_app`. Until then the entrypoint is exercised only
  through tests and is not reachable from `cargo run`.

The legacy runtime stays the default. Operators do not see the new
shell until D6 lands.

## Rejected alternatives

### Replace `vac_tui_runtime` outright

Rejected. The old runtime still has features (rich approvals UI,
shell PTY popup runtime, autopilot panel, plan mode, telemetry hooks)
that the new shell stack only models at the surface level. Cutting
over before the integration adapters land would regress live
operator workflow.

### Fork `vac_tui_runtime` and merge changes piecemeal

Rejected. The whole reason the shell stack exists is to escape the
debt accumulated in the legacy TUI. Forking would re-import that
debt into the new code path.

### Wire `ShellApp` directly into `vac_core` / `vac_session_engine`

Rejected for D1. The shell stack has held a hard "no engine
coupling" line through 26 crates and 239 tests. Crossing that line
in the same slice that introduces the entrypoint would be a large
unreviewable change. Engine coupling is a D3 / D4 / D5 concern,
and each slice gets its own scope, ADR notes, and review gate.

## Boundary

`vac_shell_entrypoint` may depend on:

- `vac_shell_app`
- `vac_shell_composition`
- `vac_shell_host_paths`
- `vac_shell_host_model`
- `vac_shell_host_activity`
- `vac_shell_host_sessions`
- `vac_shell_contracts`

It must **not** depend on:

- `vac_core`
- `vac_session_engine`
- `vac_tui_runtime`
- `stakai`
- `vendor/stakpak`
- `SecretManager`
- `AutoApproveManager`
- `.stakpak` paths (path composition stays inside `VacPaths`)

These exclusions are enforced at the `Cargo.toml` level and
verified by a denylist sweep at each RC gate.

## Rollback path

If D2 — D6 fail to deliver a credible operator experience inside
the calendar window, the entrypoint can be deleted in one PR:

1. Remove `crates/vac_shell_entrypoint/` from the workspace.
2. Remove the future CLI flag / subcommand (when added).
3. Operators continue using `vac_tui_runtime` unchanged.

No state migration is required because the entrypoint never
mutates engine state in this slice. The shell stack persists
operator-side selection state to `<project>/.vac/state/...`, but
that file is read-only on rollback and survives.

## Subsequent slices

| Slice | Goal |
|---|---|
| **D2** (`vac_shell_keymap`) | Pure crossterm-event → `RoutedKey` mapping. No I/O, no state. |
| **D2.1** (dispatch adapter) | `RoutedKey` → the right `ShellApp::dispatch_*_key` method, returning `Result<Option<AppEvent>, AppError>`. |
| **D3** | Read-only VAC config model source — first crossing into engine territory; behind a `ModelSource` impl that exposes `credentials_present: bool` only, never a key or token. |
| **D4** | Activity projection from the live VAC event bus into `ActivityLog`, behind an `ActivityProjection` trait. |
| **D5** | Real palette command dispatch bridge — slashes that aren't built-in flow into existing `vac_session_engine` / `vac_cli::commands` paths. |
| **D6** | Dogfood smoke test path + operator checklist + decision on flag/subcommand. |

Each slice is implemented in isolation, reviewed against this
ADR, and accepted only after its tests + boundary matrix + map
update pass.

## Acceptance for D1

- This ADR exists.
- `vac_shell_entrypoint` exists, compiles, and tests pass.
- Entrypoint boots `ShellApp` from a temp project root.
- `ActivityLog` and `SessionsState` are attached.
- Path resolution flows through `VacPaths` only — the test suite
  asserts no `.stakpak` substring appears on resolved paths.
- The legacy `vac_tui_runtime` remains the default.

D2 is **not** authorized to begin until D1 is reviewed and passed.

## Appendix — D7A `vac_core` host-side exception (2026-04-26)

The hard "no `vac_core` dependency" rule above applies to every
crate inside the **shell-stack runtime path**: UI widgets, the
bridge, host-state crates, the app orchestrator, the entrypoint,
and the runtime loop. None of them may link `vac_core`.

D7A introduces the **first** explicit, named exception:

- **Crate**: `vac_shell_host_vac_engine_probe`.
- **Purpose**: project `vac_core::VacConfig` into the read-only
  `.vac/model_config.json` snapshot consumed by
  `vac_shell_host_vac_config`. The shell stack stays decoupled
  from `vac_core` because the *file* is the contract — the probe
  produces, the consumer parses, and no Rust type crosses the
  seam.
- **Posture**: host-side. The probe is a *producer crate* meant
  to be invoked by hosts (CLI bootstrap, `vac config probe`,
  dogfood example) **before** `build_shell_app`. It does not
  appear in the runtime dependency graph of `vac_shell_app`,
  `vac_shell_entrypoint`, or any UI widget.
- **Allowlist-shaped projection**: the on-disk schema is closed —
  `providers[]` (id + `credentials_present: bool`), `models[]`
  (id, label, reasoning, optional cost label), and an optional
  `active`. No `api_key_env` *names*, no `base_url`, no other
  `ProviderConfig` field crosses the seam.
- **Readiness parity** with `vil_llm::LlmConfig::provider_ready`:
  `Some(non-empty api_key_env)` ⇒ check the env;
  `Some("" | "   ")` ⇒ false (malformed config);
  `None` ⇒ true (local / no-key provider, ready when the
  provider entry exists). This is what keeps the
  `sanitize_active_model` boot guard from dropping local
  models that legitimately need no API key.
- **Public API** (post-hardening): `EnvPresence::present_non_empty`,
  `ProcessEnvPresence` (default impl), with
  `pub type StdEnvPresence = ProcessEnvPresence;` retained as a
  backwards-compatible alias. Producer entry points are
  `build_snapshot`, `build_snapshot_with_env`, `write_snapshot`,
  `write_snapshot_with_env`, and `write_snapshot_doc`.
- **Atomic write**: temp file via `create_new` → `write_all` →
  `sync_all` → cross-platform replace → parent-dir fsync on
  Unix (no-op on Windows; the journaling FS handles rename
  durability). On Windows, `rename` falls back to
  `remove_file` + `rename` so existing snapshots are
  overwritable; operators that need a hard atomic guarantee
  there are directed to `tempfile::persist`.
- **Forbidden directions** still hold: nothing in
  `vac_shell_*` (excluding the probe itself) may add a
  `vac_core` dep. The probe must not gain a back-edge from any
  shell-stack crate; it sits outside the runtime graph by
  design.

Rollback: delete `crates/vac_shell_host_vac_engine_probe/` and
its workspace member entry. The on-disk snapshot remains
parseable by `vac_shell_host_vac_config` regardless of how it
was produced (host-written, hand-edited, or absent → fixture
fallback).

Future host-side exception crates (e.g. a real
`VacCommandExecutorAdapter` bridge in D7B) require their own
appendix entry justifying the boundary crossing on the same
allowlist-shaped basis.

## Appendix — D7B `vac_session_engine` host-side exception (2026-04-26)

D7B introduces the **second** named exception, on the same
shape as D7A:

- **Crate**: `vac_shell_host_vac_command_adapter`.
- **Purpose**: implement `vac_shell_host_commands::ShellCommandExecutor`
  by submitting custom palette slashes through
  `vac_session_engine::submit_one`. Built-in slashes
  (`/chat`, `/runtime`, `/model`, `/sessions`) continue to be
  handled by `ShellApp::apply_event` and never reach this
  adapter. Unmapped custom slashes return
  `ShellCommandError::Unsupported(...)`.
- **Posture**: host-side. Hosts construct a
  `VacCommandExecutorAdapter` and attach it to a
  `ShellRuntimeContext` via `with_executor`. The library
  surface of `vac_shell_entrypoint` does **not** depend on this
  crate; only the dogfood example (a dev-dep path) installs it.
- **Boundary**: `vac_shell_runtime_loop` continues to depend on
  `vac_shell_host_commands` (trait only). The adapter crate is
  the *only* shell-stack crate allowed to depend on
  `vac_session_engine`. UI / widget / bridge / app crates
  remain forbidden from that dependency.
- **Execution model (D7B v1)**: each invocation builds a fresh
  `SubmitContext` with metadata
  `{ source: "shell_palette", command_id, slash, title }`,
  drives `submit_one` with the EchoAdapter LLM stub, a
  per-invocation `TranscriptWriter`, `TrivialCompactBoundary`,
  `UsageTracker`, and a default `CompactConfig`. Operators see
  the result via the durable transcript at
  `<root>/.vac/sessions/<id>.jsonl`.
- **Sync ↔ async**: `ShellCommandExecutor::execute` stays
  synchronous. The adapter resolves the impedance mismatch
  itself: under an existing tokio multi-thread runtime it uses
  `block_in_place` + `Handle::block_on`; under a current-thread
  runtime it dispatches into a helper thread; with no runtime
  it spins a private current-thread runtime. The shell trait
  surface is unchanged.
- **D5.1 stub retained**: `vac_shell_host_commands::VacCommandExecutorAdapter`
  remains in place as a backwards-compatible reference
  fixture. Hosts that prefer the explicit "no engine wired"
  failure mode keep using the stub; new hosts attach
  `vac_shell_host_vac_command_adapter::VacCommandExecutorAdapter`.

Rollback: delete `crates/vac_shell_host_vac_command_adapter/`
and revert the dogfood example to the stub. The
`ShellCommandExecutor` trait, the runtime loop, and every UI
crate are unaffected because no shell-stack runtime graph
depends on the adapter.

D7B v1 **does not** replace real provider routing — `EchoAdapter`
is the LLM. The slice proves the engine seam, transcript
durability, and operator-visible routing direction. Real
provider adapters and `vac_cli`-grade dispatch remain a later
slice and require their own ADR appendix entry.

## Appendix — D7C `vil_llm` host-side exception (2026-04-26)

D7C extends `vac_shell_host_vac_command_adapter` (the existing
D7B exception crate) to support real provider routing without
crossing a new boundary. The crate already lives outside the
shell-stack runtime graph, so adding `vil_llm` here does not
add a back-edge into UI / widget / bridge / app / runtime-loop
crates. `cargo tree -p vac_shell_entrypoint -e normal --depth 4
| grep vil_llm` is empty after this slice.

What changed:

- **`AdapterLlm` enum**: `Echo` (default — preserves D7B
  behaviour byte-for-byte) and `Custom(Arc<dyn LlmAdapter>)`
  (host-injected). Default stays `Echo` so existing call sites
  are unaffected.
- **`AdapterConfig::with_llm(Arc<dyn LlmAdapter>)`**: accept any
  `vac_session_engine::LlmAdapter` implementation —
  `CassetteAdapter`, a custom routing layer, or the
  `vil_llm`-backed bridge below. Errors returned by the
  injected adapter become `EngineError::Other` inside
  `submit_one`, which `VacCommandExecutorAdapter::execute`
  surfaces to the operator as `ShellCommandError::Failed`.
- **`AdapterConfig::with_vil_llm_router(vil_llm::LlmRouter)`**:
  convenience helper that wraps the router in
  `VilLlmRouterAdapter`. Each `submit_one` request becomes a
  single-message `vil_llm::LlmRequest` (role = user) including
  any `LlmRequest::context` lines prefixed onto the prompt.
  Provider selection, credential resolution (env-var driven),
  retry, rate-limiting, and fallback all stay inside
  `vil_llm::LlmRouter`.
- **`VilLlmRouterAdapter`**: pub bridge type. Holds an owned
  `vil_llm::LlmRouter`, impls
  `vac_session_engine::LlmAdapter`. Translates the response
  back into the engine's `LlmResponse` shape (provider, model,
  content, prompt/completion tokens). Tool-use is **not**
  translated — D7C v1 emits `tool_calls: vec![]` so the
  engine treats every response as text. Tool-use round-tripping
  is a later slice.
- **Provider identity preserved through fallback (D7C
  hardening)**: the bridge calls
  `LlmRouter::complete_with_provider`, a new public method
  that returns `(provider_name, LlmResponse)` so the
  *actual* provider that satisfied the request lands in the
  engine `LlmResponse.provider` field. The previous wrapper
  used `router.default_provider()`, which silently mislabelled
  fallback responses with the configured-default name. The
  legacy `LlmRouter::complete` is retained as a
  compatibility wrapper (`map(|(_, r)| r)`). Pinned by:
  * `complete_with_provider_reports_actual_fallback_provider`
    in `vil_llm` (unit; no external creds)
  * `vil_llm_router_bridge_records_actual_fallback_provider_in_transcript`
    in the adapter crate (integration; fake providers)
- **Transcript durability for tool-use (D7E)**: the engine
  (`vac_session_engine::submit.rs`) now appends a
  `TranscriptKind::ToolCall` row before gate / dispatch and a
  `TranscriptKind::ToolResult` row carrying the full
  `ToolResultEnvelope` after the envelope is built. Both
  rows persist regardless of dispatcher / gate outcome, so
  the unsupported-dispatcher path, gate-deny path, and
  dispatcher-success path all leave a durable, replayable
  trail. The dogfood operator path (`ShellCommandExecutor::execute`,
  no event sink) reads tool-use outcomes directly from the
  transcript file. Pinned by:
  * `unsupported_dispatcher_writes_tool_call_and_tool_result_rows`
    in `vac_session_engine`
  * `multiple_tool_calls_preserve_transcript_order`
  * `gate_deny_writes_error_tool_result_and_skips_dispatcher`
  * `dispatcher_ok_writes_ok_tool_result_row`
  * `d7e_tool_call_and_tool_result_rows_visible_via_execute_path`
    in the adapter crate (full execute path, no event sink)
- **Tool-use round-tripping (D7D)**: every
  `vil_llm::ToolCall { id, name, arguments }` is translated
  into `vac_session_engine::ToolCallRequest` with
  `reason: None` and `estimated_tokens: 0`. `vil_llm`'s
  `ToolCall` does not carry an operator-readable rationale or
  a token estimate today; populating these fields would
  fabricate metadata, so the bridge keeps them at the
  conservative defaults. The engine's existing dispatch loop
  continues to enforce host control: when `CompactConfig`
  carries no `ToolDispatcher` (every default `AdapterConfig`),
  each translated tool call falls through to
  `UnsupportedDispatcher`, which writes an error
  `ToolResult` envelope through the event channel **without
  aborting the submit**. Tool dispatch only goes live if the
  host has explicitly attached a dispatcher and gate to
  `CompactConfig` outside this slice's scope. Pinned by:
  * `vil_llm_router_bridge_translates_tool_calls_into_engine_response`
    (direct bridge unit test)
  * `vil_llm_router_bridge_emits_empty_tool_calls_when_provider_emits_none`
  * `engine_remains_safe_when_tool_calls_arrive_without_a_dispatcher`
    (full execute path; verifies the no-dispatcher safety
    contract D7D relies on)

Boundary check after D7C:

```text
vac_shell_host_vac_command_adapter (depth 1):
├── async-trait
├── serde_json, thiserror, tokio, uuid
├── vac_session_engine        (D7B exception)
├── vac_shell_contracts
├── vac_shell_host_commands
└── vil_llm                    (D7C exception)
```

Nothing else in the shell-stack runtime graph (entrypoint
library, runtime loop, app, every UI/widget/bridge crate)
gains an edge to `vil_llm` — verified by `cargo tree`.

Sync trait surface: unchanged. The `block_in_place` /
helper-thread / private-runtime triad introduced in D7B
continues to handle the sync-to-async impedance. The Custom
arm clones the `Arc<dyn LlmAdapter>` out of `self` into the
future, so the future owns a stable reference for the entire
`submit_one` lifetime.

Operator-visible failure: pinned by
`custom_adapter_engine_error_surfaces_as_shell_command_failed`
in `tests/llm_adapter.rs`. Any provider-level error (auth,
network, rate limit) becomes a `ShellCommandError::Failed`
with the underlying message preserved, which the existing
`route_palette_command` reporter writes into `ActivityLog`.

Rollback: drop the new `AdapterLlm` enum and the bridge type;
revert `with_llm` / `with_vil_llm_router` helpers; delete the
`vil_llm` dep from the adapter crate. The default
`AdapterLlm::Echo` path means no host wiring breaks during
rollback.
