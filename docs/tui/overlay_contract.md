# Overlay Contract

All modal overlays/popups go through `OverlayManager` (`overlay.rs`). This
document is the canonical list; if you add a popup, you must add it here.

## Lifecycle

- **Open**: call `overlay::open_overlay(state, OverlayId::X)`. This pushes
  onto the stack and, where applicable, syncs the domain-state field the
  overlay backs (see *Domain-state map* below). Pushing a duplicate is a
  no-op (idempotent).
- **Close**: `overlay::close_overlay(state, OverlayId::X)`. Removes from
  the stack (wherever it is); if the stack drains to empty, restores
  `saved_focus`.
- **Close all**: `overlay::close_all_overlays(state)`.
- **Topmost**: `state.overlay_manager.topmost()` is the event-capturing
  overlay.
- **Any active**: `state.overlay_manager.any_active()`. This replaces
  the old `any_popup_active()` helper.

## Registered OverlayIds

These are the only overlays that exist. Any modal input state that is not
in this list is a bug. The table below mirrors the `OverlayId` enum in
`overlay.rs` one-for-one; if you add or remove a variant there, update
this table in the same commit.

| OverlayId | Trigger | Scope | Esc behaviour |
| --- | --- | --- | --- |
| CommandPalette | Ctrl+P | Global | pop |
| Shortcuts | `?` | Global | pop |
| IsolationSwitcher | palette/slash | Global | pop |
| ProfileSwitcher | palette/slash | Global | pop |
| RulebookSwitcher | palette/slash | Global | pop |
| ModelSwitcher | palette/slash | Global | pop |
| FileSearch | Ctrl+F | Input/Workbench | pop |
| Changeset | palette/slash | Workbench | pop |
| FileChanges | side panel click | Workbench | pop |
| PlanReview | plan tab action | Workbench | pop |
| AskUser | runtime emits AskUser | Any | pop (submits "cancelled") |
| ShellPopup | `!` / shell trigger | Input | pop (or background if running) |
| MessageAction | message context menu | Conversation | pop |
| HelperDropdown | `/` typed in Input | Input | pop |
| AtDropdown | `@` typed in Input | Input | pop |
| RejectReason | Reject key in Approvals | Workbench | pop (cancels reject) |
| ReviewPane | review tab visible w/ open item | Workbench | pop |
| TaskTray | background job created / tray toggle | Global | pop |
| ThemePicker | Ctrl+Shift+T | Global | pop |
| SessionResume | Ctrl+R | Global | pop |
| FilePicker | palette/slash / file action | Any | pop |

## Render order (bottom → top)

Defined by `RENDER_ORDER` in `overlay.rs`. Newer overlays are rendered on
top. `topmost()` matches the rightmost element of the stack, not the
render order.

## Focus restore

`saved_focus` captures `state.focus` at the moment the stack transitions
from empty → non-empty. It is restored (and cleared) when the stack
transitions back to empty via `pop` or `pop_all`. Individual overlay
handlers must not touch `state.focus` directly.

## Domain-state map

Most overlays carry no state beyond `OverlayManager.stack`. A handful
back a domain field that other subsystems read directly; when the
overlay opens/closes, `sync_domain_state` in `overlay.rs` keeps that
field in agreement. If you add a new overlay whose lifecycle mirrors a
domain bool/struct, extend that match — do **not** plumb a parallel
`show_*` flag.

| OverlayId | Backing field |
| --- | --- |
| PlanReview | `state.plan.review_open` |
| ShellPopup | `state.shell.session_store.popup_visible` |
| AtDropdown | `state.at_trigger_active` (+ clears `at_query`/`at_results`/`at_selected_idx` on close) |
| RejectReason | `state.reject_reason_input` (set `Some(String::new())` on open, `None` on close) |
| ReviewPane | `state.review.open` |

All other `OverlayId` variants rely exclusively on the stack; their
handlers read `overlay_manager.is_active(id)` instead of a bool field.
The legacy `show_*: bool` pattern has been fully removed from
`crates/vac_tui_runtime/` — a `rg 'show_[a-z]+:\s*bool'` sweep must stay
at zero hits, which the `action_spec_coverage` test guards indirectly by
requiring every modal input route through `OverlayManager`.
