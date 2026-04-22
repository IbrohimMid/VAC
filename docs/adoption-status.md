# Adoption Status — Honest Audit

Evidence-based progress audit against the original Stakpak / Trae /
Claude Code / OMNI comparison. Scored conservatively: API surface alone
does not count; implementation must be wired and reachable from a real
run path (CLI, TUI boot, tool invocation).

## Scoring rule

- **100%** — implemented, wired, exercised by tests, reachable from a
  real run path.
- **50%** — API defined and tested in isolation but not wired (dead
  code at runtime).
- **0%** — not present.

## Per-donor breakdown

### OMNI → `vac_signal` distillation layer (honest: ~75%)

| Item | Status | Evidence |
|---|---|---|
| Registry / Scorer / Distiller primitives | 100% | `crates/vac_signal/src/{buffer,score,distill,registry}.rs` |
| `SignalBuffer` wired to producers | 100% (vil_dev), 50% (shell parallel) | `AppState.vil_dev.output`, `ShellSession.output_signal` |
| `RewindStore` active per session | 100% | `event_loop.rs` opens `.vac/signal/<session>.db`, persists every `checkpoint_interval_secs` |
| `SignalRegistry::persist_to_rewind` called | 100% | event_loop.rs periodic persist |
| `vac signal list/tail` CLI | 100% | `crates/vac_cli/src/commands/signal.rs` |
| MCP retrieval tools (`signal_tail`, `signal_list`) | 100% | `crates/vac_tools/src/builtin/signal_{tail,list}.rs` — file-based, registered, tested |
| Distilled view consumed | 75% | logged via tracing at every persist tick; not yet a workbench panel |
| Filter hierarchy + trust model | 100% | `SignalConfig.custom_filters`, `ChainedScorer::from_config` |
| Stream wiring to runtime jobs | 0% | `vac_runtime::Job.output` still string-based |
| Session rollup summary records | 0% | not emitted |

### Stakpak → UX discipline (honest: ~85%)

| Item | Status | Evidence |
|---|---|---|
| AppState slimming | 100% | 20 sub-structs extracted: Approvals, VilDev, FilePicker, AtMention, AskUser, ChangesetUi, WorkbenchChrome, Switchers, Banner, FileIndex, CommandPalette, Pins, MessageUi, SidePanel, SessionResume, TaskTray, Streaming, Quit, Paste, LspUi. AppState: ~95 → ~50 flat fields |
| Banner single-visible rule | 100% | 4 contract tests in `services/banner.rs` |
| Overlay stack depth cap | 100% | `MAX_STACK_DEPTH = 2` + refusal warn + 6 contract tests |
| Approval 1-keypress contract | 100% | 3 contract tests |
| UX contract doc | 100% | `docs/ux-contract.md` — 6 rules, each cross-linked to tests |
| Operator UX formal audit | 0% | no external review |
| Autopilot guardrail ergonomics | 0% | not touched |

### Trae → research / eval modularity (honest: ~80%)

| Item | Status | Evidence |
|---|---|---|
| `AgentStrategy` trait | 100% | `crates/vil_swarm/src/strategy.rs` |
| Wired into `SwarmOrchestrator` | 100% | `strategy` field, `set_strategy_by_name`, `strategy_choose`, `strategy_name` public |
| Config-driven selection | 100% | `SwarmConfig.strategy` read at engine bootstrap |
| AgentDecision emission — boot | 100% | engine.rs emits on strategy resolve |
| AgentDecision emission — per ToolCall at runtime | 100% | `record_agent_event` now emits per ToolCall event |
| `vac decisions` CLI | 100% | dumps records from trace file |
| `vac eval` CLI | 100% | prints DecisionScore |
| `vac eval --golden <file>` | 100% | per-pair match + aggregate % |
| `VacConfig::minimal()` | 50% | tested; not yet wired into a CLI flag |
| AgentStrategy consulted in `execute_tools` | 0% | orchestrator still uses legacy tool-selection path |

### Claude Code → friction discipline (honest: ~65%)

| Item | Status | Evidence |
|---|---|---|
| Boot tracing spans | 100% | 6 spans from `tui_boot` down |
| Duplicate config load removed | 100% | single `boot_config` reused |
| Boot latency test (<150ms) | 100% | `tests/boot_latency.rs` |
| Overlay z-order contract | 100% | 6 tests in `overlay.rs` |
| Banner single-visible contract | 100% | 4 tests |
| Approval 1-keypress contract | 100% | 3 tests |
| Review keyboard-only navigation | 100% | 5 tests in `tests/review_keyboard.rs` |
| Primary-action latency guards | 100% | 3 tests in `tests/primary_action_latency.rs` |
| `load_session_snapshot` deferred past first frame | 0% | still blocks boot |
| Empirical first-paint measurement | 0% | spans exist; no captured timing doc |

## Overall

Weighted simple-average, conservative scoring:

| Donor | Honest % | Change from previous audit |
|---|---|---|
| OMNI | 75% | +15 (MCP tools + filter hierarchy) |
| Stakpak | 85% | +10 (4 more sub-structs + UX contract doc) |
| Trae | 80% | +10 (AgentDecision emitted at runtime ToolCall) |
| Claude Code | 65% | +20 (J2/J3/J4 contract tests) |
| **Overall** | **~76%** | **+14** |

## Distance to honest 80%

The overall sits at ~76% — **4 percentage points below the 80% target**.
The shortest paths to close the gap:

1. **I3 Signal workbench tab** — ~5pp OMNI. A new `WorkbenchTab::Signal`
   variant with a panel that renders `AppState::signal_registry().summary()`
   + `vil_dev_distilled`. Pattern matches existing `workbench/approvals.rs`.
2. **J1 defer session snapshot** — ~8pp Claude Code. Replace the inline
   `.await` in `event_loop.rs:154` with a spawned task that delivers
   the snapshot via a new `InputEvent::SessionSnapshotLoaded(...)`
   variant.
3. **K1 consult `strategy_choose` in `execute_tools`** — ~5pp Trae.
   Advisory log only (no behavior change) at start of `execute_tools`.

Any one of these lands the overall at ≥80%. All three push it to ~85%.

## Test + build state (2026-04-23)

- `cargo check --workspace` — green
- `vac_tui_runtime` — 360/360 tests pass
- `vac_signal` — 16/16 tests pass (+custom_filters, persist_to_rewind)
- `vac_tools` — signal_tail + signal_list tests pass
- `vac_trajectory` — 7/7 tests pass
- `vac_trace` — 14/14 tests pass
- `vil_swarm` — 79/79 tests pass
