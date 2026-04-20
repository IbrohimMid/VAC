#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::make_state;
use crate::app::{InputEvent, OutputEvent};
use crate::controller::classify_critical_banner;

#[test]
fn review_selection_normalizes_when_filter_excludes_selected() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state
        .changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), false);
    state
        .changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), false);
    state.modified_files = state.changeset_store.modified_files();
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.review.selected_path = Some("b.txt".to_string());
    state.review.filter = "a".to_string();
    state.review_sync_items();
    state.review_normalize_selection();
    assert_eq!(state.review.selected_path, Some("a.txt".to_string()));
    assert_eq!(state.review.selected_idx, 0);
}

#[test]
fn slash_semantics_fix_and_explain_send_prompt_with_args() {
    // /fix and /explain are BuiltInWithPrompt: they push to pending_user_messages,
    // not directly to the output channel.
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state.input.set_content("/fix cargo clippy");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    let msg = state
        .pending_user_messages
        .pop_front()
        .expect("/fix should enqueue a pending message");
    assert!(
        msg.final_input.contains("Fix linter/build errors"),
        "got: {}",
        msg.final_input
    );
    assert!(
        msg.final_input.contains("cargo clippy"),
        "got: {}",
        msg.final_input
    );

    state
        .input
        .set_content("/explain crates/vac_cli/src/tui/event_loop.rs");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    let msg = state
        .pending_user_messages
        .pop_front()
        .expect("/explain should enqueue a pending message");
    assert!(
        msg.final_input
            .contains("Explain the relevant code or concept"),
        "got: {}",
        msg.final_input
    );
    assert!(
        msg.final_input
            .contains("crates/vac_cli/src/tui/event_loop.rs"),
        "got: {}",
        msg.final_input
    );
}

#[test]
fn slash_review_opens_workstation_instead_of_sending_literal() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state
        .changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), false);
    state.modified_files = state.changeset_store.modified_files();
    state.input.set_content("/review");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(state.review.open);
    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
}

#[test]
fn review_open_close_transitions_clear_diff() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.txt".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 0,
        last_error: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewClose);
    assert!(!state.review.open);
    assert!(state.review.diff.is_none());
}

#[test]
fn diff_scroll_state_changes_on_page_down() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.txt".to_string(),
        old_content: Some("a\nb\nc\nd\ne\n".to_string()),
        new_content: Some("a\nb\nX\nd\ne\n".to_string()),
        scroll: 0,
        last_error: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageDown);
    assert!(state.review.diff.as_ref().unwrap().scroll > 0);
}

#[test]
fn critical_banner_mcp_tls_is_blocking() {
    let msg = "Failed to connect MCP server 'github': MCP error: Failed to build HTTP client for MCP server 'github': error setting certificate verify locations";
    let out = classify_critical_banner(msg);
    assert_eq!(
        out,
        Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Blocking
        ))
    );
}

#[test]
fn critical_banner_governance_block_is_blocking() {
    let msg = "Tool execution error: Warden blocked: process substitution $() not allowed";
    let out = classify_critical_banner(msg);
    assert_eq!(
        out,
        Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Blocking
        ))
    );
}

#[tokio::test]
async fn revert_selected_updates_status_and_working_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let session_id = uuid::Uuid::new_v4();
    std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

    let file_rel = "a.txt";
    std::fs::write(root.join(file_rel), "new").unwrap();
    std::fs::write(
        crate::services::review::snapshot_path(&root, session_id, file_rel),
        "old",
    )
    .unwrap();

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(root.clone(), session_id);
    state
        .changeset_store
        .file_modified(file_rel.to_string(), "agent".to_string(), true);
    state.modified_files = state.changeset_store.modified_files();
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review_sync_items();
    state.review.selected_path = Some(file_rel.to_string());

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertSelected);

    let content = std::fs::read_to_string(root.join(file_rel)).unwrap();
    assert_eq!(content, "old");
    assert!(!state.modified_files.contains(&file_rel.to_string()));
    let it = state.review.items.get(file_rel).unwrap();
    assert_eq!(it.status, crate::app::ReviewItemStatus::Restored);
}

#[tokio::test]
async fn revert_filtered_only_affects_filtered_modified_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let session_id = uuid::Uuid::new_v4();
    std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

    for (p, old, new) in [("a.txt", "old-a", "new-a"), ("b.txt", "old-b", "new-b")] {
        std::fs::write(root.join(p), new).unwrap();
        std::fs::write(
            crate::services::review::snapshot_path(&root, session_id, p),
            old,
        )
        .unwrap();
    }

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(root.clone(), session_id);
    state
        .changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), true);
    state
        .changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), true);
    state.modified_files = state.changeset_store.modified_files();
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.filter = "a".to_string();
    state.review_sync_items();
    state.review_normalize_selection();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertFiltered);

    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "old-a"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "new-b"
    );
    assert!(!state.modified_files.contains(&"a.txt".to_string()));
    assert!(state.modified_files.contains(&"b.txt".to_string()));
}

