# 📋 VAC Implementation Plan - Complete Roadmap

## Executive Summary

This document provides a comprehensive implementation plan for completing all remaining phases of VAC (Vastar Agentic CLI) development. The plan covers:
- **Phase 1**: Product Shell Parity (~85% complete → 100%)
- **Phase 2**: Agent CLI Parity (~60% complete → 100%)
- **Phase 3**: VIL-Native Superpowers (~40% complete → 100%)
- **Phase 4**: Claude-Code-Class Polish (~20% complete → 100%)

**Total Estimated Effort**: ~15-20 days of focused development

---

## Phase 1 — Product Shell Parity (Target: 100% Complete)

### Current Status: ~85% Complete

### Workstream 1.1 — Helper & Input Ergonomics

#### 1.1.1 Clipboard Paste Integration (P0)
**Status**: ❌ BELUM ADA  
**Donor Available**: ✅ `services_stakpak_disabled/clipboard_paste.rs` (18KB)  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Copy `clipboard_paste.rs` dari donor ke `services/`
2. [ ] Add module declaration di `services.rs`
3. [ ] Integrate paste handler ke event loop (`event_loop.rs`)
4. [ ] Add keyboard shortcut (Ctrl+V) untuk trigger paste
5. [ ] Handle image paste (save to temp, attach as base64)
6. [ ] Handle text paste dengan file path extraction
7. [ ] Testing: paste image, paste file paths, paste plain text

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/clipboard_paste.rs` (create)
- `/workspace/crates/vac_cli/src/tui/services.rs` (add mod)
- `/workspace/crates/vac_cli/src/tui/event_loop.rs` (add handler)

**Dependencies**: 
- `arboard` crate (sudah ada di Cargo.toml)
- `image` crate (sudah ada)
- `tempfile` crate (perlu ditambahkan)

---

#### 1.1.2 Text Selection System (P0)
**Status**: ❌ BELUM ADA  
**Donor Available**: ✅ `services_stakpak_disabled/text_selection.rs` (23KB)  
**Effort**: 6-8 jam  

**Tasks**:
1. [ ] Copy `text_selection.rs` dari donor ke `services/`
2. [ ] Add module declaration di `services.rs`
3. [ ] Add `SelectionState` ke `AppState` di `types.rs`
4. [ ] Implement mouse selection handling di event loop
5. [ ] Add copy-to-clipboard action untuk selected text
6. [ ] Add visual highlight rendering di `view.rs`
7. [ ] Integrate dengan message action popup

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/text_selection.rs` (create)
- `/workspace/crates/vac_cli/src/tui/services.rs` (add mod)
- `/workspace/crates/vac_cli/src/tui/app/types.rs` (add state)
- `/workspace/crates/vac_cli/src/tui/event_loop.rs` (mouse handling)
- `/workspace/crates/vac_cli/src/tui/view.rs` (highlight rendering)

---

#### 1.1.3 Recent Commands / Pinned Commands (P1)
**Status**: ❌ BELUM ADA  
**Donor Available**: ⚠️ Partial (`custom_commands.rs` di donor)  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Create `recent_commands.rs` service
2. [ ] Track command usage history di `AppState`
3. [ ] Implement pinning mechanism
4. [ ] Show recent commands di helper dropdown
5. [ ] Persist history ke file (~/.vac/recent_commands.json)
6. [ ] Add quick-access shortcut

**Files to Create**:
- `/workspace/crates/vac_cli/src/tui/services/recent_commands.rs`

---

#### 1.1.4 Better Command Palette Filtering (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 2-3 jam  

**Tasks**:
1. [ ] Improve fuzzy matching menggunakan `nucleo-matcher` (sudah ada)
2. [ ] Add category filtering (built-in vs custom)
3. [ ] Add keyboard navigation enhancement
4. [ ] Show command usage frequency

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/helper_dropdown.rs`
- `/workspace/crates/vac_cli/src/tui/event_loop.rs`

---

### Workstream 1.2 — Side Panel Architecture

#### 1.2.1 Clickable Section Headers (P0)
**Status**: ❌ BELUM ADA  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Store header areas di `AppState` (sudah ada `side_panel_header_areas`)
2. [ ] Add click handler di event loop untuk header areas
3. [ ] Toggle collapse on click
4. [ ] Add hover effect visual feedback

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/event_loop.rs` (add mouse click handler)
- `/workspace/crates/vac_cli/src/tui/view.rs` (add hover styling)

