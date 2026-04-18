# Tasks

- [ ] Task 1: PR 1 - TUI Keymap Registry and Collision Fix
  - [ ] SubTask 1.1: Restructure `crates/vac_cli/src/tui/event.rs` as a declarative registry (`KeymapContext`, `KeyBinding`, `ALL_KEY_BINDINGS`).
  - [ ] SubTask 1.2: Rewrite `map_crossterm_event_to_input_event` to accept `KeymapContext`.
  - [ ] SubTask 1.3: Fix collisions for Ctrl+E, Ctrl+X, Ctrl+V in `event.rs`.
  - [ ] SubTask 1.4: Update `event_loop.rs` to map crossterm events with context inside the loop.
  - [ ] SubTask 1.5: Add `no_duplicate_bindings_in_same_context` test in `event.rs`.

- [ ] Task 2: PR 2 - TUI Command Classification and Passthrough Fix
  - [ ] SubTask 2.1: Add `Passthrough` variant to `CommandSource` in `crates/vac_cli/src/tui/app/types.rs`.
  - [ ] SubTask 2.2: Reclassify `/vil`, `/swarm`, `/context` to `Passthrough` in `helper_block.rs`.
  - [ ] SubTask 2.3: Add handlers for `/rulebook`, `/help`, `/resume`, `/agents` in `event_loop.rs`.
  - [ ] SubTask 2.4: Update command dispatch in `event_loop.rs` to handle the `Passthrough` variant.
  - [ ] SubTask 2.5: Add `builtin_commands_must_have_handlers` test in `helper_block.rs`.

- [ ] Task 3: PR 3 - TUI Dead Event Sweep
  - [ ] SubTask 3.1: Add `collapsed_messages` to `AppState` in `types.rs`.
  - [ ] SubTask 3.2: Add match arms in `event_loop.rs` for `HandleCtrlS`, `ToggleCollapsedMessages`, `ToggleMouseCapture`, `RetryLastToolCall`, `RulebookSwitcherDeselectAll`, and `Resized`.
  - [ ] SubTask 3.3: Remove or alias `Quit` in `events.rs`.
  - [ ] SubTask 3.4: Add `input_event_coverage_check` test for exhaustive match coverage.

- [ ] Task 4: PR 4 - TUI Shortcuts Popup From Registry
  - [ ] SubTask 4.1: Rewrite `get_all_shortcuts` and `get_all_commands` in `shortcuts_popup.rs` to read from `ALL_KEY_BINDINGS` and `vac_commands()`.
  - [ ] SubTask 4.2: Remove phantom commands and fix shortcut keys in `shortcuts_popup.rs`.
  - [ ] SubTask 4.3: Update `render_shortcuts` in `view.rs` to use dynamic shortcuts.
  - [ ] SubTask 4.4: Update `welcome_messages` in `helper_block.rs` to use registry.
  - [ ] SubTask 4.5: Add `shortcuts_popup_only_contains_real_bindings` test.

- [ ] Task 5: PR 5 - TUI Palette Parity and Shared Dispatch
  - [ ] SubTask 5.1: Extract `dispatch_builtin_command` in `event_loop.rs`.
  - [ ] SubTask 5.2: Update both `InputSubmitted` and `CommandPaletteSelect` to use the shared dispatch.
  - [ ] SubTask 5.3: Add palette handling for `/file-changes`, `/plan`, `/plan-review`, `/plan-edit`.
  - [ ] SubTask 5.4: Add `all_builtin_commands_available_in_palette` test.

# Task Dependencies
- Task 3 depends on Task 1
- Task 4 depends on Task 1 and Task 2
- Task 5 depends on Task 2
