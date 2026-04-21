# VAC TUI Parity Dashboard

## Metrics

| Metric | Target | Actual | Status |
|---|---|---|---|
| Files >600 lines | 0 (ideal), <=6 (gate) | 4 | ⚠️ |
| Raw `Color::` outside theme | 0 | ~187 | ❌ |
| Tests Green | >= 191 | 191+ | ✅ |
| Wave 2.5 PR Landed | 9/9 | 4/9 | ❌ |
| Wave 3 Landed | 7/7 | 0/7 | ❌ |
| Wave 4 Landed | 5/5 | 0/5 | ❌ |
| Runtime: `block_in_place` in TUI handlers | 0 | 1 | ⚠️ |

## Progress
- Async migration in `vac_session_control` has been completed.
- Build error (`shorten`) has been fixed and tests are compiling and passing successfully.
- File splitting and full theme sweep are in progress but require complex refactoring of imports and match statements.
- `vil_expr` skeleton (PR-2) created and aligned with the specification.

## Next Steps
- Implement §8 PR-3 (`VacConfig::vil`).
- Complete the file splitting for `runtime.rs`, `text_selection.rs`, `review.rs`, `ask_user.rs`.
- Finish replacing raw `Color::` usages with `StyleKey`.
- Proceed to PR-T12.