---

#### 1.2.2 Sessions Section (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Add `Sessions` variant ke `SidePanelSection` enum
2. [ ] Create `render_sessions_section()` function
3. [ ] Show active session count
4. [ ] Show recent sessions (last 3)
5. [ ] Quick switch to session on click

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/app/types.rs` (add enum variant)
- `/workspace/crates/vac_cli/src/tui/services/side_panel.rs` (add render function)

---

#### 1.2.3 Billing Section (P2 - Optional)
**Status**: ❌ BELUM ADA  
**Effort**: 2-3 jam  

**Note**: Low priority, hanya jika ada billing integration

---

### Workstream 1.3 — Profile & Rulebook UX

#### 1.3.1 Recent Model History (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 2-3 jam  

**Tasks**:
1. [ ] Track model usage history di `AppState`
2. [ ] Show recent models di model switcher
3. [ ] Quick-select last used model

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/app/types.rs`
- `/workspace/crates/vac_cli/src/tui/services/model_switcher.rs` (copy dari donor jika perlu)

---

### Workstream 1.4 — Message/Action UX

#### 1.4.1 Copy/Retry/Revert from Message Enhancement (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Expand message action popup dengan lebih banyak actions
2. [ ] Add "Copy code block" action
3. [ ] Add "Retry tool call" action
4. [ ] Add "Revert this change" action
5. [ ] Add "Explain this" action (VIL integration)

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/message_action_popup.rs`
- `/workspace/crates/vac_cli/src/tui/event_loop.rs`

---

#### 1.4.2 Better Session/Approval Shortcuts (P1)
**Status**: ⚠️ PARTIAL  
**Donor Available**: ✅ `shortcuts_popup.rs` (34KB)  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Copy `shortcuts_popup.rs` dari donor ke `services/`
2. [ ] Integrate dengan existing shortcuts system
3. [ ] Add contextual shortcuts (berdasarkan focus)
4. [ ] Add searchable shortcuts list

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/shortcuts_popup.rs` (create from donor)
- `/workspace/crates/vac_cli/src/tui/services.rs` (add mod)

---

## Phase 2 — Agent CLI Parity (Target: 100% Complete)

### Current Status: ~60% Complete

### Workstream 2.1 — CLI Mode Split

#### 2.1.1 Run Async Polish (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Improve autopilot status reporting
2. [ ] Add progress indicators
3. [ ] Better error recovery
4. [ ] Add resume from checkpoint

**Files to Modify**:
- `/workspace/crates/vac_cli/src/commands/autopilot.rs`
- `/workspace/crates/vac_cli/src/commands/run.rs`

---

### Workstream 2.2 — Warden-Grade Isolation UX

#### 2.2.1 Default Mount Presets UI (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Create mount preset configurations
2. [ ] Add UI untuk select preset di isolation switcher
3. [ ] Show current mount configuration
4. [ ] Allow custom mount additions

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/isolation_switcher.rs`
- `/workspace/crates/vac_cli/src/commands/isolation.rs`

---

#### 2.2.2 TTY vs Non-TTY Convenience (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 2-3 jam  

**Tasks**:
1. [ ] Auto-detect interactive vs batch mode
2. [ ] Provide sensible defaults
3. [ ] Add override flag

**Files to Modify**:
- `/workspace/crates/vac_cli/src/commands/isolation.rs`

---

### Workstream 2.3 — MCP Admin Surface

#### 2.3.1 TUI Badges for MCP Servers (P0)
**Status**: ❌ BELUM ADA  
**Effort**: 2-3 jam  

**Note**: Data sudah ada di `AppState`, tinggal render

**Tasks**:
1. [ ] Add MCP server badges di header atau side panel
2. [ ] Show connection status with color coding
3. [ ] Show trust level indicator
4. [ ] Quick access to MCP details on click

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/view.rs` (header area)
- `/workspace/crates/vac_cli/src/tui/services/side_panel.rs` (enhance existing)

---

#### 2.3.2 Mode Mismatch Visibility Enhancement (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 2-3 jam  

