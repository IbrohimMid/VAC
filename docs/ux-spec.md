# VAC TUI UX Spec

**Snapshot:** 2026-04-24 (post U0–U8 unification + finalization
pass A1/A2/B1/B2/B3/D1/E1/F2). Not a plan — a description of what
the cockpit looks like today. When behaviour changes, regenerate
this file from the codebase.

## Single-sentence summary

One action registry, one borrow-only state projection, one
notification router, one deep-link function. Every feature W1–W10
surfaces through the same grammar — same glyphs, same severity
colours, same vocabulary — on statusline, operator panel, activity
timeline, and shortcuts popup.

## The unified grammar

### Severity

Four levels, one glyph each. Used identically by SystemPulse,
NotifyRouter, onboard checklist, and every downstream renderer.

| Severity | Glyph | Colour | Meaning |
|---|---|---|---|
| `Ok` | ✓ | green (`StyleKey::Success`) | Subsystem healthy / idle |
| `Info` | · | dim (`StyleKey::Accent`) | Subsystem active, no attention needed |
| `Warn` | ● | yellow (`StyleKey::Warning`) | Operator should notice |
| `Critical` | ✗ | red (`StyleKey::Error`) | Operator should act |

### Subsystem labels

Every user-visible reference to a subsystem uses the same short
identifier. Defined by `SystemFacetKind::label` and matched by
`NotifyRouter`'s `subsystem` field.

| Label | Source |
|---|---|
| `approvals` | `AppState.execution.approvals` |
| `runtime` | `AppState.execution.runtime.jobs` |
| `mcp` | `AppState.execution.mcp_maps.server_states` |
| `shell` | `AppState.execution.shell.session_store.sessions` |
| `spec` | `AppState.speculation` (W6 primitive) |
| `env` | `AppState.core.startup.environment` |
| `budget` | `UsageTracker` vs `CompactConfig.max_budget_tokens` (env-gated via `VAC_BUDGET_TOKENS`) |
| `memory` | `.vac/memory/archive` cache (refreshed by idle tick; `WorkbenchTab::Memory`) |
| `subagent` | `RootObservables.errors_seen + notifications.len()` |
| `policy` | `AppState.execution.policy` — `PolicyTracker::snapshot` (submits/hr + tokens vs caps) |
| `lsp` | `AppState.execution.lsp` — `PassiveFeedbackDriver::tick` last-run counter |
| `tasks` | `AppState.execution.task_tray.entries` — 7-kind `TaskKind` enum (A.5) |
| `cron` | `AppState.execution.cron` — `.vac/cron.json` snapshot refreshed by `spawn_cron_loop` (C.2) |

Reserved (future producers): `rate`, `vil` — slot into
`SystemFacetKind` via its `#[non_exhaustive]` attribute.

**Current facet count:** 13 (contract-pinned in
`contracts_test::pulse_compact_line_fits_width_budget` at
≤ 120 chars — raised from 100 at C.2 to fit the two new
facets on the standard row).

**Current BRIDGE_ALLOWLIST size:** 19 subsystem labels —
trust / env / mcp / spill / policy / rate / memory / resume /
spec / lsp / compact / gate / skills / cron / schedule /
monitor / hooks / web / rewind. Any new `warn!` / `error!`
target from a compete-blueprint primitive MUST register here
before merge.

## The four surfaces

### 1. Statusline

**Where:** bottom row, full width, always visible.

**Format:**

```
[MODE] | Model: X | Tokens: N | MANUAL | Provider: ready | MCP: 2 · approvals✓ · runtime✓ · mcp✓2 · shell✓ · spec· · env:host
```

The `·`-separated suffix is the `SystemPulse::compact_line()`
output. Each token is styled per its facet's `FacetSeverity`.

Width budget: ≤ 120 chars for the pulse suffix
(`pulse_compact_line_fits_width_budget` contract test; scaled
80 → 100 → 120 across the arc as the pulse grew to 13 facets).

### 2. Operator panel

**Where:** right-rail panel (`view::operator::render_operator_panel`).

**Contents (top → bottom):**

1. Spinner / streaming indicator (if active).
2. Static session metadata (environment, exec mode, tool count,
   approvals count, modified files).
3. Shell session preview (last 2 output lines, if active).
4. MCP connection summary (connected / total).
5. **System section** — one line per `SystemFacet`:
   ```
   ✓ approvals  pending:  0
   ✓ runtime    running:  0
   ✓ mcp        connected: 2
   ✓ shell      sessions: 0
   · spec       predicted: no
   ✓ env        mode: host
   ```

The System section and the statusline suffix both derive from the
same `SystemPulse::facets()` call — grammar drift between them is
caught by `contracts_test::every_pulse_facet_nav_target_is_applyable`.