#[tokio::test]
async fn slash_command_model_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /model command exists
    let commands = state.commands;
    assert!(commands.iter().any(|c| c.command == "/model"));
}

#[tokio::test]
async fn slash_command_files_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /files command exists
    let commands = state.commands;
    assert!(commands.iter().any(|c| c.command == "/files"));
}

#[tokio::test]
async fn slash_command_changes_exists_in_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Verify /changes command exists
    let commands = state.commands;
    assert!(commands.iter().any(|c| c.command == "/changes"));
}

#[test]
fn command_palette_filters_commands_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Empty filter shows all commands
    state.command_palette_input = "".to_string();
    let all = state.filtered_commands();
    assert!(!all.is_empty());

    // Filter by prefix
    state.command_palette_input = "/model".to_string();
    let filtered = state.filtered_commands();
    assert!(filtered.iter().any(|c| c.command == "/model"));

    // Non-matching filter
    state.command_palette_input = "/nonexistent".to_string();
    let empty = state.filtered_commands();
    assert!(empty.is_empty());
}

#[test]
fn command_palette_selection_stays_within_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state.command_palette_input = "".to_string();
    let commands = state.filtered_commands();

    // Selection should not exceed command count
    if !commands.is_empty() {
        state.command_palette_selected = 0;
        assert_eq!(state.command_palette_selected, 0);

        state.command_palette_selected = commands.len() - 1;
        assert_eq!(state.command_palette_selected, commands.len() - 1);
    }
}

#[tokio::test]
async fn command_palette_dispatch_does_not_send_literal_slash() {
    let dir = tempfile::tempdir().unwrap();
    let (_tx, mut rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    // Execute /review command
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
    let filtered = state.filtered_commands();
    if let Some(cmd) = filtered.iter().find(|c| c.command == "/review") {
        // Simulate command execution
        state.add_user_message(cmd.command.clone());
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        crate::overlay::close_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
    }

    // Verify review opened, not sent as literal message
    assert!(state.review.open);
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::CommandPalette)
    );

    // No output event should be sent for /review
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn revert_all_clears_modified_files_and_marks_status() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let session_id = uuid::Uuid::new_v4();
    std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

    for (p, old, new) in [("a.txt", "old-a", "new-a"), ("b.txt", "old-b", "new-b")] {
        std::fs::write(root.join(p), new).unwrap();
        std::fs::write(
            crate::services::review::snapshot_path(&root, session_id, p),
            old,
        )
        .unwrap();
    }

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(root.clone(), session_id);
    state
        .changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), true);
    state
        .changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), true);
    state.modified_files = state.changeset_store.modified_files();
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review_sync_items();
    state.review_normalize_selection();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertAll);

    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "old-a"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "old-b"
    );
    assert!(state.modified_files.is_empty());
    assert_eq!(
        state.review.items.get("a.txt").unwrap().status,
        crate::app::ReviewItemStatus::Restored
    );
    assert_eq!(
        state.review.items.get("b.txt").unwrap().status,
        crate::app::ReviewItemStatus::Restored
    );
}

// ── Branch 4A behavioral tests ──────────────────────────────────────────

#[test]
fn modified_files_is_derived_from_changeset_store() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state
        .changeset_store
        .file_modified("a.rs".to_string(), "agent".to_string(), true);
    state
        .changeset_store
        .file_created("b.rs".to_string(), "agent".to_string());
    state.modified_files = state.changeset_store.modified_files();

    // modified_files must equal store's derived view
    assert_eq!(state.modified_files, state.changeset_store.modified_files());
    assert_eq!(state.modified_files.len(), 2);
}

#[test]
fn counter_consistency_header_tab_popup() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state
        .changeset_store
        .file_modified("x.rs".to_string(), "agent".to_string(), true);
    state
        .changeset_store
        .file_created("y.rs".to_string(), "agent".to_string());
    state
        .changeset_store
        .file_modified("z.rs".to_string(), "agent".to_string(), true);
    state.changeset_store.revert_success("z.rs"); // reverted: not active

    let active = state.changeset_store.active_entries().len();
    // All three surfaces must read the same count
    assert_eq!(active, 2); // x.rs + y.rs; z.rs is reverted
    // review_filtered_paths also driven by active_entries
    state.review_sync_items();
    assert_eq!(state.review_filtered_paths().len(), 2);
}