**Tasks**:
1. [ ] Add warning indicator when server mode mismatch
2. [ ] Show which modes are allowed/expected
3. [ ] Provide quick fix suggestion

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/side_panel.rs`

---

### Workstream 2.4 — Shell/Operator Loop Polish

#### 2.4.1 Multiple Shell Sessions (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 6-8 jam  

**Tasks**:
1. [ ] Support multiple concurrent shell sessions
2. [ ] Add session switcher UI
3. [ ] Track session state individually
4. [ ] Allow naming sessions

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/app/types.rs` (multiple shells)
- `/workspace/crates/vac_cli/src/tui/services/shell_mode.rs`
- `/workspace/crates/vac_cli/src/tui/view.rs` (session switcher)

---

#### 2.4.2 Interactive Prompt Handling Polish (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Better detection of interactive prompts
2. [ ] Auto-forward input ke shell
3. [ ] Visual indicator saat shell menunggu input

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/shell_mode.rs`
- `/workspace/crates/vac_cli/src/tui/event_loop.rs`

---

#### 2.4.3 Shell → Tool Result Bridging (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Capture shell output sebagai tool result
2. [ ] Parse structured output (JSON, tables)
3. [ ] Feed back ke conversation context

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/shell_mode.rs`
- `/workspace/crates/vac_cli/src/tui/adapter/mod.rs`

---

#### 2.4.4 Clearer Shell State Transitions (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 2-3 jam  

**Tasks**:
1. [ ] Add visual transitions (starting, running, completed, failed)
2. [ ] Show exit code prominently
3. [ ] Add duration display

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/bash_block.rs`
- `/workspace/crates/vac_cli/src/tui/view.rs`

---

## Phase 3 — VIL-Native Superpowers (Target: 100% Complete)

### Current Status: ~40% Complete

### Workstream 3.1 — VIL Project Awareness

#### 3.1.1 Current Rulebook/Semantic Mode Visible (P0)
**Status**: ⚠️ PARTIAL  
**Effort**: 2-3 jam  

**Tasks**:
1. [ ] Show active rulebook name di side panel
2. [ ] Show semantic mode indicator
3. [ ] Display VIL version compatibility

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/side_panel.rs` (VilStatus section)

---

#### 3.1.2 Current IR/Codegen Awareness (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Track IR generation state
2. [ ] Show which files have IR metadata
3. [ ] Display codegen regions in editor

**Files to Create**:
- `/workspace/crates/vac_cli/src/tui/services/ir_status.rs`

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/app/types.rs`
- `/workspace/crates/vac_cli/src/tui/services/side_panel.rs`

---

### Workstream 3.2 — VIL Review Workstation (P0 Priority)

#### 3.2.1 Semantic Validation Findings Display (P0)
**Status**: ❌ BELUM ADA  
**Effort**: 4-6 jam  

**Note**: Infrastructure sudah ada (`vil_validate` crate), tinggal integrate ke TUI

**Tasks**:
1. [ ] Run validation pass on changeset
2. [ ] Display findings di side panel (VilStatus section)
3. [ ] Add severity indicators (error, warning, info)
4. [ ] Click to navigate to offending code
5. [ ] Show validation score trend

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/side_panel.rs` (expand VilStatus)
- `/workspace/crates/vac_cli/src/tui/event_loop.rs` (trigger validation)
- `/workspace/crates/vac_cli/src/tui/adapter/mod.rs` (integrate vil_validate)

**Integration Points**:
```rust
// Use existing vil_validate crate
use vil_validate::{validate_changes, FinalValidationReport};

// Run on changeset updates
let report = validate_changes(&ir_pipeline, &modified_files)?;
state.vil_status.validation_issues = report.issues;
state.vil_status.validation_score = report.score;
```

---

#### 3.2.2 IR-Related Violations (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Detect IR boundary violations
2. [ ] Show violation details
3. [ ] Suggest fixes

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/side_panel.rs`
- `/workspace/crates/vil_validate/src/passes.rs` (add new pass)

---

#### 3.2.3 Generated Plumbing Hints (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Identify generated vs handwritten code
2. [ ] Show hints for generated regions
3. [ ] Warn against manual edits

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/file_diff.rs`
- `/workspace/crates/vil_validate/src/passes.rs` (pass_generated_plumbing already exists)

