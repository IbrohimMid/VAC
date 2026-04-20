---
name: TUI Interaction Model Refactor
description: Large ongoing refactor of vac_tui_runtime — overlay stack, action registry, input routing
type: project
---

Ongoing execution of the TUI convergence plan documented in `docs/tui/`.

**Why:** The old code had 16+ `show_*: bool` flags, a 779-line if-chain dispatcher, duplicated boolean checks in `ActionContext::from_app_state`, and no single source of truth for keybindings/actions.

**How to apply:** Continue from where left off — check todo list in session. Reference `docs/tui/interaction_model.md`, `overlay_contract.md`, `action_matrix.md` for contracts.

## Status (as of 2026-04-20)

### Completed
- Phase 0: docs/tui/{interaction_model,overlay_contract,action_matrix}.md written
- Patch 1.1: OverlayId::RejectReason, ReviewPane added; AtDropdown tracked in manager; set_show_flag handles all 17 ids
- Patch 1.2: Removed `any_popup_active()` fn; ActionContext::from_app_state = 3 lines
- Patch 1.3: input_core.rs split → router + handlers/workspace_input.rs + handlers/workbench_input.rs
- Patch 1.4: input_popup.rs dispatch_popup_event → match overlay_manager.topmost(); per-overlay private fns
- Patch 1.5: handle_esc() deterministic precedence in input_core.rs
- Handler migrations: all open()/close() in model_switcher, file_search, changeset, profile_switcher, rulebook_switcher, isolation_switcher, message_action, workspace_input (helper/at), input_commands migrated to open_overlay/close_overlay
- Patch 2.1: ActionId enum + ActionSpec struct + ACTION_SPECS static registry; footer_specs, palette_specs, spec_by_slash_alias, specs_for_context helpers
- Patch 2.2 (partial): Footer now uses footer_specs(ctx) + availability check
- Phase 3.1: render_header strips runtime.snapshot (exec/intent/env); those 3 chips moved to render_operator_panel

### Pre-existing bugs fixed (not part of plan)
- vac_approvals/src/lib.rs: lifetime on lock_recover
- vac_tui_runtime/src/runner.rs: ConfigError → VacError::Io; runtime_project_root moved-into-loop-closure fix

- Phase 3.2: side_panel.rs — Changeset/Todos demoted to 1-line summary stubs ("▸ Changeset (N) — Workbench"); panel keeps 5 primary sections
- Phase 3.3: render_activity_panel and render_operator_panel cross-concerns — already clean, verified
- Phase 4: WorkbenchTabView trait — `src/workbench/mod.rs` with trait + 7 implementors (ApprovalsTab, ReviewTab, SessionsTab, AgentsTab, RuntimeTab, PlanTab, VilTab); view.rs uses tab_labels/active_tab_index/render_active_tab; view.rs reduced 2342→1388 lines; all list/detail splits normalized to 35/65
- Phase 5: Removed all 12 redundant `show_*` bool fields from AppState; OverlayManager is now sole source of truth; `set_show_flag` replaced with `sync_domain_state` (5 arms: PlanReview, ShellPopup, AtDropdown, RejectReason, ReviewPane); all 64 reads replaced with `overlay_manager.is_active(id)`

- Phase 6.1: RenderMetrics (last_render_time_us, avg_render_time_us) wired to terminal.draw(); over-budget ⚠ indicator in operator panel
- Phase 6.2: 13 new journey tests in contracts_test.rs (overlay open/close/stack, action registry, WorkbenchTabView, esc determinism)
- Phase 6.3: focus_style unified to Color::DarkGray unfocused; operator panel render budget indicator
- Phase 6.4: WorkbenchAny variant added to ActionContext; CycleWorkbenchTab spec scoped to WorkbenchAny; specs_for_context updated; all test setups using direct flag assignment (review.open, at_trigger_active) fixed to use open_overlay

### Next (in priority order)
1. Patch 2.3: mouse click regions → ActionId dispatch (deferred)
2. Phase 6 journeys pass — all tests green
