# TUI2 Implementation Notes

## Active Files

The following files are actively used in TUI2:

- `mod.rs` - Entry point
- `constants.rs` - UI constants
- `terminal.rs` - Terminal guard
- `stub_types.rs` - Type definitions replacing Stakpak dependencies
- `event.rs` - Crossterm event mapping
- `event_loop_minimal.rs` - Main event loop
- `view_minimal.rs` - Rendering logic
- `app/` - Application state and events
- `adapter/` - VacEngine adapter (for future integration)
- `services_minimal/` - Minimal UI services

## Disabled Files

The `services_stakpak_disabled/` directory contains 43 files transplanted from Stakpak that are NOT currently used. These are kept for reference when implementing advanced features:

- Markdown rendering
- Syntax highlighting
- Bash block rendering
- File diff rendering
- Side panel
- Plan review
- And more...

To activate these services:
1. Fix all `use stakpak_*` imports to use `stub_types`
2. Update `use crate::AppState` references
3. Enable `mod services;` in mod.rs
4. Replace `view_minimal.rs` with full `view.rs`

## VacEngine Integration

TUI2 currently uses a stub handler in `commands/tui2.rs`. To integrate with VacEngine:

1. Import `VacEngine` and `TuiPolicyEngine`
2. Initialize engine with policy
3. Handle `TaskEvent::ApprovalRequest` with oneshot channel
4. Replace stub handler with VacEngineAdapter

## License

Portions derived from Stakpak (https://github.com/stakpak/agent) under Apache 2.0.
See LICENSE_ATTRIBUTION.md for details.