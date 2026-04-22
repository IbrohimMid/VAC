# VAC TUI Parity Dashboard

**Last audited:** 2026-04-22 HEAD `7d47b55` (post Task-1 audit)

## Metrics

| Metric | Target | Actual | Status |
|---|---|---|---|
| Files >600 lines | 0 (ideal), <=6 (gate) | 0 | ✅ |
| Raw `Color::` outside theme | 0 | 0 (audited 2026-04-22) | ✅ |
| Tests Green (lib) | >= 315 | 315/315 | ✅ |
| Tests Green (integration) | >= 81 | 81/81 | ✅ |
| Wave 1 | complete | all 15 gates PASS (see `docs/audit/WAVE1_GATE_MAPPING.md`) | ✅ |
| Wave 2 | functionally-complete | T6,T7,T8,T9,T10 landed; PR-T5 theme closeout complete | ✅ |
| Wave 2.5 | functionally-complete | T3=0, Color localized=0, block_in_place=0, check_sync_io=0 | ✅ |
| Wave 3 | materially-advanced | T11 complete, T12 complete, T13 complete, T14 partial (→ Task-6,7), T15 complete | ⚠️ |
| Wave 4 | materially-advanced | T16 partial (→ Task-5), T17 partial (→ Task-8,9), T18 complete, T19 complete | ⚠️ |
| Runtime: `block_in_place` in TUI handlers | 0 | 0 | ✅ |

## Item Status (calibrated to HEAD 2026-04-22)

| Item | Status | Notes / Closeout Task |
|------|--------|----------------------|
| Wave 1 | **complete** | All gates PASS — `docs/audit/WAVE1_GATE_MAPPING.md` |
| Wave 2 | **functionally-complete** | T5–T10 landed with tests |
| Wave 2.5 | **functionally-complete** | files>600=0, Color::=0, block_in_place=0 |
| T11 VWFD inspector | **complete** | `VwfdInspectorState` wired, service+handlers present |
| T12 vil-expr validator | **complete** | `services/vil_expr_lint.rs` + 6 tests green |
| T12.1 UI wiring | **partial** | Input hook + render + Alt+H popup → Task-4 |
| T13 VIL-aware diff overlay | **complete** | `vwfd_diff_render` delegation in `services/review.rs` |
| T14 Background vil dev | **materially-advanced** | Substrate + log panel; runner events not yet routed to task tray + activity → Task-6, Task-7 |
| T15 Inline diagnostics | **complete** | `diagnostics_consumers` wired |
| T16 Mouse dispatch | **partial** | Tab/tray/banner wired; review rows, side panel, overlay list, approvals gap → Task-5 |
| T17 Kitty image | **partial** | DCS probe + cache present; PTY e2e + LRU cache → Task-8, Task-9 |
| T18 Recorder + Replay | **complete** | JSONL writer/reader + event-loop tap + CLI flag wired |
| T19 Keybindings | **complete** | Loader + watcher + runtime wiring + startup load |
| §8 PR-1..9 | **complete** (except PR-3 subsumed) | PR-3 subsumed into commands/vil.rs + doctor → ADR-0001 (Task-3) |
| T5 closeout | **complete** | theme_loader + watcher + test |
| F-27 async path | **complete** (sync retained legacy) | `spawn_blocking` path added; legacy sync caller low-risk |

## Wave 4 Honest Status (2026-04-22)

**Formulation:** *Wave 4 foundation and product-surface wiring largely complete. Two items remain for final closeout: T16 mouse surface gap and T17 PTY e2e.*

| PR | Verdict | Closeout Task |
|----|---------|---------------|
| **T15** | `complete` | — |
| **T16** | `partial` | Task-5: expand dispatch_click to all interactive surfaces |
| **T17** | `partial` | Task-8: LRU image cache · Task-9: PTY e2e + sign-off |
| **T18** | `complete` | — |
| **T19** | `complete` | — |

## Per-PR Bar (2026-04-22)

| Gate | Status |
|------|--------|
| `cargo nextest run -p vac_tui_runtime --lib` | ✅ 315/315 |
| `cargo nextest run -p vac_cli --tests` | ✅ 81/81 |
| `scripts/check_sync_io.sh` | ✅ exit 0 |
| Files >600 LOC | ✅ 0 |
| Raw `Color::` outside theme | ✅ 0 |
| `block_in_place` in TUI handlers | ✅ 0 |

## Pending Closeout Tasks

| Task | Target | Description |
|------|--------|-------------|
| Task-4 | T12.1 | Wire vil-expr validator into input + render + Alt+H |
| Task-5 | T16 | Expand mouse dispatch_click to all interactive surfaces |
| Task-6 | T14 | Route VilDevEvent to task tray + activity log |
| Task-7 | T14 | Session timeline markers — decide Jalur A/B |
| Task-8 | T17 | LRU KittyImageCache |
| Task-9 | T17 | PTY/e2e positive path + final wave sign-off |

## Testing Policy
**NEVER use `cargo test`.** A PreToolUse hook blocks it. Always use:
```bash
cargo nextest run -p <crate> --lib           # all lib tests
cargo nextest run -p <crate> -E 'test(name)' # single test
cargo check --tests                          # compile-only check
```
