# TUI Interaction Model

Single source of truth for how input flows through the VAC TUI. Frozen during
the Phase 1–2 refactor; changes require updating this file + the related
`overlay_contract.md` and `action_matrix.md`.

## Principles

1. **One overlay stack** — `OverlayManager` in `overlay.rs`. No ad-hoc
   `show_*: bool` gating of input. Every modal popup has an `OverlayId`.
2. **One action spec** — every user-invocable intent is an `ActionSpec` in
   `action_registry.rs`. Keybindings, palette entries, footer hints, slash
   aliases, and mouse click targets all resolve through the registry.
3. **Linear render** — `view(state)` is a pure function over
   `(UiState, DerivedViewState)`. Render does not mutate domain state.
4. **Deterministic Esc** — Esc precedence is fixed and testable.

## Four-stage input router

`handle_input_event` routes in this strict order. The first stage that
consumes the event wins; subsequent stages are skipped.

```
InputEvent
    │
    ▼
(1) Overlay router
    if overlay_manager.topmost() is Some(id):
        dispatch to overlays::<id>::handle(...)
        return (always consumes)
    │
    ▼
(2) Workspace router
    match state.focus:
        Input      → workspace_input::handle
        Conversation → workspace_conversation::handle
        Activity   → workspace_activity::handle
        Workbench  → (3)
    │
    ▼
(3) Workbench router
    match state.workbench_tab:
        Approvals → tabs::approvals::handle
        Review    → tabs::review::handle
        Sessions  → tabs::sessions::handle
        Runtime   → tabs::runtime::handle
        Agents    → tabs::agents::handle
        Plan      → tabs::plan::handle
        Vil       → tabs::vil::handle
    │
    ▼
(4) Global fallback
    - Command palette trigger
    - Global shortcuts
    - Quit
```

## Esc precedence (single source of truth)

Evaluated top-down; first match wins.

1. `overlay_manager.any_active()` → `close_overlay(topmost)`.
2. `shell.is_popup_visible() && shell.command_running()` → background the shell.
3. `state.is_streaming` → emit `CancelStream`.
4. default: no-op.

## Action invocation contract

Every action reaches domain state through `ActionInvoker::invoke(id, ctx)`.
No handler may mutate `state.pending_approvals`, `state.review`, etc.
directly — go through the service layer owned by the registry entry.

## Out of scope (do not change during refactor)

- `ratatui` / `crossterm` versions
- Splitting `vac_tui_runtime` into multiple crates
- Adding new workbench tabs
