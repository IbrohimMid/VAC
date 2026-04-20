#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashSet;

    // ── Existing keymap / command contracts ───────────────────────────────────

    #[test]
    fn test_keymap_uniqueness_contract() {
        let source = include_str!("event.rs");
        let mut seen = HashSet::new();

        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("KeyCode::") {
                let pattern = if trimmed.contains("=>") {
                    trimmed.split("=>").next().unwrap().trim().to_string()
                } else {
                    trimmed.split('{').next().unwrap().trim().to_string()
                };
                let normalized = pattern.split_whitespace().collect::<Vec<_>>().join(" ");
                if normalized.contains("KeyCode::Char(c)") {
                    continue;
                }
                assert!(
                    seen.insert(normalized.clone()),
                    "Duplicate keymap found in event.rs: {}",
                    normalized
                );
            }
        }
    }

    #[test]
    fn test_command_dispatch_uniqueness_contract() {
        let commands = crate::services::helper_block::vac_commands();
        let mut seen = HashSet::new();
        for cmd in commands {
            assert!(
                seen.insert(cmd.command.clone()),
                "Duplicate command found: {}",
                cmd.command
            );
        }
    }

    // ── OverlayManager journey tests ──────────────────────────────────────────

    #[test]
    fn overlay_open_makes_is_active_true() {
        let mut state = crate::app::AppState::default();
        assert!(
            !state
                .overlay_manager
                .is_active(crate::overlay::OverlayId::CommandPalette)
        );
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        assert!(
            state
                .overlay_manager
                .is_active(crate::overlay::OverlayId::CommandPalette)
        );
    }

    #[test]
    fn overlay_close_makes_is_active_false() {
        let mut state = crate::app::AppState::default();
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ModelSwitcher);
        crate::overlay::close_overlay(&mut state, crate::overlay::OverlayId::ModelSwitcher);
        assert!(
            !state
                .overlay_manager
                .is_active(crate::overlay::OverlayId::ModelSwitcher)
        );
    }

    #[test]
    fn overlay_stack_topmost_is_last_pushed() {
        let mut state = crate::app::AppState::default();
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        assert_eq!(
            state.overlay_manager.topmost(),
            Some(crate::overlay::OverlayId::Shortcuts),
            "topmost should be the last pushed overlay"
        );
    }

    #[test]
    fn overlay_stack_close_topmost_reveals_previous() {
        let mut state = crate::app::AppState::default();
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        crate::overlay::close_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        assert_eq!(
            state.overlay_manager.topmost(),
            Some(crate::overlay::OverlayId::CommandPalette),
            "CommandPalette should be topmost after Shortcuts is closed"
        );
    }

    #[test]
    fn overlay_open_is_idempotent() {
        let mut state = crate::app::AppState::default();
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::FileSearch);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::FileSearch);
        // Stack should only contain one entry for FileSearch
        let active: Vec<_> = crate::overlay::active_render_ids(&state).collect();
        let count = active
            .iter()
            .filter(|&&id| id == crate::overlay::OverlayId::FileSearch)
            .count();
        assert_eq!(
            count, 1,
            "FileSearch should appear exactly once in the stack"
        );
    }

    #[test]
    fn close_all_overlays_drains_stack() {
        let mut state = crate::app::AppState::default();
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ModelSwitcher);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        crate::overlay::close_all_overlays(&mut state);
        assert!(
            !state.overlay_manager.any_active(),
            "all overlays should be closed"
        );
    }

    #[test]
    fn overlay_focus_restored_on_stack_drain() {
        use crate::app::WorkspaceFocus;
        let mut state = crate::app::AppState::default();
        state.focus = WorkspaceFocus::Conversation;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        // Focus may change while overlay is open — simulate that
        state.focus = WorkspaceFocus::Input;
        crate::overlay::close_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        // When stack drains, saved focus (Conversation) should be restored
        assert_eq!(
            state.focus,
            WorkspaceFocus::Conversation,
            "focus should be restored to pre-overlay value when stack drains"
        );
    }

    // ── Action registry contracts ─────────────────────────────────────────────

    #[test]
    fn action_specs_no_duplicate_ids() {
        use crate::action_registry::ACTION_SPECS;
        let mut seen = HashSet::new();
        for spec in ACTION_SPECS.iter() {
            assert!(
                seen.insert(spec.id as u32),
                "Duplicate ActionId in ACTION_SPECS: {:?}",
                spec.id
            );
        }
    }

    #[test]
    fn action_specs_slash_aliases_globally_unique() {
        use crate::action_registry::ACTION_SPECS;
        let mut seen = HashSet::new();
        for spec in ACTION_SPECS.iter() {
            for alias in spec.slash_aliases {
                assert!(
                    seen.insert(*alias),
                    "Duplicate slash alias '{}' in ACTION_SPECS (action: {:?})",
                    alias,
                    spec.id
                );
            }
        }
    }

    #[test]
    fn action_spec_by_slash_alias_roundtrips() {
        use crate::action_registry::{ACTION_SPECS, spec_by_slash_alias};
        for spec in ACTION_SPECS.iter() {
            for alias in spec.slash_aliases {
                let found = spec_by_slash_alias(alias);
                assert!(
                    found.is_some(),
                    "spec_by_slash_alias('{}') returned None",
                    alias
                );
                assert_eq!(
                    found.unwrap().id as u32,
                    spec.id as u32,
                    "spec_by_slash_alias('{}') returned wrong ActionId",
                    alias
                );
            }
        }
    }

    // ── WorkbenchTabView tab label contract ───────────────────────────────────

    #[test]
    fn workbench_tab_labels_count_is_seven() {
        let state = crate::app::AppState::default();
        let labels = crate::workbench::tab_labels(&state);
        assert_eq!(labels.len(), 7, "expected exactly 7 workbench tab labels");
    }

    #[test]
    fn workbench_active_tab_index_covers_all_variants() {
        use crate::app::WorkbenchTab;
        let variants = [
            WorkbenchTab::Approvals,
            WorkbenchTab::Review,
            WorkbenchTab::Sessions,
            WorkbenchTab::Agents,
            WorkbenchTab::Runtime,
            WorkbenchTab::Plan,
            WorkbenchTab::Vil,
        ];
        let indices: Vec<usize> = variants
            .iter()
            .map(crate::workbench::active_tab_index)
            .collect();
        let unique: HashSet<usize> = indices.iter().copied().collect();
        assert_eq!(
            unique.len(),
            7,
            "each WorkbenchTab variant should map to a unique index"
        );
        assert_eq!(*indices.iter().max().unwrap(), 6, "max index should be 6");
    }

    // ── ACTION_SPECS keybinding consistency ──────────────────────────────────

    /// Every ActionSpec with footer_visible=true must have at least one keybinding
    /// so the hint it shows in the footer is actionable.
    #[test]
    fn footer_visible_actions_have_keybindings() {
        for spec in crate::action_registry::ACTION_SPECS {
            if spec.footer_visible {
                assert!(
                    !spec.keybindings.is_empty(),
                    "ActionId::{:?} has footer_visible=true but no keybindings — user sees hint with no key",
                    spec.id,
                );
            }
        }
    }

    /// footer_specs for each workbench context must be non-empty (proves WorkbenchAny
    /// tab-cycling hint propagates to every tab).
    #[test]
    fn footer_specs_non_empty_for_all_workbench_contexts() {
        use crate::action_registry::ActionContext;
        let workbench_ctxs = [
            ActionContext::WorkbenchApprovals,
            ActionContext::WorkbenchReview,
            ActionContext::WorkbenchSessions,
            ActionContext::WorkbenchAgents,
            ActionContext::WorkbenchRuntime,
            ActionContext::WorkbenchPlan,
            ActionContext::WorkbenchVil,
        ];
        for ctx in workbench_ctxs {
            let count = crate::action_registry::footer_specs(ctx).count();
            assert!(
                count > 0,
                "footer_specs for {ctx:?} is empty — workbench tab shows no hints",
            );
        }
    }

    /// tab_from_index is the inverse of active_tab_index: round-trip must be identity.
    #[test]
    fn tab_index_roundtrip() {
        use crate::app::WorkbenchTab;
        let variants = [
            WorkbenchTab::Approvals,
            WorkbenchTab::Review,
            WorkbenchTab::Sessions,
            WorkbenchTab::Agents,
            WorkbenchTab::Runtime,
            WorkbenchTab::Plan,
            WorkbenchTab::Vil,
        ];
        for tab in &variants {
            let idx = crate::workbench::active_tab_index(tab);
            let restored = crate::workbench::tab_from_index(idx);
            assert_eq!(
                *tab, restored,
                "tab_from_index(active_tab_index({tab:?})) != {tab:?}",
            );
        }
    }

    // ── Deterministic Esc precedence ──────────────────────────────────────────

    #[tokio::test]
    async fn esc_closes_topmost_overlay_first() {
        use crate::app::InputEvent;
        use tokio::sync::mpsc;

        let mut state = crate::app::AppState::default();
        let (tx, _rx) = mpsc::channel(16);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        assert!(state.overlay_manager.any_active());
        crate::handlers::input_core::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);
        assert!(
            !state.overlay_manager.any_active(),
            "Esc should close the topmost overlay"
        );
    }
}
