# Tasks

## Phase 1: Product Shell Parity (Sprint 1)
- [x] Task 1.1: Clipboard Paste Integration (P0)
  - [x] Copy `clipboard_paste.rs` dari donor ke `services/`
  - [x] Add module declaration di `services.rs`
  - [x] Integrate paste handler ke event loop (`event_loop.rs`)
  - [x] Add keyboard shortcut (Ctrl+V) untuk trigger paste
  - [x] Handle image paste (save to temp, attach as base64)
  - [x] Handle text paste dengan file path extraction
  - [x] Testing: paste image, paste file paths, paste plain text
- [x] Task 1.2: Text Selection System (P1)
  - [x] Copy `text_selection.rs` dari donor ke `services/`
  - [x] Add module declaration di `services.rs`
  - [x] Add `SelectionState` ke `AppState` di `types.rs`
  - [x] Implement mouse selection handling di event loop
  - [x] Add copy-to-clipboard action untuk selected text
  - [x] Add visual highlight rendering di `view.rs`
  - [x] Integrate dengan message action popup
- [x] Task 1.3: Recent Commands & Better Palette Filtering (P1)
  - [x] Create `recent_commands.rs` service dan persist history
  - [x] Improve fuzzy matching `nucleo-matcher` di `helper_dropdown.rs`
  - [x] Add command frequency tracking
  - [x] Update `command_palette` rendering to show recents first
- [x] Task 1.4: Side Panel Architecture (P0/P1)
  - [x] Add click handler di event loop untuk header areas (toggle collapse)
  - [x] Render Sessions section di side panel
  - [x] Update `SidePanelSection` enum
  - [x] Wire up session selection shortcut
- [x] Task 1.5: Profile & Message UX (P1)
  - [x] Track model usage history di `AppState` & `model_switcher.rs`
  - [x] Expand message action popup (Copy code block, Retry, Revert, Explain)
  - [x] Copy `shortcuts_popup.rs` dari donor dan integrate dengan shortcuts system

## Phase 2: Agent CLI Parity (Sprint 2)
- [x] Task 2.1: Run Async Polish (P1)
  - [x] Improve autopilot status reporting & error recovery di `autopilot.rs` dan `run.rs`
- [x] Task 2.2: Isolation UX Parity (P1)
  - [x] Create default mount preset configurations di isolation switcher
  - [x] Auto-detect interactive vs batch mode untuk TTY convenience
- [x] Task 2.3: MCP Admin Surface (P0/P1)
  - [x] Add MCP server badges di header/side panel (color coding, trust level)
  - [x] Add warning indicator when server mode mismatch
- [x] Task 2.4: Shell/Operator Loop Polish (P1)
  - [x] Support multiple concurrent shell sessions & session switcher UI
  - [x] Better detection of interactive prompts & auto-forward input
  - [x] Capture shell output sebagai tool result & parse output
  - [x] Add visual transitions untuk shell state (starting, completed, failed)

## Phase 3: VIL-Native Superpowers (Sprint 3)
- [x] Task 3.1: VIL Project & IR Awareness (P0/P1)
  - [x] Show active rulebook name & semantic mode indicator di side panel
  - [x] Track IR generation state & show files with IR metadata
- [x] Task 3.2: VIL Review Workstation (P0/P1)
  - [x] Run validation pass on changeset (`vil_validate`) dan display findings di side panel
  - [x] Detect IR boundary violations & Zero-Copy risks
  - [x] Highlight generated vs handwritten code & show hints
- [x] Task 3.3: VIL-Aware Code Actions (P0/P1)
  - [x] Add "Repair VIL Contract" code action dengan auto-generate fixes
  - [x] Add "Explain Generated Plumbing", "Audit Zero-Copy", dan "Diff IR-Significant Change" actions
- [x] Task 3.4: VIL-Native Planning Policy (P1)
  - [x] Analyze AST/IR changes untuk classify Semantic vs Cosmetic
  - [x] Check changes against marked generated regions & show warning prompt

## Phase 4: Claude-Code-Class Polish (Sprint 4)
- [x] Task 4.1: Reliability & State Recovery (P0/P1)
  - [x] Enhance checkpoint serialization untuk pending operations
  - [x] Save shell state & restart background jobs on recovery
  - [x] Serialize & validate pending approvals on restore (Safer Pending Approval Restore)
- [x] Task 4.2: Performance Optimization (P1)
  - [x] Implement per-message cache invalidation & LRU cache untuk rendering
  - [x] Add incremental search & background indexing untuk Large Repo File Search
- [x] Task 4.3: Defaults & Ergonomics (P1)
  - [x] Define default profiles, isolation presets, dan model defaults
  - [x] Enhance `vac doctor` dengan interactive setup wizard & tutorial mode
  - [x] Check environment on startup & show warnings di toast/banner

# Task Dependencies
- [Task 1.2] depends on [Task 1.1]
- [Task 2.4] depends on [Task 2.1]
- [Task 3.2] depends on [Task 3.1]
- [Task 3.3] depends on [Task 3.2]
- [Task 4.1] depends on Phase 1 & 2 completion for shell states
