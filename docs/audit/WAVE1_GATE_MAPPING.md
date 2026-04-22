# Wave 1 Gate Mapping — Acceptance Criteria vs Current Metrics

**Audited:** 2026-04-22 HEAD `af4c48d`  
**Auditor:** agent (automated + manual trace)

---

## Gate Table

| Gate ID | Plan Criterion | Current Metric | Evidence Path | Status |
|---------|---------------|----------------|---------------|--------|
| W1-G1 | Boot truthfulness: `state.hydrated` gate — first render blocked until hydrated | `AppState.hydrated: bool` field; `view/mod.rs:26` blocks render; deadline timeout at 10s | `src/app/types/mod.rs:53,55` · `src/event_loop.rs:352-353` · `src/view/mod.rs:26-27` | **PASS** |
| W1-G2 | `render_boot_skeleton` shown while hydrating | `render_boot_skeleton` in `view/popups.rs:214` called from `view/mod.rs:27` | `src/view/popups.rs:214` · `src/view/mod.rs:27` | **PASS** |
| W1-G3 | Hydration deadline timeout prevents infinite loading state | `hydration_deadline = now + 10s`; event_loop force-sets `hydrated=true` on deadline | `src/app/types/helpers.rs:42-43` · `src/event_loop.rs:352-353` | **PASS** |
| W1-G4 | Zero `vunknown` / `Model: none` rendered in UI | Test assertions in `view/mod.rs:355-358` guard against both strings; `grep` in `src/` finds only test assertions, not render paths | `src/view/mod.rs:355-358` | **PASS** |
| W1-G5 | Single ActionSpec dispatch path via `spec_by_slash_alias` | `input_commands.rs:139` resolves slash via `spec_by_slash_alias`; `dispatch_action` is single call-site | `src/action_ids.rs:183` · `src/handlers/input_commands.rs:139-140` | **PASS** |
| W1-G6 | Unknown slash: 3-tier fuzzy suggestions (prefix → subsequence → Levenshtein ≤ 2) | `spec_by_slash_alias` fallback path with fuzzy match in `action_registry.rs` | `src/action_ids.rs:183` · `src/action_registry.rs:7` | **PASS** |
| W1-G7 | Capability registry live (`capabilities.rs`, shim in `detect_term.rs`) | `pub mod capabilities` in `lib.rs:46`; `detect_term.rs:9` delegates to `TerminalCapabilities::detect()` | `src/lib.rs:46` · `src/services/detect_term.rs:8-9` | **PASS** |
| W1-G8 | Controller files < 600 lines (hard gate) | Max file in `src/`: `action_registry.rs` at 587 LOC; all others ≤ 570 | `wc -l` scan 2026-04-22: 0 files > 600 | **PASS** |
| W1-G9 | `block_in_place` = 0 in TUI handlers | `grep -rn block_in_place crates/vac_tui_runtime/src/` = 0 results | `grep` scan 2026-04-22 | **PASS** |
| W1-G10 | Raw `Color::` outside theme files = 0 | `grep -rn 'Color::' … | grep -v theme` = 0 results | `grep` scan 2026-04-22 | **PASS** |
| W1-G11 | `event_loop_tests` split into per-feature modules | `src/event_loop_tests/` exists with `session.rs`, `approval.rs`, `shell.rs`, `runtime.rs`, `runtime/` subdir | `src/event_loop_tests/` directory listing 2026-04-22 | **PASS** |
| W1-G12 | `runner.rs` < 600 LOC | `runner.rs` = 427 LOC | `wc -l src/runner.rs` 2026-04-22 | **PASS** |
| W1-G13 | `cargo nextest run -p vac_tui_runtime --lib` ≥ 315 PASS | 315/315 PASS, 0 FAIL | nextest run 2026-04-22 | **PASS** |
| W1-G14 | `cargo nextest run -p vac_cli --tests` ≥ 81 PASS | 81/81 PASS, 0 FAIL | nextest run 2026-04-22 | **PASS** |
| W1-G15 | `scripts/check_sync_io.sh` exit 0 | Script exits 0; no sync I/O violations in hot paths | `check_sync_io.sh` 2026-04-22 | **PASS** |

---

## Summary

**Wave 1: COMPLETE**

All 15 acceptance gates pass against HEAD `af4c48d`. Key metrics:

- Files > 600 LOC: **0** (max = 587, `action_registry.rs`)
- Raw `Color::` outside theme: **0** (theme sweep complete)
- `block_in_place` in TUI handlers: **0**
- Test suite: **315/315** lib · **81/81** integration
- Boot skeleton, hydration gate, dispatch unification, capability registry: all wired and verified

No Wave 1 blockers remain.