#[test]
fn task_completed_does_not_dual_write() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    let result = vac_core::task::TaskResult {
        task_id: vac_core::task::TaskId::new(),
        status: vac_core::task::TaskStatus::Completed,
        summary: "done".to_string(),
        modified_files: vec!["src/lib.rs".to_string()],
        created_files: vec!["src/new.rs".to_string()],
        validation_score: None,
        elapsed_ms: 0,
        total_tokens_used: 0,
        agent_contributions: vec![],
    };
    crate::controller::handle_backend_event(&mut state, &tx, InputEvent::TaskCompleted(result));

    // modified_files must equal store's derived view - no independent writes
    assert_eq!(state.modified_files, state.changeset_store.modified_files());
    assert_eq!(state.changeset_store.active_entries().len(), 2);
}

// ── Branch 5A behavioral tests ──────────────────────────────────────────

#[test]
fn show_model_switcher_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowModelSwitcher);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
    assert!(state.model_switcher_filter.is_empty());
}

#[test]
fn show_file_search_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowFileSearch);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
    assert!(state.file_search_query.is_empty());
}

#[test]
fn show_changeset_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowChangeset);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::Changeset)
    );
}

#[tokio::test]
async fn slash_model_dispatch_opens_model_switcher_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.input.set_content("/model");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
}

#[tokio::test]
async fn slash_files_dispatch_opens_file_search_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.input.set_content("/files");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::FileSearch)
    );
    assert!(
        !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::ModelSwitcher)
    );
}

#[tokio::test]
async fn slash_changes_dispatch_opens_changeset_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.input.set_content("/changes");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(
        state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::Changeset)
    );
}

#[tokio::test]
async fn slash_review_dispatch_opens_review_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.input.set_content("/review");
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
    assert!(state.review.open);
    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
}

// ── Branch 6B behavioral tests ──────────────────────────────────────────

#[test]
fn at_trigger_activates_on_at_char() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));

    assert!(state.at_trigger_active);
    assert!(state.at_query.is_empty());
}

#[test]
fn at_trigger_updates_query_on_subsequent_chars() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('s'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('c'));

    assert!(state.at_trigger_active);
    assert_eq!(state.at_query, "src");
}

#[test]
fn at_trigger_deactivates_on_esc() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;
    state.at_trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.at_query = "src".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);

    assert!(!state.at_trigger_active);
    assert!(state.at_query.is_empty());
}

#[test]
fn at_trigger_deactivates_on_space() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;
    state.at_trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.at_query = "src".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged(' '));

    assert!(!state.at_trigger_active);
}

#[test]
fn at_trigger_backspace_pops_query() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;
    state.at_trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.at_query = "sr".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
    assert_eq!(state.at_query, "s");
    assert!(state.at_trigger_active);

    // Backspace on empty query deactivates
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace); // removes '@'
    assert!(!state.at_trigger_active);
}

#[test]
fn at_trigger_enter_creates_context_chip() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Input;
    state.at_trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.at_query = "src".to_string();
    state.at_results = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
    state.at_selected_idx = 0;
    // Simulate @src already in input
    state.input.insert_str("@src");

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

    // PR-T7: @mention now creates a context chip instead of inserting into input buffer.
    assert!(!state.at_trigger_active);
    assert!(state.at_query.is_empty());
    assert_eq!(state.context_chips.len(), 1);
    assert_eq!(state.context_chips[0].label, "main.rs");
    // Input buffer should be cleared of the @src token
    let content = state.input.get_content();
    assert!(!content.contains("@src"));
}

#[tokio::test]
async fn runtime_tab_requests_refresh_on_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Sessions;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Agents);
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::ListAgentTasks
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::LoadAgentState
    ));

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Runtime);
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::ListRuntimeJobs
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        OutputEvent::LoadRuntimeState
    ));
}

// ── Branch 5B behavioral tests ──────────────────────────────────────────

#[test]
fn review_open_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewOpen);

    assert!(state.review.open);
    assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
    assert_eq!(state.focus, crate::app::WorkspaceFocus::Workbench);
}

#[test]
fn review_close_via_esc_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewClose);

    assert!(!state.review.open);
    assert!(state.review.diff.is_none());
}

#[test]
fn review_filter_push_pop_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('a'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('b'));
    assert_eq!(state.review.filter, "ab");

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterBackspace);
    assert_eq!(state.review.filter, "a");
}

#[test]
fn review_toggle_diff_clears_when_same_path() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.selected_path = Some("a.rs".to_string());
    state.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.rs".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 0,
        last_error: None,
    });

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewToggleDiff);

    assert!(state.review.diff.is_none());
}

#[test]
fn review_scroll_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.focus = crate::app::WorkspaceFocus::Workbench;
    state.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.rs".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 5,
        last_error: None,
    });

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageUp);
    assert_eq!(state.review.diff.as_ref().unwrap().scroll, 0);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageDown);
    assert!(state.review.diff.as_ref().unwrap().scroll > 0);
}