---

#### 3.2.4 Zero-Copy Contract Risks (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Highlight zero-copy violations
2. [ ] Show performance impact estimate
3. [ ] Suggest ShmSlice alternatives

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/file_diff.rs`
- `/workspace/crates/vil_validate/src/passes.rs` (pass_zero_copy_legality already exists)

---

### Workstream 3.3 — VIL-Aware Code Actions

#### 3.3.1 "Repair VIL Contract" Action (P0)
**Status**: ❌ BELUM ADA  
**Effort**: 6-8 jam  

**Tasks**:
1. [ ] Add code action to message action popup
2. [ ] Auto-generate fix suggestions
3. [ ] Apply fix with user confirmation
4. [ ] Re-run validation to confirm fix

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/message_action_popup.rs`
- `/workspace/crates/vac_cli/src/tui/event_loop.rs`
- `/workspace/crates/vil_validate/src/` (add auto-fix module)

**Files to Create**:
- `/workspace/crates/vil_validate/src/autofix.rs`

---

#### 3.3.2 "Explain Generated Plumbing" Action (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Detect generated code regions
2. [ ] Generate explanation via LLM
3. [ ] Display in popup or inline

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/message_action_popup.rs`
- `/workspace/crates/vac_cli/src/tui/adapter/mod.rs`

---

#### 3.3.3 "Audit Zero-Copy Legality" Action (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Scan for zero-copy violations
2. [ ] Generate audit report
3. [ ] Show before/after comparison

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/message_action_popup.rs`
- `/workspace/crates/vil_validate/src/passes.rs`

---

#### 3.3.4 "Diff IR-Significant Change" Action (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Compare IR before/after change
2. [ ] Highlight semantic differences
3. [ ] Show impact analysis

**Files to Create**:
- `/workspace/crates/vac_cli/src/tui/services/ir_diff.rs`

---

### Workstream 3.4 — VIL-Native Planning Policy

#### 3.4.1 Detect Semantic vs Cosmetic Changes (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Analyze AST/IR changes
2. [ ] Classify change type
3. [ ] Adjust approval policy based on classification

**Files to Create**:
- `/workspace/crates/vac_core/src/change_classifier.rs`

---

#### 3.4.2 Warn When Touching Generated Regions (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Mark generated regions in IR
2. [ ] Check changes against marked regions
3. [ ] Show warning prompt

**Files to Modify**:
- `/workspace/crates/vil_ir/src/types.rs`
- `/workspace/crates/vac_core/src/planner.rs`

---

#### 3.4.3 Distinguish Handwritten vs Generated (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Add metadata tracking
2. [ ] Display indicator di file diff
3. [ ] Different treatment in review

**Files to Modify**:
- `/workspace/crates/vil_ir/src/types.rs`
- `/workspace/crates/vac_cli/src/tui/services/file_diff.rs`

---

## Phase 4 — Claude-Code-Class Polish (Target: 100% Complete)

### Current Status: ~20% Complete

### Workstream 4.1 — Reliability & State Recovery

#### 4.1.1 Stronger Recovery on Interrupted Runs (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Enhance checkpoint serialization
2. [ ] Save pending operations
3. [ ] Resume from exact interruption point
4. [ ] Test various failure scenarios

**Files to Modify**:
- `/workspace/crates/vil_swarm/src/checkpoint.rs`
- `/workspace/crates/vac_cli/src/commands/restore.rs`

---

#### 4.1.2 Cleaner Shell/Runtime Recovery (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Save shell state on checkpoint
2. [ ] Restore working directory
3. [ ] Restore environment variables
4. [ ] Restart background jobs

**Files to Modify**:
- `/workspace/crates/vac_runtime/src/state.rs`
- `/workspace/crates/vac_cli/src/commands/resume.rs`

---

#### 4.1.3 Safer Pending Approval Restore (P0)
**Status**: ❌ BELUM ADA  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Serialize pending approvals
2. [ ] Validate approvals on restore
3. [ ] Re-prompt if needed
4. [ ] Handle expired approvals

**Files to Modify**:
- `/workspace/crates/vil_swarm/src/checkpoint.rs`
- `/workspace/crates/vac_cli/src/tui/app/types.rs`

