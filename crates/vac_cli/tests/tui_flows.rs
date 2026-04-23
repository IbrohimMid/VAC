#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]

//! Integration tests for TUI state flow.

use std::collections::HashSet;
use std::fs;

use tempfile::tempdir;
use tokio::sync::mpsc;
use uuid::Uuid;

use vac_tui_runtime::{
    app::ShortcutsPopupMode,
    app::{AppState, AppStateOptions, InputEvent, OutputEvent, PendingUserMessage, SessionInfo},
    handlers::{HandlerContext, input_core, profile_switcher, rulebook_switcher},
    services::{
        commands::{CommandAction, CommandSurface},
        helper_block, shortcuts_popup,
    },
    update::flush_pending_user_messages_if_idle,
};

fn slash_commands_from_palette(state: &AppState) -> HashSet<String> {
    state
        .filtered_commands()
        .into_iter()
        .map(|cmd| cmd.command)
        .collect()
}

fn slash_commands_from_shortcuts(state: &AppState) -> HashSet<String> {
    shortcuts_popup::filter_commands("", state)
        .into_iter()
        .filter_map(|cmd| match cmd.action {
            CommandAction::InsertSlashCommand(s) => Some(s),
            _ => None,
        })
        .collect()
}

#[test]
fn welcome_copy_is_truthful_on_first_frame() {
    let state = AppState::default();
    let messages = helper_block::welcome_messages(None, &state);

    assert_eq!(messages.len(), 1);
    let content = &messages[0].content;
    assert!(content.contains("Vastar Agentic CLI"));
    assert!(content.contains("No active model configured"));
    assert!(!content.contains("unknown"));
    assert!(!content.contains("Model: none"));
}

#[test]
fn operator_surfaces_hide_passthrough_commands_and_match_parity() {
    let state = AppState::default();
    let palette = slash_commands_from_palette(&state);
    let shortcuts = slash_commands_from_shortcuts(&state);

    assert_eq!(palette, shortcuts);

    let registry = helper_block::vac_commands();
    for hidden in ["/vil", "/swarm", "/rulebook", "/resume", "/help"] {
        let spec = registry
            .iter()
            .find(|cmd| cmd.command == hidden)
            .unwrap_or_else(|| panic!("missing registry entry for {hidden}"));
        assert_eq!(
            spec.surface,
            CommandSurface::Hidden,
            "{hidden} should remain hidden from operator surfaces"
        );
        assert!(
            !palette.contains(hidden),
            "{hidden} leaked into command palette"
        );
        assert!(
            !shortcuts.contains(hidden),
            "{hidden} leaked into shortcuts popup"
        );
    }
}

#[test]
fn popup_precedence_blocks_lower_priority_open_requests() {
    let mut state = AppState::default();
    state
        .overlay_manager
        .push(vac_tui_runtime::overlay::OverlayId::AskUser, state.focus);

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::ShowProfileSwitcher);

    assert!(
        state
            .overlay_manager
            .is_active(vac_tui_runtime::overlay::OverlayId::AskUser)
    );
    assert!(
        !state
            .overlay_manager
            .is_active(vac_tui_runtime::overlay::OverlayId::ProfileSwitcher)
    );
}

