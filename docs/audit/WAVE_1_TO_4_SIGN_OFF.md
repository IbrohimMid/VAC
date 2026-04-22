# Wave 1–4 Sign-Off Record

**Last audited:** 2026-04-22  
**HEAD at sign-off:** `c10b7edb38910ea2b15b972430d960cca7a8894f`

---

## Wave Status

| Wave | Scope | Status |
|------|-------|--------|
| Wave 1 | Boot truthfulness, hydration gate, Color:: = 0, block_in_place = 0 | complete |
| Wave 2 | Mouse dispatch unification, capability registry | complete |
| Wave 2.5 | Structural: file LOC <600, dispatch split | complete |
| Wave 3 | VIL-native surfaces (VIL tab, VWFD inspector, activity log) | complete |
| Wave 4 | vil dev runner → task tray integration, Kitty LRU cache, PTY e2e tests | complete |

---

## Exit Criteria — Final State

| Criterion | Target | Result |
|-----------|--------|--------|
| `vac_tui_runtime --lib` test count | ≥315 | **324/324** ✅ |
| `vac_cli --tests` | ≥81 | **81/81** ✅ |
| `kitty_pty_e2e` integration tests | 3 PASS | **3/3** ✅ |
| `update_vil_dev_routing` integration tests | 4 PASS | **4/4** ✅ |
| Files >600 LOC | 0 | **0** ✅ (update.rs: 589) |
| `Color::` outside theme = 0 | 0 | **0** ✅ |
| `block_in_place` = 0 | 0 | **0** ✅ |
| `check_sync_io.sh` | exit 0 | **exit 0** ✅ |
| ADR files present | 2 | **2** ✅ |
| `clippy -p vil_context` | green | pre-existing 25 errors (see note) ⚠️ |

**Note on clippy:** 25 `unwrap_used` errors in `vil_context/src/shm.rs` are pre-existing at `af4c48d` (before Wave 4 work). They are not a regression from Wave 1–4. Tracked as Task-12 (optional, stand-alone).

---

## Evidence Map

| Artifact | Location |
|----------|----------|
| Gate mapping (Wave 1, 15 gates) | `docs/audit/WAVE1_GATE_MAPPING.md` |
| VIL bridge decision | `docs/adr/ADR-0001-vil-bridge-subsumed.md` |
| VWFD codegen inline templates | `docs/adr/ADR-0002-vwfd-codegen-inline-templates.md` |
| Kitty PTY e2e tests | `crates/vac_tui_runtime/tests/kitty_pty_e2e.rs` |
| vil dev routing tests | `crates/vac_tui_runtime/tests/update_vil_dev_routing.rs` |
| Product spec | `docs/PRODUCT_SPEC.md` |
| Per-feature PRDs | `docs/prd/` |

---

## Commit Chain

| Commit | Description |
|--------|-------------|
| `af4c48d` | fix(audit): harden phase A-F cleanup (Wave 4 base) |
| `de87377` | fix(trajectory): deterministic summaries and fallback |
| `c35383f` | feat(trajectory): add observe explain why |
| `3853f7b` | feat(ingest): wire project context through doctor and TUI |
| `762611b` | feat(vil): complete phase D gates |
| `7d47b55` | (Wave 1–4 Task batch — gate mapping, ADRs, mouse split, kitty, PTY e2e, vil dev routing) |
| `7801233` | docs(parity): sign off Wave 1-4 complete after closeout |
| `5d6c43b` | docs(product): replace plan docs with PRODUCT_SPEC and per-feature PRDs |
| `c10b7ed` | refactor(update): extract vil_dev routing tests to integration target (Task-10) |
| `HEAD` | docs(audit): restore Wave 1-4 sign-off and gate mapping records (Task-11) |
