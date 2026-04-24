# VAC TUI UX Spec

**Snapshot:** 2026-04-24 (post U0–U8 unification landing +
post-audit hardening). Not a plan — a description of what the
cockpit looks like today. When behaviour changes, regenerate this
file from the codebase.

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

Reserved (future producers): `lsp`, `vil`, `subagent`, `memory`,
`policy`, `rate` — slot into `SystemFacetKind` via its
`#[non_exhaustive]` attribute.

## The four surfaces

### 1. Statusline

**Where:** bottom row, full width, always visible.

**Format:**

```
[MODE] | Model: X | Tokens: N | MANUAL | Provider: ready | MCP: 2 · approvals✓ · runtime✓ · mcp✓2 · shell✓ · spec· · env:host
```

The `·`-separated suffix is the `SystemPulse::compact_line()`
output. Each token is styled per its facet's `FacetSeverity`.

Width budget: ≤ 80 chars for the pulse suffix
(`pulse_compact_line_fits_width_budget` contract test).

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

## What is NOT in the TUI today

Producers that exist as primitives but don't yet push events into
the unified surfaces:

- `PolicyTracker` — primitive in `vac_core::policy_limits`, not
  called from `submit_one`.
- `RateLimitTracker` — primitive in `vil_llm::rate_limit`, not
  called from the LLM router.
- `AutoDreamService`, `AwaySummaryService`,
  `PassiveFeedbackDriver` — primitives in
  `vac_tui_runtime::services::*`, not driven by the REPL poll loop.
- `ElicitationHandler` — trait in `vac_mcp_core`, no concrete
  driver wiring yet.
- OAuth login binary — `PkceChallenge` + `TokenCache` primitives in
  `vac_bridge::auth`, no `vac auth login <provider>` wired yet.

When those producers come online, they slot into `SystemPulse`
via new `SystemFacetKind` variants — the enum is
`#[non_exhaustive]` precisely for this expansion.

## Where to look next in code

- `crates/vac_tui_runtime/src/system_pulse.rs` — projection + facets.
- `crates/vac_tui_runtime/src/services/notify_router.rs` — lane routing.
- `crates/vac_tui_runtime/src/action_registry.rs` — chord registry.
- `crates/vac_tui_runtime/src/view/overlays.rs::render_shortcuts` — popup.
- `crates/vac_tui_runtime/src/view/operator.rs::render_operator_panel` — panel.
- `crates/vac_tui_runtime/src/services/statusline.rs::render_statusline` — row.
- `crates/vac_tui_runtime/src/contracts_test.rs` — invariants.
- `crates/vac_cli/src/commands/onboard.rs` — checklist command.