---

### Workstream 4.2 — Performance

#### 4.2.1 Rendering Cache Refinement (P1)
**Status**: ❌ BELUM ADA  
**Effort**: 6-8 jam  

**Tasks**:
1. [ ] Implement per-message cache invalidation
2. [ ] Add LRU cache for rendered messages
3. [ ] Cache visible lines only
4. [ ] Profile and optimize hot paths

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/app/types.rs` (cache structures)
- `/workspace/crates/vac_cli/src/tui/view.rs` (caching logic)
- `/workspace/crates/vac_cli/src/tui/services/markdown_renderer.rs`

---

#### 4.2.2 Large Repo File Search Responsiveness (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Add incremental search
2. [ ] Debounce input
3. [ ] Background indexing
4. [ ] Cache index results

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/services/file_search.rs`
- `/workspace/crates/vac_cli/src/tui/event_loop.rs`

---

### Workstream 4.3 — Defaults & Ergonomics

#### 4.3.1 Opinionated Defaults (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Define default profiles
2. [ ] Set sensible model defaults
3. [ ] Configure isolation presets
4. [ ] Document defaults

**Files to Modify**:
- `/workspace/crates/vac_core/src/config.rs`
- `/workspace/crates/vac_cli/src/commands/init.rs`

---

#### 4.3.2 Better Onboarding (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 4-6 jam  

**Tasks**:
1. [ ] Enhance `vac doctor` command
2. [ ] Add interactive setup wizard
3. [ ] Create tutorial mode
4. [ ] Add contextual help

**Files to Modify**:
- `/workspace/crates/vac_cli/src/commands/doctor.rs`
- `/workspace/crates/vac_cli/src/commands/init.rs`

---

#### 4.3.3 Environment Warnings (P1)
**Status**: ⚠️ PARTIAL  
**Effort**: 3-4 jam  

**Tasks**:
1. [ ] Check environment on startup
2. [ ] Warn about missing tools
3. [ ] Warn about config issues
4. [ ] Show in toast or banner

**Files to Modify**:
- `/workspace/crates/vac_cli/src/tui/event_loop.rs`
- `/workspace/crates/vac_cli/src/tui/services/toast.rs`

---

## Implementation Priority Matrix

### P0 (Critical - Do First)
| Item | Phase | Effort | Impact |
|------|-------|--------|--------|
| Clipboard Paste | 1.1 | 4-6h | High |
| Text Selection | 1.1 | 6-8h | High |
| Clickable Side Panel | 1.2 | 3-4h | Medium |
| MCP TUI Badges | 2.3 | 2-3h | High |
| Semantic Validation Display | 3.2 | 4-6h | Very High |
| Repair VIL Contract Action | 3.3 | 6-8h | Very High |
| Safer Pending Approval Restore | 4.1 | 4-6h | High |

### P1 (Important - Do Second)
| Item | Phase | Effort | Impact |
|------|-------|--------|--------|
| Recent Commands | 1.1 | 4-6h | Medium |
| Better Command Filtering | 1.1 | 2-3h | Medium |
| Sessions Section | 1.2 | 4-6h | Medium |
| Recent Model History | 1.3 | 2-3h | Low |
| Message Actions Enhancement | 1.4 | 3-4h | Medium |
| Shortcuts Popup | 1.4 | 4-6h | Medium |
| Run Async Polish | 2.1 | 3-4h | Medium |
| Mount Presets UI | 2.2 | 3-4h | Medium |
| Multiple Shell Sessions | 2.4 | 6-8h | Medium |
| IR/Codegen Awareness | 3.1 | 4-6h | High |
| All VIL Code Actions | 3.3 | 16-24h | Very High |
| VIL Planning Policy | 3.4 | 10-14h | High |
| Rendering Cache | 4.2 | 6-8h | High |

### P2 (Nice to Have)
| Item | Phase | Effort | Impact |
|------|-------|--------|--------|
| Billing Section | 1.2 | 2-3h | Low |
| TTY vs Non-TTY | 2.2 | 2-3h | Low |
| Environment Warnings | 4.3 | 3-4h | Medium |

---

## Suggested Sprint Plan

