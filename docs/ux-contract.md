# VAC UX Contract

Enforced rules for the operator cockpit. Each rule is paired with a
test that fails if the rule is broken, so a future refactor that
violates the contract must delete the test — making the UX regression
explicit in review.

## Rules

### 1. At most one banner visible at a time

Queued banners do not render stacked. Dismissing the current banner
advances the queue. Dismissed banners cannot re-enter.

- Rationale: banner nesting hurts scannability; operators should always
  know which message is acting on the current keystroke.
- Enforcing tests: `services::banner::tests::contract_queue_deduplicates_by_id`,
  `contract_only_first_banner_is_current`,
  `contract_dismiss_advances_queue_without_losing_others`,
  `contract_dismissed_banner_cannot_be_re_pushed` — `crates/vac_tui_runtime/src/services/banner.rs`.

### 2. Overlay stack depth ≤ 2

Opening a third modal overlay is refused (with a tracing `warn!`).
Deep nesting makes it unclear which keystroke acts on which modal.

- Constant: `OverlayManager::MAX_STACK_DEPTH = 2`
- Enforcing tests: `overlay::tests::contract_stack_depth_capped_at_max`,
  `contract_push_is_idempotent`, `contract_topmost_is_last_pushed`,
  `contract_pop_from_middle_removes_only_that_overlay`,
  `contract_saved_focus_only_restored_when_stack_empties`,
  `contract_render_order_follows_canonical_not_push_order`
  — `crates/vac_tui_runtime/src/overlay.rs`.

### 3. Primary approval action completes in one keypress

Pressing `a` with an approval focused moves the tool from
`pending_approvals` to `approved_tools` in a single handler call — no
confirmation modal, no two-step prompt.

- Enforcing tests: `handlers::approval::tests::contract_approve_current_is_single_step`,
  `contract_approve_on_empty_queue_is_noop`,
  `contract_approve_all_clears_queue_in_single_call`
  — `crates/vac_tui_runtime/src/handlers/approval.rs`.

### 4. Every workbench action reachable via keyboard

No primary action requires a mouse. Navigation works via delta
integers the keyboard handler feeds the state.

- Enforcing tests: `review_keyboard::*` (5 tests) —
  `crates/vac_tui_runtime/tests/review_keyboard.rs`.

### 5. Primary actions stay sub-millisecond

Regression guard: review navigation, user-message append, and activity
push stay in micro-to-low-millisecond range at representative loop
sizes. Fails loudly if a future change introduces disk I/O or blocking
work on a hot path.

- Enforcing tests: `primary_action_latency::*` (3 tests) —
  `crates/vac_tui_runtime/tests/primary_action_latency.rs`.

### 6. Boot path stays under 150ms

`AppState::default()` + `VacConfig::default()` construction must
complete under 150ms to preserve first-paint latency budget.

- Enforcing test: `boot_latency::boot_constructors_complete_under_150ms`
  — `crates/vac_tui_runtime/tests/boot_latency.rs`.

## What this contract does NOT enforce (yet)

- First-paint latency in an actual terminal (requires a TTY harness).
- Approval rejection prompt has its own modal — intentional
  (reject reason prompt is deliberate friction to avoid
  hitting `r` and losing a tool call silently).
- Diff viewer rendering fidelity (unit-tested separately).

## Adding a new rule

1. Write the rule here with its rationale.
2. Add a contract test in the most specific test file.
3. Name the test `contract_<behavior>` so it shows up in the
   `cargo nextest run -E 'test(contract_)'` filter.