#[test]
fn shortcuts_popup_swallows_input_without_touching_editor_state() {
    let mut state = AppState::default();
    state
        .overlay_manager
        .push(vac_tui_runtime::overlay::OverlayId::Shortcuts, state.focus);
    state.input.set_content("seed");

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::InputChanged('x'));

    assert!(
        state
            .overlay_manager
            .is_active(vac_tui_runtime::overlay::OverlayId::Shortcuts)
    );
    assert_eq!(state.input.get_content(), "seed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shortcuts_popup_executes_slash_commands_directly() {
    let mut state = AppState::default();
    state
        .overlay_manager
        .push(vac_tui_runtime::overlay::OverlayId::Shortcuts, state.focus);
    state.command_palette.shortcuts_mode = ShortcutsPopupMode::Commands;
    state.input.set_content("seed");

    let commands = shortcuts_popup::filter_commands("", &state);
    let model_idx = commands
        .iter()
        .position(|cmd| {
            matches!(
                &cmd.action,
                CommandAction::InsertSlashCommand(s) if s == "/model"
            )
        })
        .expect("/model should be present in shortcuts popup");
    state.command_palette.shortcuts_scroll = model_idx;

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::InputSubmitted);

    assert!(
        state
            .overlay_manager
            .is_active(vac_tui_runtime::overlay::OverlayId::ModelSwitcher)
    );
    assert!(
        !state
            .overlay_manager
            .is_active(vac_tui_runtime::overlay::OverlayId::Shortcuts)
    );
    assert_eq!(state.input.get_content(), "seed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shortcuts_popup_can_switch_tabs_without_closing() {
    let mut state = AppState::default();
    state
        .overlay_manager
        .push(vac_tui_runtime::overlay::OverlayId::Shortcuts, state.focus);
    state.command_palette.shortcuts_mode = ShortcutsPopupMode::Commands;
    state.command_palette.input = "stale filter".to_string();

    let commands = shortcuts_popup::filter_commands("", &state);
    let sessions_idx = commands
        .iter()
        .position(|cmd| matches!(&cmd.action, CommandAction::OpenSessions))
        .expect("Resume Session command should be present");
    state.command_palette.shortcuts_scroll = sessions_idx;

    let (output_tx, _output_rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &output_tx, InputEvent::InputSubmitted);

    assert!(
        state
            .overlay_manager
            .is_active(vac_tui_runtime::overlay::OverlayId::Shortcuts)
    );
    assert_eq!(state.command_palette.shortcuts_mode, ShortcutsPopupMode::Sessions);
    assert_eq!(state.command_palette.shortcuts_scroll, 0);
    assert!(state.command_palette.input.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn buffered_messages_flush_as_a_single_user_message() {
    let mut state = AppState::default();
    state
        .pending_user_messages
        .push_back(PendingUserMessage::new(
            "first".to_string(),
            None,
            vec![],
            "first".to_string(),
        ));
    state
        .pending_user_messages
        .push_back(PendingUserMessage::new(
            "second".to_string(),
            None,
            vec![],
            "second".to_string(),
        ));

    let (input_tx, mut input_rx) = mpsc::channel(8);
    let (output_tx, mut output_rx) = mpsc::channel(8);

    flush_pending_user_messages_if_idle(&mut state, &input_tx, &output_tx);

    match output_rx
        .recv()
        .await
        .expect("output event should be emitted")
    {
        OutputEvent::UserMessage(final_input, shell_calls, image_parts, revert_idx) => {
            assert_eq!(final_input, "first\n\nsecond");
            assert!(shell_calls.is_none());
            assert!(image_parts.is_empty());
            assert!(revert_idx.is_none());
        }
        other => panic!("unexpected output event: {other:?}"),
    }

    match input_rx
        .recv()
        .await
        .expect("input event should be emitted")
    {
        InputEvent::AddUserMessage(text) => assert_eq!(text, "first\n\nsecond"),
        other => panic!("unexpected input event: {other:?}"),
    }

    assert_eq!(state.queue_metrics.total_queued, 1);
    assert_eq!(state.queue_metrics.total_merged, 1);
    assert!(state.pending_user_messages.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_switcher_request_on_open_and_submit_is_deterministic() {
    let mut state = AppState::default();
    state.switchers.active_profile = "migration".to_string();

    let (output_tx, mut output_rx) = mpsc::channel(8);

    input_core::handle_input_event(&mut state, &output_tx, InputEvent::ShowProfileSwitcher);
    {
        let mut ctx = HandlerContext::new(&mut state, &output_tx);
        assert!(
            ctx.state
                .overlay_manager
                .is_active(vac_tui_runtime::overlay::OverlayId::ProfileSwitcher)
        );
        let filtered = ctx.state.profile_switcher_filtered();
        assert!(
            filtered.iter().any(|p| p == "migration"),
            "profile switcher should preselect from the active profile set"
        );
        assert_eq!(filtered[ctx.state.switchers.profile_selected], "migration");
        profile_switcher::submit_selected(&mut ctx).unwrap();
    }

    match output_rx.recv().await.unwrap() {
        OutputEvent::SwitchProfile(profile) => assert_eq!(profile, "migration"),
        other => panic!("unexpected output event: {other:?}"),
    }
    assert!(
        !state
            .overlay_manager
            .is_active(vac_tui_runtime::overlay::OverlayId::ProfileSwitcher)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rulebook_switcher_loads_disk_data_and_submits_active_rulebook() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join(".vac")).unwrap();
    fs::write(
        root.join(".vac/rules.toml"),
        r#"
id = "workspace"
name = "Workspace Rules"

[[constraints]]
id = "no-unwrap"
description = "Do not use unwrap in production code"
severity = "warn"
"#,
    )
    .unwrap();

    let mut state = AppState::new(AppStateOptions {
        model: None,
        session_id: Some(Uuid::new_v4().to_string()),
        checkpoint_path: None,
        project_root: root.to_path_buf(),
    });
    state.switchers.selected_rulebooks.insert("workspace".to_string());

    let (output_tx, mut output_rx) = mpsc::channel(8);

    input_core::handle_input_event(&mut state, &output_tx, InputEvent::ShowRulebookSwitcher);
    {
        let mut ctx = HandlerContext::new(&mut state, &output_tx);
        assert!(
            ctx.state
                .overlay_manager
                .is_active(vac_tui_runtime::overlay::OverlayId::RulebookSwitcher)
        );
        assert_eq!(ctx.state.switchers.available_rulebooks.len(), 1);
        assert_eq!(ctx.state.switchers.available_rulebooks[0].id, "workspace");
        assert_eq!(ctx.state.switchers.rulebook_selected, 0);
        rulebook_switcher::submit_selected(&mut ctx).unwrap();
    }

    match output_rx.recv().await.unwrap() {
        OutputEvent::ApplyRulebooks(selected) => {
            assert_eq!(selected, vec!["workspace".to_string()]);
        }
        other => panic!("unexpected output event: {other:?}"),
    }
    assert!(
        !state
            .overlay_manager
            .is_active(vac_tui_runtime::overlay::OverlayId::RulebookSwitcher)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sessions_tab_cleans_selected_session_artifacts_and_refreshes() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let session_id = Uuid::new_v4();
    let session_id_str = session_id.to_string();
    fs::create_dir_all(root.join(".vac/sessions")).unwrap();
    fs::create_dir_all(root.join(".vac/checkpoints")).unwrap();
    fs::create_dir_all(root.join(".vac/approvals")).unwrap();

    fs::write(
        root.join(".vac/sessions")
            .join(format!("{session_id_str}.snapshot.json")),
        "{\"schema_version\":1}",
    )
    .unwrap();
    fs::write(
        root.join(".vac/checkpoints")
            .join(format!("{session_id_str}_state.json")),
        "{}",
    )
    .unwrap();
    fs::write(
        root.join(".vac/approvals").join("approval.json"),
        format!("{{\"session_id\":\"{session_id_str}\"}}"),
    )
    .unwrap();

    let mut state = AppState::new(AppStateOptions {
        model: None,
        session_id: Some(Uuid::new_v4().to_string()),
        checkpoint_path: Some(root.join(".vac/checkpoints")),
        project_root: root.to_path_buf(),
    });
    state.focus = vac_tui_runtime::app::WorkspaceFocus::Workbench;
    state.workbench_tab = vac_tui_runtime::app::WorkbenchTab::Sessions;
    state.sessions = vec![SessionInfo {
        title: "Session cleanup".to_string(),
        id: session_id_str.clone(),
        updated_at: "2026-04-18T00:00:00Z".to_string(),
        checkpoints: vec!["checkpoint.json".to_string()],
        task_count: 1,
        last_activity: "2026-04-18 00:00".to_string(),
        has_checkpoint: true,
        snapshot_present: true,
        snapshot_stale: false,
    }];
    state.operator.sessions_selected_idx = 0;

    let (tx, mut rx) = mpsc::channel(8);
    input_core::handle_input_event(&mut state, &tx, InputEvent::InputChanged('d'));

    // Since PR-W25-9 the 'd' handler only dispatches `OutputEvent::CleanupSession`
    // (the actual filesystem work runs asynchronously in the runner task). The
    // test simulates that downstream step by invoking cleanup_session_async
    // directly, matching what runner::spawn_session_event_loop does.
    let cleanup_id = match rx.try_recv().unwrap() {
        OutputEvent::CleanupSession(id) => id,
        other => panic!("unexpected output event: {other:?}"),
    };
    assert_eq!(cleanup_id, session_id_str);

    let report = vac_session_control::cleanup_session_async(root.to_path_buf(), session_id)
        .await
        .expect("cleanup_session_async should succeed");
    assert!(report.snapshot_removed, "snapshot should be removed");
    assert!(report.checkpoint_removed, "checkpoint should be removed");
    assert_eq!(
        report.approvals_removed, 1,
        "one approval file should be removed"
    );
    assert!(
        report.errors.is_empty(),
        "cleanup should not report errors: {:?}",
        report.errors
    );

    assert!(
        !root
            .join(".vac/sessions")
            .join(format!("{session_id_str}.snapshot.json"))
            .exists()
    );
    assert!(
        !root
            .join(".vac/checkpoints")
            .join(format!("{session_id_str}_state.json"))
            .exists()
    );
    assert!(!root.join(".vac/approvals").join("approval.json").exists());
}

// =======================================================================
// PR-T16 P2 — E2E mouse-dispatch coverage per surface
//
// Each test drives `handlers::mouse::dispatch_click` from the vac_cli
// integration-test boundary (= public crate surface), seeding the region
// fields exactly as a view render pass would, and asserting that the
// observable AppState mutation matches the documented behaviour in
// docs/tui/action_matrix.md.
// =======================================================================

mod pr_t16_mouse_dispatch_e2e {
    use ratatui::layout::Rect;
    use tokio::sync::mpsc;
    use vac_tui_runtime::app::{AppState, OutputEvent, WorkbenchTab, WorkspaceFocus};
    use vac_tui_runtime::handlers::mouse::dispatch_click;

    fn make_state() -> (
        AppState,
        mpsc::Sender<OutputEvent>,
        mpsc::Receiver<OutputEvent>,
    ) {
        let state = AppState::default();
        let (tx, rx) = mpsc::channel::<OutputEvent>(64);
        (state, tx, rx)
    }

    #[test]
    fn review_row_click_selects_path_and_switches_tab() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Sessions;
        state
            .workbench_chrome.review_file_row_regions
            .push(("src/lib.rs".to_string(), Rect::new(2, 5, 40, 1)));
        state
            .workbench_chrome.review_file_row_regions
            .push(("src/main.rs".to_string(), Rect::new(2, 6, 40, 1)));

        let handled = dispatch_click(&mut state, &tx, 10, 6);
        assert!(handled, "click inside review row region must be consumed");
        assert_eq!(
            state.review.selected_path.as_deref(),
            Some("src/main.rs"),
            "clicked file should become the selected path"
        );
        assert_eq!(state.workbench_tab, WorkbenchTab::Review);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn approvals_row_click_selects_idx_and_switches_tab() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state
            .workbench_chrome.approvals_row_regions
            .push((0, Rect::new(4, 8, 30, 1)));
        state
            .workbench_chrome.approvals_row_regions
            .push((2, Rect::new(4, 10, 30, 1)));

        let handled = dispatch_click(&mut state, &tx, 5, 10);
        assert!(handled);
        assert_eq!(state.approvals.approval_selected_idx, 2);
        assert_eq!(state.workbench_tab, WorkbenchTab::Approvals);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn vil_issue_row_click_selects_and_switches_tab() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state
            .workbench_chrome.vil_issue_row_regions
            .push((3, Rect::new(2, 12, 60, 1)));

        let handled = dispatch_click(&mut state, &tx, 5, 12);
        assert!(handled);
        assert_eq!(state.vil.workbench_selected, 3);
        assert_eq!(state.workbench_tab, WorkbenchTab::Vil);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn workbench_body_click_grabs_focus_without_tab_switch() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Plan;
        state.workbench_chrome.body_region = Some(Rect::new(0, 5, 80, 20));

        let handled = dispatch_click(&mut state, &tx, 40, 15);
        assert!(handled);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
        // Body fallback must NOT silently change the active tab.
        assert_eq!(state.workbench_tab, WorkbenchTab::Plan);
    }

    #[test]
    fn sessions_row_click_selects_idx_and_switches_tab() {
        use vac_tui_runtime::app::SessionInfo;

        fn make_session(title: &str) -> SessionInfo {
            SessionInfo {
                title: title.to_string(),
                id: format!("id-{title}"),
                updated_at: String::new(),
                checkpoints: Vec::new(),
                task_count: 0,
                last_activity: String::new(),
                has_checkpoint: false,
                snapshot_present: false,
                snapshot_stale: false,
            }
        }

        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state.sessions = vec![
            make_session("alpha"),
            make_session("beta"),
            make_session("gamma"),
        ];
        state.operator.sessions_selected_idx = 0;
        state.workbench_chrome.sessions_row_regions.push((0, Rect::new(2, 5, 30, 1)));
        state.workbench_chrome.sessions_row_regions.push((1, Rect::new(2, 6, 30, 1)));
        state.workbench_chrome.sessions_row_regions.push((2, Rect::new(2, 7, 30, 1)));

        let handled = dispatch_click(&mut state, &tx, 10, 7);
        assert!(handled, "click inside sessions row region must be consumed");
        assert_eq!(state.operator.sessions_selected_idx, 2);
        assert_eq!(state.workbench_tab, WorkbenchTab::Sessions);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn sessions_row_click_out_of_bounds_is_ignored() {
        // Defence-in-depth: if view render pushed a stale region for an
        // index that no longer exists in state.sessions, the dispatcher
        // must not panic or mutate state.
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state.sessions.clear();
        state.workbench_chrome.sessions_row_regions.push((5, Rect::new(2, 5, 30, 1)));

        let handled = dispatch_click(&mut state, &tx, 10, 5);
        assert!(
            !handled,
            "stale sessions row region for missing session must be a no-op"
        );
        assert_eq!(state.operator.sessions_selected_idx, 0);
        assert_eq!(state.workbench_tab, WorkbenchTab::Review);
    }

    #[test]
    fn click_outside_all_regions_is_ignored() {
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state
            .workbench_chrome.review_file_row_regions
            .push(("a.rs".to_string(), Rect::new(0, 0, 10, 1)));
        state.workbench_chrome.body_region = Some(Rect::new(0, 5, 20, 5));

        let handled = dispatch_click(&mut state, &tx, 80, 40);
        assert!(!handled, "click outside every region must be a no-op");
        assert_eq!(state.focus, WorkspaceFocus::Input);
    }
}

// =======================================================================
// PR-T15 / R7 — hover popup dismissal behaviour.
//
// The hover popup is opened by clicking a VIL issue row (covered by
// `pr_t16_mouse_dispatch_e2e::vil_issue_row_click_selects_and_switches_tab`).
// These tests lock in the *dismiss* half of the contract documented in
// docs/tui/action_matrix.md: a click outside the popup rect clears it and
// is considered handled (no fall-through to underlying regions), while a
// click inside the popup leaves it in place.
// =======================================================================

mod pr_t15_hover_popup_e2e {
    use ratatui::layout::Rect;
    use tokio::sync::mpsc;
    use vac_core::lsp::types::LspSeverity;
    use vac_tui_runtime::app::{AppState, OutputEvent, WorkbenchTab, WorkspaceFocus};
    use vac_tui_runtime::handlers::mouse::dispatch_click;
    use vac_tui_runtime::services::diagnostics_overlay::HoverDetail;

    fn make_state() -> (
        AppState,
        mpsc::Sender<OutputEvent>,
        mpsc::Receiver<OutputEvent>,
    ) {
        let state = AppState::default();
        let (tx, rx) = mpsc::channel::<OutputEvent>(64);
        (state, tx, rx)
    }

    fn seeded_hover() -> HoverDetail {
        HoverDetail {
            severity: LspSeverity::Error,
            message: "unresolved import `foo`".to_string(),
            code: Some("E0432".to_string()),
            source: Some("rustc".to_string()),
            line_index: 41,
        }
    }

    #[test]
    fn click_outside_popup_dismisses_hover() {
        // Seed a visible hover popup anchored somewhere mid-screen, then
        // click well outside its rect. The dispatcher must:
        //   * clear active_hover + hover_popup_region,
        //   * return true (handled — swallowed, not fall-through),
        //   * NOT mutate focus / workbench_tab just because of the dismiss.
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Workbench;
        state.workbench_tab = WorkbenchTab::Vil;
        state.lsp_ui.active_hover = Some(seeded_hover());
        state.lsp_ui.hover_popup_region = Some(Rect::new(20, 10, 40, 7));

        // Click is well to the upper-left of the popup rect.
        let handled = dispatch_click(&mut state, &tx, 2, 2);
        assert!(
            handled,
            "click outside the popup must be consumed by the dismiss path"
        );
        assert!(
            state.lsp_ui.active_hover.is_none(),
            "active_hover must be cleared on outside click"
        );
        assert!(
            state.lsp_ui.hover_popup_region.is_none(),
            "hover_popup_region must be cleared alongside active_hover"
        );
        // Dismiss itself must not change the active tab. (Whether focus
        // stays put is incidental — we only lock in tab + popup state.)
        assert_eq!(state.workbench_tab, WorkbenchTab::Vil);
    }

    #[test]
    fn click_outside_popup_with_no_region_dismisses_hover() {
        // Defence-in-depth: if render did not refresh hover_popup_region
        // this frame (None), any click while a hover is active still counts
        // as "outside" and must dismiss the popup.
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Workbench;
        state.workbench_tab = WorkbenchTab::Vil;
        state.lsp_ui.active_hover = Some(seeded_hover());
        state.lsp_ui.hover_popup_region = None;

        let handled = dispatch_click(&mut state, &tx, 30, 15);
        assert!(handled);
        assert!(state.lsp_ui.active_hover.is_none());
        assert!(state.lsp_ui.hover_popup_region.is_none());
    }

    #[test]
    fn click_inside_popup_keeps_hover_and_falls_through() {
        // Clicks inside the popup rect are intentionally NOT swallowed by
        // the dismiss path; they should fall through and land on whatever
        // region sits underneath (here: nothing, so handled == false).
        // Crucially, active_hover must remain Some — the popup stays up.
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Workbench;
        state.workbench_tab = WorkbenchTab::Vil;
        state.lsp_ui.active_hover = Some(seeded_hover());
        state.lsp_ui.hover_popup_region = Some(Rect::new(20, 10, 40, 7));

        // Click squarely inside the popup rect.
        let handled = dispatch_click(&mut state, &tx, 30, 13);
        assert!(
            !handled,
            "inside-popup click falls through and, with no underlying region, is a no-op"
        );
        assert!(
            state.lsp_ui.active_hover.is_some(),
            "inside-popup click must NOT dismiss the hover"
        );
        assert_eq!(
            state.lsp_ui.hover_popup_region,
            Some(Rect::new(20, 10, 40, 7)),
            "hover_popup_region must be preserved across an inside click"
        );
    }

    #[test]
    fn click_without_active_hover_skips_dismiss_path() {
        // When no popup is up, the dismiss block is a pure no-op — clicks
        // must be routed normally to downstream regions (here: a VIL issue
        // row), exactly as in pr_t16_mouse_dispatch_e2e.
        let (mut state, tx, _rx) = make_state();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state.lsp_ui.active_hover = None;
        state.lsp_ui.hover_popup_region = None;
        state
            .workbench_chrome.vil_issue_row_regions
            .push((2, Rect::new(2, 12, 60, 1)));

        let handled = dispatch_click(&mut state, &tx, 5, 12);
        assert!(handled);
        assert_eq!(state.vil.workbench_selected, 2);
        assert_eq!(state.workbench_tab, WorkbenchTab::Vil);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }
}

// =======================================================================
// PR-T15 / R7 extension — real-hover dispatch behaviour.
//
// These tests lock in the move-based popup semantics documented in
// `handlers::mouse::dispatch_hover`:
//   * Moving over a VIL row populates / swaps active_hover and resets
//     hover_popup_region so the renderer re-anchors next frame.
//   * Moving over the current popup rect keeps the hover alive.
//   * Moving outside every tracked region clears active_hover +
//     hover_popup_region.
//   * Spurious moves (no change in target) return false so the runtime
//     does not paint needlessly.
//
// hover_detail_at itself is unit-tested in diagnostics_overlay; these E2E
// tests focus on the *dispatch* state machine, not the diag lookup.
// =======================================================================

mod pr_t15_hover_move_e2e {
    use ratatui::layout::Rect;
    use vac_core::lsp::types::LspSeverity;
    use vac_tui_runtime::app::{AppState, WorkbenchTab};
    use vac_tui_runtime::handlers::mouse::dispatch_hover;
    use vac_tui_runtime::services::diagnostics_overlay::HoverDetail;

    fn seeded_hover() -> HoverDetail {
        HoverDetail {
            severity: LspSeverity::Warning,
            message: "field is never read".to_string(),
            code: Some("dead_code".to_string()),
            source: Some("rustc".to_string()),
            line_index: 7,
        }
    }

    #[test]
    fn move_off_all_regions_dismisses_active_hover() {
        // A prior click or hover left active_hover + popup_region set.
        // Moving far away from every region must clear them and signal
        // "handled" so the runtime repaints the dismiss.
        let mut state = AppState::default();
        state.workbench_tab = WorkbenchTab::Vil;
        state.lsp_ui.active_hover = Some(seeded_hover());
        state.lsp_ui.hover_popup_region = Some(Rect::new(20, 10, 40, 7));

        let changed = dispatch_hover(&mut state, 2, 2);
        assert!(
            changed,
            "off-region move must be reported as a state change"
        );
        assert!(state.lsp_ui.active_hover.is_none());
        assert!(state.lsp_ui.hover_popup_region.is_none());
    }

    #[test]
    fn move_over_popup_keeps_hover_alive() {
        // Mousing into the popup body (to read it) must not dismiss it.
        // Since nothing changed, dispatch_hover must return false — no
        // repaint is needed.
        let mut state = AppState::default();
        state.workbench_tab = WorkbenchTab::Vil;
        state.lsp_ui.active_hover = Some(seeded_hover());
        state.lsp_ui.hover_popup_region = Some(Rect::new(20, 10, 40, 7));

        let changed = dispatch_hover(&mut state, 30, 13);
        assert!(!changed, "hover over popup must be a no-op repaint-wise");
        assert!(state.lsp_ui.active_hover.is_some(), "hover must stay alive");
        assert_eq!(state.lsp_ui.hover_popup_region, Some(Rect::new(20, 10, 40, 7)));
    }

    #[test]
    fn move_over_vil_row_with_no_lsp_snapshot_clears_stale_hover() {
        // Hovering a row but with no lsp_diagnostics snapshot produces
        // new_hover = None. If we previously had a hover, it must flip
        // to None (reported as a change); popup_region resets too.
        let mut state = AppState::default();
        state.workbench_tab = WorkbenchTab::Vil;
        state.lsp_ui.active_hover = Some(seeded_hover());
        state.lsp_ui.hover_popup_region = Some(Rect::new(20, 10, 40, 7));
        state
            .workbench_chrome.vil_issue_row_regions
            .push((0, Rect::new(2, 20, 60, 1)));
        assert!(
            state.lsp_ui.lsp_diagnostics.is_none(),
            "precondition: no LSP snapshot seeded"
        );

        let changed = dispatch_hover(&mut state, 5, 20);
        assert!(changed, "stale hover must be cleared on empty-row hover");
        assert!(state.lsp_ui.active_hover.is_none());
        assert!(state.lsp_ui.hover_popup_region.is_none());
    }

    #[test]
    fn move_outside_with_no_prior_hover_is_noop() {
        // No hover to dismiss, nothing to populate — dispatch_hover must
        // report false so the runtime elides the repaint entirely.
        let mut state = AppState::default();
        state.workbench_tab = WorkbenchTab::Review;
        // No regions, no hover, no popup.

        let changed = dispatch_hover(&mut state, 50, 50);
        assert!(!changed);
        assert!(state.lsp_ui.active_hover.is_none());
        assert!(state.lsp_ui.hover_popup_region.is_none());
    }

    #[test]
    fn hover_dispatch_does_not_mutate_workbench_selected() {
        // Defence: hovering a different row (say idx 3) must NOT bump the
        // keyboard selection cursor (vil.workbench_selected). Hover is
        // read-only w.r.t. selection; only click / j-k should move it.
        let mut state = AppState::default();
        state.vil.workbench_selected = 1;
        state
            .workbench_chrome.vil_issue_row_regions
            .push((3, Rect::new(2, 30, 60, 1)));

        let _ = dispatch_hover(&mut state, 5, 30);
        assert_eq!(
            state.vil.workbench_selected, 1,
            "hover must not shift keyboard selection"
        );
    }
}
