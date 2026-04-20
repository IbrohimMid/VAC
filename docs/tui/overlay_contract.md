# Overlay Contract

All modal overlays/popups go through `OverlayManager` (`overlay.rs`). This
document is the canonical list; if you add a popup, you must add it here.

## Lifecycle

- **Open**: call `overlay::open_overlay(state, OverlayId::X)`. This pushes
  onto the stack and, during the transitional period, also sets the legacy
  `show_X: bool`. Pushing a duplicate is a no-op (idempotent).
- **Close**: `overlay::close_overlay(state, OverlayId::X)`. Removes from
  the stack (wherever it is); if the stack drains to empty, restores
  `saved_focus`.
- **Close all**: `overlay::close_all_overlays(state)`.
- **Topmost**: `state.overlay_manager.topmost()` is the event-capturing
  overlay.
- **Any active**: `state.overlay_manager.any_active()`. This replaces
  the old `any_popup_active()` helper.

## Registered OverlayIds

After Patch 1.1 completes, these are the only overlays that exist. Any
modal input state that is not in this list is a bug.

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
| RejectReason *(new in 1.1)* | Reject key in Approvals | Workbench | pop (cancels reject) |
| ReviewPane *(new in 1.1)* | review tab visible w/ open item | Workbench | pop |

## Render order (bottom → top)

Defined by `RENDER_ORDER` in `overlay.rs`. Newer overlays are rendered on
top. `topmost()` matches the rightmost element of the stack, not the
render order.

## Focus restore

`saved_focus` captures `state.focus` at the moment the stack transitions
from empty → non-empty. It is restored (and cleared) when the stack
transitions back to empty via `pop` or `pop_all`. Individual overlay
handlers must not touch `state.focus` directly.

## Migration note

During Phase 1, `set_show_flag` keeps legacy `show_*` booleans in sync.
These booleans are removed in Phase 5.2.
