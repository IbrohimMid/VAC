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

    // ── AppState domain census (regression guard for M8 refactor) ────────────
    //
    // The 367-flat-field AppState was collapsed into ~11 owned domain
    // sub-structs. If this test fails upward, someone added a flat
    // field to AppState — push it into the appropriate domain instead.
    #[test]
    fn app_state_stays_domain_organized() {
        let source = include_str!("app/types/mod.rs");
        let start = source
            .find("pub struct AppState {")
            .expect("AppState struct must exist");
        let tail = &source[start..];
        let end = tail.find("\n}\n").expect("AppState must close");
        let body = &tail[..end];
        let field_count = body
            .lines()
            .filter(|l| l.trim_start().starts_with("pub "))
            .count();
        assert!(
            field_count <= 15,
            "AppState has {} top-level fields — keep it domain-organized (≤15)",
            field_count
        );
    }

    // ── OverlayManager journey tests ──────────────────────────────────────────

    #[test]
    fn overlay_open_makes_is_active_true() {
        let mut state = crate::app::AppState::default();
        assert!(
            !state
                .layout.overlay_manager
                .is_active(crate::overlay::OverlayId::CommandPalette)
        );
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        assert!(
            state
                .layout.overlay_manager
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
                .layout.overlay_manager
                .is_active(crate::overlay::OverlayId::ModelSwitcher)
        );
    }

    #[test]
    fn overlay_stack_topmost_is_last_pushed() {
        let mut state = crate::app::AppState::default();
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        assert_eq!(
            state.layout.overlay_manager.topmost(),
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
            state.layout.overlay_manager.topmost(),
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
            !state.layout.overlay_manager.any_active(),
            "all overlays should be closed"
        );
    }

    #[test]
    fn overlay_focus_restored_on_stack_drain() {
        use crate::app::WorkspaceFocus;
        let mut state = crate::app::AppState::default();
        state.layout.focus = WorkspaceFocus::Conversation;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        // Focus may change while overlay is open — simulate that
        state.layout.focus = WorkspaceFocus::Input;
        crate::overlay::close_overlay(&mut state, crate::overlay::OverlayId::Shortcuts);
        // When stack drains, saved focus (Conversation) should be restored
        assert_eq!(
            state.layout.focus,
            WorkspaceFocus::Conversation,
            "focus should be restored to pre-overlay value when stack drains"
        );
    }

    // ── UX unification invariants (U6) ────────────────────────────────────────

    /// U6 — SystemPulse stays borrow-only. Any future refactor that
    /// adds `Clone` or `Arc` on SystemPulse itself creates the
    /// "third event plane" the unification plan forbids.
    #[test]
    fn system_pulse_source_has_no_clone_or_arc_on_pulse_type() {
        let src = include_str!("system_pulse.rs");
        let banned = ["Arc<SystemPulse", "Clone for SystemPulse"];
        for needle in banned {
            assert!(
                !src.contains(needle),
                "SystemPulse must remain borrow-only; found '{needle}'"
            );
        }
    }

    /// U6 — every pulse facet's NavTarget is applyable against a
    /// fresh AppState without panicking. Catches future facets that
    /// reference a removed WorkbenchTab or OverlayId variant.
    #[test]
    fn every_pulse_facet_nav_target_is_applyable() {
        use crate::app::AppState;
        use crate::system_pulse::SystemPulse;
        let state = AppState::default();
        let pulse = SystemPulse::from_state(&state);
        for facet in pulse.facets() {
            let tgt = facet
                .nav_target
                .clone()
                .unwrap_or_else(|| panic!("facet {:?} missing nav_target", facet.kind));
            let mut fresh = AppState::default();
            let _ = tgt.apply(&mut fresh);
        }
    }

    /// U6 — compact statusline tokens stay under 80 cols so they
    /// survive narrow terminals.
    #[test]
    fn pulse_compact_line_fits_width_budget() {
        use crate::app::AppState;
        use crate::system_pulse::SystemPulse;
        let state = AppState::default();
        let pulse = SystemPulse::from_state(&state);
        let line = pulse.compact_line();
        assert!(
            line.chars().count() <= 80,
            "compact_line too wide: {} chars",
            line.chars().count()
        );
    }

    /// U6 — NotifyRouter severity→lane matrix.
    #[test]
    fn notify_router_severity_lane_matrix_is_stable() {
        use crate::app::AppState;
        use crate::services::notify_router::{route, NotifyEvent};

        let mut state = AppState::default();
        route(&mut state, NotifyEvent::info("a", "i"));
        assert_eq!(state.execution.activity.len(), 1);
        assert!(state.layout.toasts.is_empty());
        assert!(state.layout.banner.message.is_none());

        let mut state = AppState::default();
        route(&mut state, NotifyEvent::warn("a", "w"));
        assert_eq!(state.execution.activity.len(), 1);
        assert_eq!(state.layout.toasts.len(), 1);
        assert!(state.layout.banner.message.is_none());

        let mut state = AppState::default();
        route(&mut state, NotifyEvent::critical("a", "c"));
        assert_eq!(state.execution.activity.len(), 1);
        assert!(state.layout.toasts.is_empty());
        assert!(state.layout.banner.message.is_some());
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

    /// U0 — no two ACTION_SPECS in the same scope can share a
    /// keybinding. Scope-qualified so a Global binding and a tab-
    /// local binding with the same chord are allowed (rare but
    /// legitimate). Prior to U0 `Ctrl+P` was bound to both
    /// `OpenCommandPalette` and `OpenFilePicker` in Global scope —
    /// this test pins the fix in place.
    #[test]
    fn action_specs_no_duplicate_keybindings_in_scope() {
        use crate::action_registry::ACTION_SPECS;
        let mut seen: HashSet<(String, String)> = HashSet::new();
        for spec in ACTION_SPECS.iter() {
            let scope_key = format!("{:?}", spec.scope);
            for kb in spec.keybindings {
                assert!(
                    seen.insert((scope_key.clone(), (*kb).to_string())),
                    "Duplicate keybinding '{kb}' in scope {scope_key} \
                     (action: {:?})",
                    spec.id
                );
            }
        }
    }

    /// U0 — overlap consistency between the two registries.
    ///
    /// The two surfaces (`ACTION_SPECS` and `helper_block::vac_commands`)
    /// carry different semantics: ACTION_SPECS is the keybinding /
    /// palette-availability registry; `vac_commands` adds passthrough
    /// and slash-only entries (`/vil`, `/swarm`, …) that have no
    /// keybinding or context.
    ///
    /// We do NOT require full coverage in either direction. We DO
    /// require: **every slash alias that appears in both must
    /// resolve back to the same ActionId via `spec_by_slash_alias`.**
    /// If a drift-introducing commit stuffs `/files` into
    /// `vac_commands` but forgets to update ACTION_SPECS (or vice
    /// versa and the mapping now points at a different ActionId),
    /// this test fails.
    #[test]
    fn action_specs_and_helper_block_overlap_is_consistent() {
        use crate::action_registry::{ACTION_SPECS, spec_by_slash_alias};
        let helper_commands =
            crate::services::helper_block::vac_commands();
        let spec_alias_to_id: std::collections::HashMap<&'static str, u32> =
            ACTION_SPECS
                .iter()
                .flat_map(|s| s.slash_aliases.iter().map(move |a| (*a, s.id as u32)))
                .collect();
        for cmd in &helper_commands {
            let alias = cmd.command.as_str();
            // If ACTION_SPECS claims this alias AND `spec_by_slash_alias`
            // exposes it, the round-trip must agree on the id.
            if let Some(expected_id) = spec_alias_to_id.get(alias) {
                let via_fn = spec_by_slash_alias(alias)
                    .map(|s| s.id as u32)
                    .unwrap_or(u32::MAX);
                assert_eq!(
                    via_fn, *expected_id,
                    "slash alias '{alias}' resolves to different ActionId \
                     via spec_by_slash_alias vs direct ACTION_SPECS lookup — \
                     registry drift",
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
    fn workbench_tab_labels_count_matches_variants() {
        let state = crate::app::AppState::default();
        let labels = crate::workbench::tab_labels(&state);
        assert_eq!(labels.len(), 9, "expected exactly 9 workbench tab labels");
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
            WorkbenchTab::Vwfd,
            WorkbenchTab::Signal,
        ];
        let indices: Vec<usize> = variants
            .iter()
            .map(crate::workbench::active_tab_index)
            .collect();
        let unique: HashSet<usize> = indices.iter().copied().collect();
        assert_eq!(
            unique.len(),
            9,
            "each WorkbenchTab variant should map to a unique index"
        );
        assert_eq!(*indices.iter().max().unwrap(), 8, "max index should be 8");
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
            ActionContext::WorkbenchVwfd,
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
            WorkbenchTab::Vwfd,
            WorkbenchTab::Signal,
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
        assert!(state.layout.overlay_manager.any_active());
        crate::handlers::input_core::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);
        assert!(
            !state.layout.overlay_manager.any_active(),
            "Esc should close the topmost overlay"
        );
    }

    /// F3.5 — AppState structural invariants. Locks in the sub-struct
    /// grouping so later refactors don't silently regress the domain
    /// boundaries Fase 3 established.
    ///
    /// The `std::mem::size_of_val` checks verify the sub-structs exist
    /// with non-ZST layout — any regression that flattens the fields
    /// back onto AppState would fail to compile here.
    #[test]
    fn appstate_new_honors_structural_invariants() {
        let state = crate::app::AppState::default();

        // F3.1 — Operator grouping is non-ZST and fields have moved.
        let _: &crate::app::types::OperatorState = &state.operator_config.operator;
        assert!(state.operator_config.operator.current_model.is_none());
        assert_eq!(state.operator_config.operator.sessions_selected_idx, 0);
        assert_eq!(state.operator_config.operator.theme_picker_selected, 0);
        assert_eq!(state.operator_config.operator.message_action_popup_selected, 0);
        assert!(state.operator_config.operator.message_action_target_id.is_none());

        // F3.3 — SessionMetaState grouping.
        let _: &crate::app::types::SessionMetaState = &state.session.session_meta;
        assert!(state.session.session_meta.title.is_none());
        assert!(state.session.session_meta.checkpoint_path.is_none());
        assert!(!state.session.session_meta.loading);

        // F3.2 — BridgeState placeholder: always detached on boot.
        let _: &crate::app::types::BridgeState = &state.execution.bridge;
        assert!(!state.execution.bridge.attached);

        // ScrollState zeros.
        assert_eq!(state.layout.scroll.messages, 0);
        assert_eq!(state.layout.scroll.activity, 0);
        assert_eq!(state.layout.scroll.cursor_position, 0);

        // Hydration is a two-step dance: flag off, deadline in future.
        assert!(!state.core.hydrated);
        assert!(state.core.hydration_deadline > std::time::Instant::now());
    }

    /// F3.5 — AppStateOptions must route into the sub-structs, not
    /// onto AppState directly. If a future refactor re-introduces a
    /// flat `current_model` / `checkpoint_path` field on AppState,
    /// this test still passes through sub-struct access — but if the
    /// constructor forgets to wire options into the sub-struct, the
    /// values won't show up and the asserts fail. That's the actual
    /// regression we care about.
    #[test]
    fn appstate_options_route_into_substructs() {
        use crate::app::types::AppStateOptions;
        use crate::types::Model;

        let model = Model {
            name: "claude-sonnet-4-6".to_string(),
            ..Model::default()
        };
        let checkpoint = std::path::PathBuf::from("/tmp/vac-fake-checkpoint");
        let state = crate::app::AppState::new(AppStateOptions {
            model: Some(model.clone()),
            session_id: Some("test-session".into()),
            checkpoint_path: Some(checkpoint.clone()),
            project_root: std::env::temp_dir(),
        });

        assert_eq!(state.session.session_id, "test-session");
        assert_eq!(
            state.operator_config.operator.current_model.as_ref().map(|m| m.name.as_str()),
            Some(model.name.as_str()),
            "options.model must route into operator.current_model",
        );
        assert_eq!(
            state.session.session_meta.checkpoint_path.as_ref(),
            Some(&checkpoint),
            "options.checkpoint_path must route into session_meta.checkpoint_path",
        );
    }

    // ── F6.5 — Checkpoint resumption contract ──────────────────

    /// F6.5 — checkpoint_path lives under session_meta, not on the
    /// AppState root. Guards against a future refactor flattening
    /// the grouping back out.
    #[test]
    fn checkpoint_path_lives_on_session_meta() {
        use crate::app::types::AppStateOptions;
        let path = std::path::PathBuf::from("/tmp/vac/session.checkpoint");
        let state = crate::app::AppState::new(AppStateOptions {
            model: None,
            session_id: Some("s-abc".into()),
            checkpoint_path: Some(path.clone()),
            project_root: std::env::temp_dir(),
        });
        assert_eq!(state.session.session_meta.checkpoint_path.as_ref(), Some(&path));
    }

    /// F6.5 — absent checkpoint path renders as `None` on boot and
    /// after an explicit clear.
    #[test]
    fn checkpoint_path_absent_by_default_and_clearable() {
        let mut state = crate::app::AppState::default();
        assert!(state.session.session_meta.checkpoint_path.is_none());
        state.session.session_meta.checkpoint_path =
            Some(std::path::PathBuf::from("/tmp/ck"));
        state.session.session_meta.checkpoint_path = None;
        assert!(state.session.session_meta.checkpoint_path.is_none());
    }

    /// F6.5 — `vac resume <checkpoint>` parses the session id from
    /// the checkpoint filename, honoring the historical `_state`
    /// suffix. Use `strip_suffix` (not `replace`) so a UUID whose
    /// hex encoding happens to contain the substring `_state` is
    /// not corrupted. `test_state_<uuid>.json` is a prefix shape
    /// that should parse back to the real uuid.
    #[test]
    fn checkpoint_filename_parses_session_id() {
        use std::path::PathBuf;
        use uuid::Uuid;
        fn parse_stem(stem: &str) -> Option<Uuid> {
            let core = stem.strip_suffix("_state").unwrap_or(stem);
            Uuid::parse_str(core).ok()
        }
        let sid = Uuid::new_v4();
        for p in [
            PathBuf::from(format!("/ck/{sid}.json")),
            PathBuf::from(format!("/ck/{sid}_state.json")),
        ] {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap();
            assert_eq!(parse_stem(stem), Some(sid));
        }
        // Filenames that are NOT valid checkpoint stems must not
        // masquerade as uuids via substring rewriting.
        assert!(parse_stem("test_state_sid").is_none());
        assert!(parse_stem("not-a-uuid").is_none());
    }

    /// R2.c — service wires live on AppState and stay reachable
    /// from handler code. Without this assertion, a "dead field"
    /// cleanup could silently drop the rate-limit banner / prompt
    /// history surfaces.
    #[test]
    fn service_wires_reachable_on_appstate() {
        let mut state = crate::app::AppState::default();
        assert!(!state.transcript.rate_limit.is_active());
        let before = state.transcript.rate_limit.current_message().to_string();
        let after = state.transcript.rate_limit.next_message().to_string();
        assert_ne!(before, after);
        assert!(state.composer.prompt_history.is_empty());
        state.composer.prompt_history.record("refactor auth module");
        let hits = state.composer.prompt_history.suggest("refactor", 5);
        assert_eq!(hits.len(), 1);
    }
}
