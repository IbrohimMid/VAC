# VAC TUI Parity Dashboard

## Metrics

| Metric | Target | Actual | Status |
|---|---|---|---|
| Files >600 lines | 0 (ideal), <=6 (gate) | 4 | ⚠️ |
| Raw `Color::` outside theme | 0 | 303 (audited 2026-04-21) | ❌ |
| Tests Green | >= 191 | 191+ | ✅ |
| Wave 2.5 PR Landed | 9/9 | 6/9 (W25-2,3,4,5,6,8) | ⚠️ |
| Wave 3 Landed | 7/7 | 2/7 (T12, T12.1) | ⚠️ |
| Wave 4 Feature-Complete | 5/5 | **0/5** (all PARTIAL — integration pending) | ❌ |
| Wave 4 Foundation Landed | 5/5 | 5/5 (helpers/loaders/probes) | ⚠️ |
| Per-PR Bar proof for T15–T19 | Pass | Not yet verified | ⚠️ |
| Runtime: `block_in_place` in TUI handlers | 0 | 1 | ⚠️ |

## Wave 4 Honest Status (2026-04-21 audit)

**Formulation:** *Wave 4 core support modules landed (T15–T19), but 4/5 items remain integration-incomplete and therefore Wave 4 is not yet complete against plan acceptance.*

Short form: **Wave 4 helper layer complete; product-surface completion still pending.**

| PR | Plan target (Wave 4) | Landed (foundation) | Missing (product surface) | Verdict |
|----|----------------------|---------------------|---------------------------|---------|
| **T15** | Inline diagnostics (LSP-style, reusing `vil_validate`) | `services/diagnostics_overlay.rs` helper + per-file cache + spans | Renderer integration in Review + vil_workbench | `PARTIAL` |
| **T16** | Mouse click → action dispatch **complete** (Phase 6.5 closure) | `mouse::dispatch_click`, wired: tab / task tray / banner dismiss | Review rows, side panel rows, workbench panel body, overlay list rows, vil_workbench editor, approvals pane | `PARTIAL` |
| **T17** | Kitty image protocol **support** (preview images inline) | `services/kitty_image.rs` DCS probe + timeout + ASCII fallback | Startup call, `TerminalCapabilities` field, render-path consumption | `PARTIAL` |
| **T18** | Command recorder / replay for demos | `services/recorder.rs` JSONL writer/reader, rotate 10MB × keep 20 | Event-loop tap, `vac tui --replay <file>` CLI flag, replay driver | `PARTIAL` (near-blocker) |
| **T19** | Custom keybinding user config at `.vac/keybindings.toml` | `services/keybindings_loader.rs` TOML loader + `resolve_effective` + warning-on-error | Wire into `handle_input_event` matcher / `ActionSpec` override, startup load, surface warnings | `PARTIAL` |

### Per-PR Bar — proof status for T15–T19

| Gate | Status |
|------|--------|
| `cargo nextest run -p vac_tui_runtime` | ✅ (passing locally) |
| `cargo test -p vac_cli --test integration_events` | ⚠️ Not run for these PRs |
| `insta` snapshot for new overlay/panel | ❌ No new snapshots added |
| `docs/tui/action_matrix.md` update (T16, T19 touch ActionSpec) | ❌ Not updated |
| `scripts/check_sync_io.sh` no regression | ⚠️ Not verified |
| ≥1 E2E via TUI harness | ❌ Not added |

## Progress
- Async migration in `vac_session_control` has been completed.
- Build error (`shorten`) has been fixed and tests are compiling and passing successfully.
- File splitting and full theme sweep are in progress but require complex refactoring of imports and match statements.
- `vil_expr` skeleton (PR-2) created and aligned with the specification.
- §8 PR-3 (`VacConfig::vil`) landed.
- **PR-T12 landed**: `services/vil_expr_lint.rs` with `LintState` + debounced `parse` → `validate` pipeline; `StyleKey::Validation{Error,Warning,Ok}` added to all three palettes; `AppState::vil_expr_lint` field wired. 6 unit tests green locally (incl. required `lint_detects_unknown_identifier`, `lint_ok_on_valid_expression`, `lint_debounces_rapid_typing`).
- **Wave 4 foundation pushed to `origin/main` (`636e892`) — commits T15–T19 reviewable, but not claiming feature-complete.**

## Next Steps — Wave 4 integration (to earn `PASS`)

### P0 (unblocks "feature exists" claim)
1. **T19 runtime wiring** — loader → merged keymap → matcher / `ActionSpec` override in `handle_input_event`; load at startup; surface errors in banner.
2. **T18 event-loop tap + CLI replay** — hook recorder in input pipeline; add `vac tui --replay <file>` in `vac_cli`; replay driver feeds `RecordedInput` back through event loop.
3. **T17 startup wire-up** — call DCS probe at startup, populate `TerminalCapabilities.kitty_graphics`, gate image-render path on it.

### P1 (closes "complete" qualifier in plan wording)
4. **T15 renderer integration** — render `DiagnosticsOverlay` spans inline under Review rows + vil_workbench editor gutter.
5. **T16 coverage completion** — enumerate actionable surfaces, close each; target: mouse dispatch consistent across majority of clickable UI.

### P2 (Per-PR Bar proof)
6. Run & capture: `vac_cli --test integration_events`, add `insta` snapshots for new overlays/panels, update `docs/tui/action_matrix.md`, verify `scripts/check_sync_io.sh`, add ≥1 E2E harness test per wired surface.

## Other outstanding work
- W25-1: Theme sweep — 303 raw `Color::` in services/ + workbench/ (baseline audited)
- W25-7: event_loop_tests split — structurally done, `runtime.rs` at 804 LOC (eligible for further split)
- W25-9: Split `runner.rs` + finish async migration (1 `block_in_place` remaining)
- Complete the file splitting for `text_selection.rs`, `review.rs`, `ask_user.rs`

## Testing Policy
**NEVER use `cargo test`.** A PreToolUse hook blocks it. Always use:
```bash
cargo nextest run -p <crate> --lib           # all lib tests
cargo nextest run -p <crate> -E 'test(name)' # single test
cargo check --tests                          # compile-only check
```