### 3. Activity timeline + toast + banner

**Where:** Activity panel (right-rail); toast ring (floating);
banner strip (top).

**Router:** `services::notify_router::route(&mut state, NotifyEvent)`.
No subsystem writes directly to toast / banner; everything goes
through the router.

**Severity → lane matrix** (pinned by
`contracts_test::notify_router_severity_lane_matrix_is_stable`):

| Severity | Activity | Toast | Banner |
|---|---|---|---|
| `Info` | ✓ (kind: `Status`) | | |
| `Warn` | ✓ (kind: `Status`) | ✓ (Warning, 4s) | |
| `NonBlockingCritical` | ✓ (kind: `Error`) | | ✓ (Error style) |
| `OperatorDecision` | reserved — approvals / ask-user / reject-reason own this slot | | |

Every row is prefixed `[<subsystem>] <summary>` using the same
vocabulary as the SystemPulse labels.

**Tracing bridge (A1 + D2):** `services::tracing_bridge::TuiTracingLayer`
is a `tracing_subscriber::Layer` that forwards `info!` / `warn!` /
`error!` events on an allowlisted set of subsystem targets directly
into `NotifyRouter`. Severity mapping:

- `error!` → `NotifyEvent::critical` (activity + banner)
- `warn!`  → `NotifyEvent::warn` (activity + toast)
- `info!`  → `NotifyEvent::info` (activity only — breadcrumb lane
  used by D2 speculation warmed-reads, auto_dream writes, etc.)

Prefix match means nested module paths (`vac_tools::trust_gate::foo`)
resolve to the parent subsystem (`trust_gate`). Enable via
`VAC_TRACING_BRIDGE=1` at startup.

Length caps (post-audit hardening):
- `NOTIFY_SUBSYSTEM_CAP = 64 chars`
- `NOTIFY_SUMMARY_CAP   = 512 chars`
- `NOTIFY_DETAIL_CAP    = 4 KiB`

### 4. Shortcuts popup (`Ctrl+P → ?` or `/help`)

**Where:** centred popup overlay.

**Source:** generated from `action_registry::ACTION_SPECS`, grouped
by scope. `Global` scope shown first, then other scopes in
alphabetical order. An appended "View-internal navigation" section
documents chords that aren't handler-backed actions (Tab, Ctrl+Tab,
PageUp/Down, arrows, Enter, Esc).

**Drift guards:**
- `action_specs_no_duplicate_keybindings_in_scope` — no two actions
  can claim the same chord in the same scope.
- `action_specs_and_helper_block_overlap_is_consistent` — when a
  slash alias appears in both `ACTION_SPECS` and `vac_commands`, it
  must round-trip via `spec_by_slash_alias` to the same ActionId.

## Keybinding facts

**Canonical global chords** (from `ACTION_SPECS`, `ActionContext::Global`):

| Chord | Action |
|---|---|
| `Ctrl+C×2` | Quit |
| `Ctrl+C` | Cancel stream (when streaming) |
| `Ctrl+P` | Command palette |
| `Ctrl+Shift+P` | File picker |
| `Ctrl+F` | File search |
| `?` | Shortcuts popup |
| `Esc` | Cancel / close overlay |
| `Enter` | Submit / select |

`Ctrl+P` resolved a pre-unification collision with `OpenFilePicker`
— file picker moved to `Ctrl+Shift+P`. Contract test prevents
regression.

**View-internal navigation** (handled by input layer, not action
registry):

| Chord | Behaviour |
|---|---|
| `Tab` | Cycle focus pane |
| `Ctrl+Tab` | Cycle workbench tabs |
| `PageUp` / `PageDown` | Scroll pane content |
| `Up` / `Down` | List / history navigation |

**Workbench-tab-specific chords** exist for Review, Approvals,
Sessions, Runtime, Agents, Plan, VIL, VWFD tabs — see the grouped
shortcuts popup for the live list.

## Deep-link navigation

`NavTarget` carries where Enter on a facet takes the operator.

| Facet | NavTarget | Lands at |
|---|---|---|
| approvals | `WorkbenchTab(Approvals)` | Approvals tab |
| runtime | `WorkbenchTab(Runtime)` | Runtime tab |
| mcp | `WorkbenchTab(Signal)` | Signal tab |
| shell | `Overlay(ShellPopup)` | Shell popup |
| spec | `None` | observational only |
| env | `None` | observational only |

`NavTarget::apply(&mut state)` is idempotent — applying the same
WorkbenchTab target twice returns `false` on the second call. Tests
`nav_target_apply_switches_workbench_tab` and
`nav_target_apply_opens_overlay` pin the semantics.

