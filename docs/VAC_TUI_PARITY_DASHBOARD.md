# VAC TUI Parity Dashboard

## Metrics

| Metric | Target | Actual | Status |
|---|---|---|---|
| Files >600 lines | 0 (ideal), <=6 (gate) | 4 | ⚠️ |
| Raw `Color::` outside theme | 0 | 303 (audited 2026-04-21) | ❌ |
| Tests Green | >= 191 | 191+ | ✅ |
| Wave 2.5 PR Landed | 9/9 | 6/9 (W25-2,3,4,5,6,8) | ⚠️ |
| Wave 3 Landed | 7/7 | 2/7 (T12, T12.1) | ⚠️ |
| Wave 4 Landed | 5/5 | 0/5 | ❌ |
| Runtime: `block_in_place` in TUI handlers | 0 | 1 | ⚠️ |

## Progress
- Async migration in `vac_session_control` has been completed.
- Build error (`shorten`) has been fixed and tests are compiling and passing successfully.
- File splitting and full theme sweep are in progress but require complex refactoring of imports and match statements.
- `vil_expr` skeleton (PR-2) created and aligned with the specification.
- §8 PR-3 (`VacConfig::vil`) landed.
- **PR-T12 landed**: `services/vil_expr_lint.rs` with `LintState` + debounced `parse` → `validate` pipeline; `StyleKey::Validation{Error,Warning,Ok}` added to all three palettes; `AppState::vil_expr_lint` field wired. 6 unit tests green locally (incl. required `lint_detects_unknown_identifier`, `lint_ok_on_valid_expression`, `lint_debounces_rapid_typing`).

## Next Steps
- ~~Wire `AppState::vil_expr_lint.on_input_changed()` into input handler~~ ✅ **Done in PR-T12.1** (`2a2e6ad`)
- ~~Render `AppState::vil_expr_lint.issues()` under the composer~~ ✅ **Done in PR-T12.1**
- ~~Alt+H hover popup~~ → Changed to **Alt+T** (Alt+H was taken by InputDeleteWord); stub toast in PR-T12.1, full type inference deferred to Wave 4
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
