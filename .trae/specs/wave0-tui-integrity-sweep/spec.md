# Wave 0 — TUI Integrity Sweep Spec

## Why
VAC's TUI has grown feature-rich but accumulated significant integrity debt, making it appear more mature than its actual behavior. There are key binding collisions, dead key bindings with no handlers, misleading shortcuts popup entries, phantom slash commands, passthrough commands labeled incorrectly, dispatch duplication between main input and command palette, and zero test coverage for these areas. Wave 0 closes this debt so future waves can build on a trustworthy foundation.

## What Changes
- Implement a context-aware keymap registry (`ALL_KEY_BINDINGS`) to resolve 3 key binding collisions (Ctrl+E, Ctrl+X, Ctrl+V).
- Reclassify 6 passthrough commands (`/vil`, `/swarm`, `/context`, `/rulebook`, `/resume`, `/help`) honestly and add missing handlers.
- Sweep and handle or explicitly remove 7 dead key bindings mapped to `InputEvent` variants with no handlers.
- Rewrite the shortcuts popup and footer to generate entries dynamically from the new key ensuring all 8 missing palette commands are available and eliminate duplication.
- Add exhaustive test coverage for keymap uniqueness, slash command dispatch, and event handler coverage.

## Impact
- Affected specs: TUI Interaction, Keymap Management, Command Dispatch
- Affected code: `crates/vac_cli/src/tui/event.rs`, `crates/vac_cli/src/tui/event_loop.rs`, `crates/vac_cli/src/tui/app/events.rs`, `crates/vac_cli/src/tui/app/types.rs`, `crates/vac_cli/src/tui/services/helper_block.rs`, `crates/vac_cli/src/tui/services/shortcuts_popup.rs`, `crates/vac_cli/src/tui/view.rs`

## ADDED Requirements
### Requirement: Context-Aware Keymap Registry
The system SHALL use a single declarative registry (`ALL_KEY_BINDINGS`) for key mappings, preventing duplicates within the same context.

#### Scenario: Key binding collision prevention
- **WHEN** a developer adds a new key binding that conflicts with an existing one in the same context
- **THEN** the automated test `no_duplicate_bindings_in_same_context` will fail.

### Requirement: Command Classification
The system SHALL accurately classify slash commands as `BuiltIn`, `Passthrough`, or `Custom`, and ensure all `BuiltIn` commands have active handlers.

#### Scenario: Executing a passthrough command
- **WHEN** a user executes `/vil`
- **THEN** the system explicitly sends it to the LLM as a `UserMessage` via the `Passthrough` variant.

### Requirement: Dynamic Shortcuts Popup
The system SHALL render the shortcuts popup and footer hints dynamically based on the registry.

#### Scenario: Viewing shortcuts
- **WHEN** the user opens the shortcuts popup
- **THEN** the popup only displays real bindings from `ALL_KEY_BINDINGS` and real commands from `vac_commands()`.

## MODIFIED Requirements
### Requirement: Command Palette Parity
The command palette SHALL share the exact same dispatch logic as the main input to ensure 100% command availability.

#### Scenario: Using command palette
- **WHEN** the user selects a command from the palette
- **THEN** it executes via the shared `dispatch_builtin_command` logic, identical to typing it in the input box.