### Sprint 1 (Week 1-2): Phase 1 Completion
- Clipboard Paste (4-6h)
- Text Selection (6-8h)
- Clickable Side Panel (3-4h)
- Recent Commands (4-6h)
- Better Command Filtering (2-3h)
- Sessions Section (4-6h)
- Shortcuts Popup (4-6h)

**Total**: ~30 hours

### Sprint 2 (Week 3-4): Phase 2 Completion + VIL Foundation
- MCP TUI Badges (2-3h)
- Multiple Shell Sessions (6-8h)
- Shell Polish (9-13h)
- Semantic Validation Display (4-6h)
- IR/Codegen Awareness (4-6h)

**Total**: ~30 hours

### Sprint 3 (Week 5-6): VIL Superpowers
- Repair VIL Contract Action (6-8h)
- Explain Generated Plumbing (4-6h)
- Audit Zero-Copy Legality (4-6h)
- Diff IR-Significant Change (4-6h)
- VIL Planning Policy (10-14h)

**Total**: ~35 hours

### Sprint 4 (Week 7-8): Polish & Reliability
- Safer Pending Approval Restore (4-6h)
- Rendering Cache (6-8h)
- File Search Optimization (4-6h)
- Recovery Improvements (7-10h)
- Onboarding Enhancement (4-6h)

**Total**: ~30 hours

---

## Risk Assessment

### Technical Risks
1. **Donor Code Compatibility**: Stakpak donor code may need adaptation
   - Mitigation: Test incrementally, adapt piecemeal

2. **VIL Integration Complexity**: Deep IR integration could be complex
   - Mitigation: Start with simple validation display, iterate

3. **Performance Regression**: New features may slow down TUI
   - Mitigation: Implement caching early, profile regularly

### Schedule Risks
1. **Scope Creep**: VIL features could expand beyond estimate
   - Mitigation: Stick to MVP for each feature

2. **Testing Overhead**: Comprehensive testing may take longer
   - Mitigation: Write tests alongside features

---

## Success Metrics

### Phase 1 Success
- [ ] All helper/input features working
- [ ] Side panel fully interactive with 6 sections
- [ ] Profile/rulebook/model switching seamless
- [ ] Message actions comprehensive

### Phase 2 Success
- [ ] All CLI commands polished
- [ ] Isolation UX best-in-class
- [ ] MCP management complete
- [ ] Shell experience smooth

### Phase 3 Success
- [ ] VIL validation visible and actionable
- [ ] Code actions automated
- [ ] Planning policy VIL-aware
- [ ] Clear differentiation from generic AI coders

### Phase 4 Success
- [ ] Recovery from interruptions seamless
- [ ] Performance on large repos acceptable
- [ ] Onboarding experience excellent
- [ ] Defaults sensible and documented

---

## Appendix: Donor Files Inventory

### Ready to Activate (High Priority)
| File | Size | Purpose | Priority |
|------|------|---------|----------|
| `clipboard_paste.rs` | 18KB | Image/text paste | P0 |
| `text_selection.rs` | 23KB | Mouse text selection | P0 |
| `shortcuts_popup.rs` | 34KB | Comprehensive shortcuts | P1 |
| `model_switcher.rs` | 17KB | Rich model selection | P1 |
| `shell_popup.rs` | 8KB | Shell management | P1 |

### Reference Only (May Need Rewrite)
| File | Size | Purpose |
|------|------|---------|
| `bash_block.rs` | 156KB | Enhanced bash rendering |
| `message.rs` | 110KB | Advanced message handling |
| `textarea.rs` | 82KB | Full-featured textarea |
| `plan_review.rs` | 63KB | Plan review workflow |

---

## Conclusion

This implementation plan provides a clear roadmap to complete VAC development across all four phases. The estimated total effort is approximately 125-150 hours (15-20 working days), which can be completed in 4 sprints of 2 weeks each.

**Key Recommendations**:
1. Start with P0 items for immediate impact
2. Leverage donor code where available
3. Prioritize VIL differentiation features (Phase 3)
4. Don't skip reliability work (Phase 4)

**Next Steps**:
1. Review and approve this plan
2. Set up project tracking (GitHub Projects or similar)
3. Begin Sprint 1 with P0 items
4. Schedule weekly progress reviews
