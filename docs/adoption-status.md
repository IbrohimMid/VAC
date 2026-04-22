# Adoption Status — Honest Audit

Status audit of VAC's progress toward the analysis in the original
comparison with Stakpak, Trae, Claude Code, and OMNI. Percentages are
deliberately conservative — API surface alone does not count; the
implementation has to be wired and reachable from a real run path.

## Scoring rule

For each recommended adoption item:

- **100%** — implemented, wired, exercised by tests, and reachable from
  a real run path (CLI, TUI boot, or an existing CLI subcommand).
- **50%** — API defined and tested in isolation but not wired (dead
  code at runtime).
- **0%** — not present.

## Per-donor breakdown

### OMNI → `vac_signal` distillation layer (honest: ~60%)

| Item | Status | Evidence |
|---|---|---|
| Registry → Scorer → Distiller primitives | 100% | `crates/vac_signal/src/{buffer,score,distill,registry}.rs` |
| `SignalBuffer` wired to producers | 100% (vil_dev), 50% (shell paralel) | `AppState.vil_dev.output`, `ShellSession.output_signal` |
| `RewindStore` active per session | 100% | `event_loop.rs` opens `.vac/signal/<session>.db`, persists every `checkpoint_interval_secs` |
| `vac signal list/tail` CLI | 100% | `crates/vac_cli/src/commands/signal.rs`, `--features signal-rewind` |
| Distilled view actually rendered | 50% | logged via tracing, not yet a workbench panel |
| MCP retrieval tools (`signal.tail` MCP) | 0% | requires ToolContext plumbing; not done |
| Filter hierarchy + trust model | 0% | flat 4-class scorer only |
| Session rollup summary records | 0% | not emitted |
| Stream wiring to runtime jobs | 0% | `vac_runtime::Job.output` still string-based |

### Stakpak → UX discipline (honest: ~75%)

| Item | Status | Evidence |
|---|---|---|
| AppState slimming | 80% | 16 sub-structs extracted: ApprovalsState, VilDevState, FilePickerState, AtMentionState, AskUserState, ChangesetUiState, WorkbenchChromeState, SwitchersState, BannerState, FileIndexState, CommandPaletteState, PinsState, MessageUiState, SidePanelState, SessionResumeState, TaskTrayState. AppState dropped from ~95 flat fields to ~20 flat + 16 grouped. |
| Banner single-visible rule | 100% | F2 contract tests in `services/banner.rs` |
| Overlay stack depth cap | 100% | F6 — `OverlayManager::MAX_STACK_DEPTH = 2`, refuses 3rd push with warn log |
| Operator UX audit | 0% | no formal review yet |
| Autopilot guardrail ergonomics | 0% | not touched |

### Trae → research / eval modularity (honest: ~70%)

| Item | Status | Evidence |
|---|---|---|
| `AgentStrategy` trait | 100% | `crates/vil_swarm/src/strategy.rs`, `DefaultStrategy` + `ConservativeStrategy` |
| Wired into `SwarmOrchestrator` | 100% | `strategy` field on struct, `set_strategy_by_name` called from `vac_core::engine` bootstrap |
| Config-driven selection | 100% | `SwarmConfig.strategy: String` read at bootstrap |
| `AgentDecision` emission site | 50% | emitted when strategy is resolved; NOT yet emitted at runtime tool-pick sites (orchestrator loop still uses legacy path) |
| `vac decisions` CLI | 100% | dumps AgentDecision records from a trace file |
| `vac eval` CLI | 100% | prints DecisionScore |
| `vac eval --golden <file>` | 100% | per-pair match + aggregate % |
| `VacConfig::minimal()` | 50% | constructor exists + tested; only consumed via code (not CLI flag yet) |

### Claude Code → friction discipline (honest: ~45%)

| Item | Status | Evidence |
|---|---|---|
| Boot tracing spans | 100% | `tui_boot`, `boot_kitty_probe`, `boot_config_load`, `boot_session_snapshot`, `boot_mcp_probe_spawn`, `boot_vil_dev_spawn` |
| Duplicate config load eliminated | 100% | single `boot_config` reused |
| Boot latency integration test | 100% | `tests/boot_latency.rs` asserts <150ms |
| Overlay z-order contract | 100% | 5 `contract_*` tests in `overlay.rs` |
| Overlay max depth 2 | 100% | F6 |
| Banner single-visible contract | 100% | F2 |
| `load_session_snapshot` deferred past first frame | 0% | still blocks boot |
| Primary-action benchmark | 0% | no criterion bench yet |
| Diff/review keyboard-only navigation test | 0% | not done |
| Approval 1-key contract test | 0% | not done |
| First-paint latency empirical measurement | 0% | spans exist but no published timing |

## Overall

Weighted simple-average of the four donors, conservative scoring:

| Donor | Honest % |
|---|---|
| OMNI | 60% |
| Stakpak | 75% |
| Trae | 70% |
| Claude Code | 45% |
| **Overall** | **~62%** |

## What changed from the "80% claim" commit

Before this audit, three APIs were defined but never called in production:

1. `SignalRegistry::persist_to_rewind` — now called every
   `checkpoint_interval_secs` in `event_loop.rs`.
2. `AgentStrategy` + `strategy_from_name` — now held by
   `SwarmOrchestrator.strategy` and switched via
   `SwarmConfig.strategy`.
3. `record_agent_decision` — now emitted once per engine boot when the
   strategy is resolved.
4. `AppState::vil_dev_distilled` — now called in event loop to log
   distillation summaries every persist tick.
5. `AppState::signal_registry` — now consumed by the persist task.

The previous "80%" claim was based on API surface; after wiring, the
honest number is ~62%. Work remains on real runtime integration of
OMNI MCP tools, Claude Code friction fixes (F1/F3/F4/F5), and using
`AgentStrategy::choose_next` at actual orchestrator decision sites.

## Test + build state

- `cargo check --workspace` — green
- `vac_tui_runtime` — 349/349 tests pass
- `vac_signal` — 14/14 tests pass (incl. feature `rewind`)
- `vac_trajectory` — 7/7 tests pass
- `vac_trace` — 14/14 tests pass
- `vil_swarm` — 79/79 tests pass