## Onboarding (`vac onboard`)

Post-install checklist printed by `vac onboard`. Uses the same
glyph grammar (✓ · ✗). Checks six surfaces:

| Item | Status semantics |
|---|---|
| `.vac/config.toml` | Missing → `vac init` |
| `.vac/policy.toml` | Info (absence = unlimited) |
| `.vac/sandbox.toml` | Info → `vac sandbox-toggle` |
| `.vac/memory/` | Info (auto-created) |
| LSP binaries on PATH (4 probes: rust-analyzer, pyright, tsserver, gopls) | Info when missing + shows the env-var override |
| git working tree | Info → `git init` for bughunter / AwaySummary |

Probes honour the same env-var overrides
(`VAC_LSP_RUST_SERVER` et al.) as the `LspServerManager` pool
from W5.1 — one source of truth for "which binary runs for Rust".

## Contract tests (enforced in CI)

Located in `crates/vac_tui_runtime/src/contracts_test.rs`. These
make drift impossible without a test failure:

| Test | Invariant |
|---|---|
| `system_pulse_source_has_no_clone_or_arc_on_pulse_type` | SystemPulse stays borrow-only (no third event plane) |
| `every_pulse_facet_nav_target_is_applyable` | Facet NavTargets reference live tabs/overlays |
| `pulse_compact_line_fits_width_budget` | Statusline suffix ≤ 80 chars |
| `notify_router_severity_lane_matrix_is_stable` | Info→activity, Warn→activity+toast, Critical→activity+banner |
| `action_specs_no_duplicate_keybindings_in_scope` | No chord collision per scope |
| `action_specs_slash_aliases_globally_unique` | Slash aliases don't collide |
| `action_specs_and_helper_block_overlap_is_consistent` | Two registries agree on overlapping aliases |

## Idle maintenance (D1)

`services::idle_maintenance` spawns three tokio tasks from the
event loop boot sequence:

| Task | Cadence | Effect |
|---|---|---|
| `spawn_prune_spill_loop` | hourly tick | `vac_tools::result_spill::prune_spill_dir` with 24h retention — bounds `.vac/tool-results/` disk use |
| `spawn_auto_dream_loop` | per-minute poll | gates internally on 5-min idle, triggers `AutoDreamService` memory consolidation |
| `away_summary_probe` | one-shot at startup | routes an info notification when session gap ≥ 1h |

Failures on any job emit `tracing::warn!` on a subsystem target
(`auto_dream`, `away_summary`, `result_spill`), which the A1
bridge then surfaces in the activity panel.

## Elicitation (G1)

MCP servers can request an interactive elicitation mid-session
(`elicitation/request`). VAC ships two halves:

- **`McpElicitationRegistry`** (`vac_mcp_core::elicitation`) —
  connection-scoped `attach_elicitation_handler(Arc<dyn
  ElicitationHandler>)`. Kept separate from the serialisable
  `McpConnection` state record so clone/equality on state
  doesn't touch the handler Arc. Absent handler falls back to
  `UnsupportedElicitationHandler` → `Cancelled`.
- **`TuiElicitationHandler`** (`services/elicitation.rs`) — locks
  the shared `AppState`, parks an `ElicitationPrompt { url,
  prompt, response_tx }` on `layout.elicitation`, opens
  `OverlayId::Elicitation`, awaits a oneshot (120 s timeout →
  `Cancelled`).

Input contract: Enter → `open::that(url)` + `ElicitationResult::
Accepted{}`. Esc → `Cancelled`. Text/Confirm variants degrade to
`Cancelled` with a `warn!` on target `vac_mcp_core::channel` —
follow-up ships their own modal shape.

## CLI auth (G2)

`vac auth oauth <provider>` runs the full PKCE flow from a
terminal: 32-byte OS entropy → `PkceChallenge::generate` → bind
`127.0.0.1:0` → `open::that(auth_url)` → parse `code=` from the
loopback GET → token exchange via `reqwest` → `TokenCache::save`
at `~/.vac/auth/<provider>.json`. Palette slash `/auth-login` is
the in-TUI launcher.

Provider registry is deliberately empty for Anthropic/OpenAI
(both issue API keys, not OAuth tokens) — operators with private
IdPs pass `--auth-url / --token-url / --client-id`.

## Compete-blueprint primitives (Phase 0 + A–D)

The 26-task arc in `docs/COMPETE_EXECUTION_PLAN.md` landed
library primitives for everything Claude Code exposes that VAC
previously lacked. Grammar-facing surfaces (covered above):

- **Streamed agent loop** (A.1 – A.6): `SubmitChunk` +
  `submit_stream` + `CompositeGate` (policy / trust / approvals /
  hooks / plan) + tool-use iteration + auto-compact circuit
  breaker + `tasks` facet. Back-pressure via bounded 256-chunk
  channel.
