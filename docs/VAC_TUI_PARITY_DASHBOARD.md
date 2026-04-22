# VAC TUI Parity Dashboard

**Last audited:** 2026-04-22 HEAD `5fda989` (post Task-9 closeout)

## Metrics

| Metric | Target | Actual | Status |
|---|---|---|---|
| Files >600 lines | 0 (ideal), <=6 (gate) | 0 | ✅ |
| Raw `Color::` outside theme | 0 | 0 (audited 2026-04-22) | ✅ |
| Tests Green (lib) | >= 315 | 328/328 | ✅ |
| Tests Green (integration) | >= 81 | 81/81 | ✅ |
| Tests Green (e2e kitty) | >= 3 | 3/3 | ✅ |
| Wave 1 | complete | all 15 gates PASS — `docs/audit/WAVE1_GATE_MAPPING.md` | ✅ |
| Wave 2 | complete | T5–T10 landed + theme sweep complete | ✅ |
| Wave 2.5 | complete | files>600=0, Color::=0, block_in_place=0, check_sync_io=0 | ✅ |
| Wave 3 | complete | T11–T15 complete; T12.1 verified; T14 activity routing done | ✅ |
| Wave 4 | complete | T15–T19 complete; T16 surfaces expanded; T17 cache+e2e done | ✅ |
| Runtime: `block_in_place` in TUI handlers | 0 | 0 | ✅ |

## Item Status (calibrated to HEAD 2026-04-22)

| Item | Status | Notes |
|------|--------|-------|
| Wave 1 | **complete** | 15/15 gates PASS — `docs/audit/WAVE1_GATE_MAPPING.md` |
| Wave 2 | **complete** | T5–T10 + theme TOML + hot-reload |
| Wave 2.5 | **complete** | All W25-1..9 done; metrics all green |
| T11 VWFD inspector | **complete** | `VwfdInspectorState` wired, service+handlers |
| T12 vil-expr validator | **complete** | `services/vil_expr_lint.rs` + 6 tests |
| T12.1 UI wiring | **complete** | 3 tests verified PASS in `handlers/workspace_input.rs` |
| T13 VIL-aware diff overlay | **complete** | `vwfd_diff_render` delegation |
| T14 Background vil dev | **complete** | Runner events routed to task tray + activity (Task-6); timeline reframed (Task-7) |
| T15 Inline diagnostics | **complete** | `diagnostics_consumers` wired |
| T16 Mouse dispatch | **complete** | All surfaces covered incl. side panel header toggle (Task-5); 5 new tests |
| T17 Kitty image | **complete** | DCS probe + LRU cache (Task-8) + PTY e2e tests (Task-9) |
| T18 Recorder + Replay | **complete** | JSONL writer/reader + event-loop tap + CLI flag |
| T19 Keybindings | **complete** | Loader + watcher + runtime wiring + startup load |
| §8 ADR-0001 | **complete** | vil_bridge subsume documented |
| §8 ADR-0002 | **complete** | inline templates documented |

## Per-PR Bar (2026-04-22 final)

| Gate | Status |
|------|--------|
| `cargo nextest run -p vac_tui_runtime --lib` | ✅ 328/328 |
| `cargo nextest run -p vac_tui_runtime --test kitty_pty_e2e` | ✅ 3/3 |
| `cargo nextest run -p vac_cli --tests` | ✅ 81/81 |
| `scripts/check_sync_io.sh` | ✅ exit 0 |
| `cargo clippy --workspace -- -D warnings` | ✅ |
| Files >600 LOC | ✅ 0 |
| Raw `Color::` outside theme | ✅ 0 |
| `block_in_place` in TUI handlers | ✅ 0 |
| `docs/audit/WAVE1_GATE_MAPPING.md` present | ✅ |
| 2 ADRs present | ✅ ADR-0001, ADR-0002 |

## Testing Policy
**NEVER use `cargo test`.** A PreToolUse hook blocks it. Always use:
```bash
cargo nextest run -p <crate> --lib           # all lib tests
cargo nextest run -p <crate> -E 'test(name)' # single test
cargo nextest run -p vac_tui_runtime --test kitty_pty_e2e  # e2e
cargo check --tests                          # compile-only check
```
