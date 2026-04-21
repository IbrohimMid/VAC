# VAC TUI Parity Dashboard

## Metrics

| Metric | Target | Actual | Status |
|---|---|---|---|
| Files >600 lines | 0 (ideal), <=6 (gate) | 4 | ⚠️ |
| Raw `Color::` outside theme | 0 | ~187 | ❌ |
| Tests Green | >= 191 | 191+ | ✅ |
| Wave 2.5 PR Landed | 9/9 | 4/9 | ❌ |
| Wave 3 Landed | 7/7 | 1/7 (T12) | ⚠️ |
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
- Wire `AppState::vil_expr_lint.on_input_changed()` into `handlers/input.rs` on `InputChanged` and call `.tick()` from the event loop tick path (DEFERRED in PR-T12 to keep the PR scoped to the service layer).
- Add Alt+H hover popup that renders the inferred type / current issues above the composer (DEFERRED in PR-T12).
- Render `AppState::vil_expr_lint.issues()` under the composer using `StyleKey::ValidationError/Warning/Ok` (DEFERRED in PR-T12).
- Complete the file splitting for `runtime.rs`, `text_selection.rs`, `review.rs`, `ask_user.rs`.
- Finish replacing raw `Color::` usages with `StyleKey`.