- **Subagents** (B.1 – B.4): `SubagentRunner` writes a
  `TranscriptKind::Sidechain` row in the parent transcript per
  run. `AgentTool` exposes 5 built-ins (explore / plan / verify /
  general-purpose / statusline-setup) + the markdown skills
  registry (`.vac/skills/*.md`) for custom kinds. `PlanModeGate`
  enforces a strict write-gate with a read-only allowlist.
- **Autonomous loops** (C.1 – C.6): `CronStore` +
  `spawn_cron_loop` + `cron` facet; `MonitorTool` streams
  regex-matched subprocess stdout through NotifyRouter;
  `ScheduleWakeup` + `/loop` dynamic pacing; `HookRegistry` with
  9 events × 4 command types, shell-hook exec denies on
  non-zero exit; `HookFireRecord` ring on ExecutionState for
  the Runtime sub-pane.
- **Portable sessions** (D.1 – D.7): `web::fetch` with 2 MB
  response cap + allowlist; `SearchBackend` trait + Brave;
  `enter_worktree` / `exit_worktree`; `/statusline` +
  `/output-style`; `/ctx` extended with sidechain + compaction
  ribbons; `/scrub-back` + `/thinkback-play` palette entries;
  `issue_teleport_token` + `validate_teleport_token` + SSE-
  ready `RemoteSessionConfig`.

Per the unified-UX discipline, every new primitive above either
lives behind a palette entry + slash alias, routes through the
A1 tracing bridge, or — when it has ambient state — earns a
`SystemFacet`. No orphan overlays, no new severity lanes.

## Palette (B1 / F2)

Action registry was extended with sixteen new palette rows, all
`ActionContext::Global`:

- **Six skills** (B1): `SkillBatch`, `SkillLoop`, `SkillRemember`,
  `SkillSimplify`, `SkillStuck`, `SkillVerify` — slash-accessible
  via `/skill-<name>`.
- **Ten CLI bridges** (F2): `SpawnCliAdvisor`, `SpawnCliAutofixPr`,
  `SpawnCliBughunter`, `SpawnCliSecurityReview`, `SpawnCliPerfIssue`,
  `SpawnCliTeleport`, `SpawnCliThinkback`, `SpawnCliUltraplan`,
  `SpawnCliRewind`, `SpawnCliDecisions`. Each launches the matching
  `vac <cmd>` in a shell popup so W8 commands are reachable without
  leaving the TUI.

Invariants (`action_specs_no_duplicate_keybindings_in_scope` +
`action_specs_slash_aliases_globally_unique`) pin these rows
against future drift.

## What is NOT in the TUI today

Producers that exist as primitives but don't yet push events into
the unified surfaces:

- `PolicyTracker` — primitive in `vac_core::policy_limits`, not
  called from `submit_one`.
- `RateLimitTracker` — primitive in `vil_llm::rate_limit`, not
  called from the LLM router.
- `PassiveFeedbackDriver` — primitive in
  `vac_tui_runtime::services::passive_feedback`, no REPL tick yet.
- `ElicitationHandler` — trait in `vac_mcp_core`, no concrete
  driver wiring yet.
- OAuth login binary — `PkceChallenge` + `TokenCache` primitives in
  `vac_bridge::auth`, no `vac auth login <provider>` wired yet.

`AutoDreamService` and `AwaySummaryService` were in this list
previously — D1 moved them into the live cockpit via
`idle_maintenance`.

When those producers come online, they slot into `SystemPulse`
via new `SystemFacetKind` variants — the enum is
`#[non_exhaustive]` precisely for this expansion.

## Where to look next in code

- `crates/vac_tui_runtime/src/system_pulse.rs` — projection + facets.
- `crates/vac_tui_runtime/src/services/notify_router.rs` — lane routing.
- `crates/vac_tui_runtime/src/services/tracing_bridge.rs` — warn/error → NotifyRouter forwarding.
- `crates/vac_tui_runtime/src/services/idle_maintenance.rs` — spill prune / auto-dream / away-summary tasks.
- `crates/vac_tui_runtime/src/action_registry.rs` — chord registry.
- `crates/vac_tui_runtime/src/view/overlays.rs::render_shortcuts` — popup.
- `crates/vac_tui_runtime/src/view/operator.rs::render_operator_panel` — panel.
- `crates/vac_tui_runtime/src/services/statusline.rs::render_statusline` — row.
- `crates/vac_tui_runtime/src/contracts_test.rs` — invariants.
- `crates/vac_cli/src/commands/onboard.rs` — checklist command.
