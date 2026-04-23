#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::make_state;
use crate::app::InputEvent;

#[test]
fn review_selection_normalizes_when_filter_excludes_selected() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state
        .workspace.changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), false);
    state
        .workspace.changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), false);
    state.workspace.modified_files = state.workspace.changeset_store.modified_files();
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.workspace.review.selected_path = Some("b.txt".to_string());
    state.workspace.review.filter = "a".to_string();
    state.review_sync_items();
    state.review_normalize_selection();
    assert_eq!(state.workspace.review.selected_path, Some("a.txt".to_string()));
    assert_eq!(state.workspace.review.selected_idx, 0);
}

#[test]
fn review_open_close_transitions_clear_diff() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
    state.workspace.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.txt".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 0,
        last_error: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewClose);
    assert!(!state.workspace.review.open);
    assert!(state.workspace.review.diff.is_none());
}

#[test]
fn diff_scroll_state_changes_on_page_down() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
    state.workspace.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.txt".to_string(),
        old_content: Some("a\nb\nc\nd\ne\n".to_string()),
        new_content: Some("a\nb\nX\nd\ne\n".to_string()),
        scroll: 0,
        last_error: None,
    });
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageDown);
    assert!(state.workspace.review.diff.as_ref().unwrap().scroll > 0);
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
        .workspace.changeset_store
        .file_modified(file_rel.to_string(), "agent".to_string(), true);
    state.workspace.modified_files = state.workspace.changeset_store.modified_files();
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
    state.review_sync_items();
    state.workspace.review.selected_path = Some(file_rel.to_string());

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertSelected);

    let content = std::fs::read_to_string(root.join(file_rel)).unwrap();
    assert_eq!(content, "old");
    assert!(!state.workspace.modified_files.contains(&file_rel.to_string()));
    let it = state.workspace.review.items.get(file_rel).unwrap();
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
        .workspace.changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), true);
    state
        .workspace.changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), true);
    state.workspace.modified_files = state.workspace.changeset_store.modified_files();
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
    state.workspace.review.filter = "a".to_string();
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
    assert!(!state.workspace.modified_files.contains(&"a.txt".to_string()));
    assert!(state.workspace.modified_files.contains(&"b.txt".to_string()));
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
        .workspace.changeset_store
        .file_modified("a.txt".to_string(), "agent".to_string(), true);
    state
        .workspace.changeset_store
        .file_modified("b.txt".to_string(), "agent".to_string(), true);
    state.workspace.modified_files = state.workspace.changeset_store.modified_files();
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
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
    assert!(state.workspace.modified_files.is_empty());
    assert_eq!(
        state.workspace.review.items.get("a.txt").unwrap().status,
        crate::app::ReviewItemStatus::Restored
    );
    assert_eq!(
        state.workspace.review.items.get("b.txt").unwrap().status,
        crate::app::ReviewItemStatus::Restored
    );
}

#[test]
fn modified_files_is_derived_from_changeset_store() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    state
        .workspace.changeset_store
        .file_modified("a.rs".to_string(), "agent".to_string(), true);
    state
        .workspace.changeset_store
        .file_created("b.rs".to_string(), "agent".to_string());
    state.workspace.modified_files = state.workspace.changeset_store.modified_files();

    // modified_files must equal store's derived view
    assert_eq!(state.workspace.modified_files, state.workspace.changeset_store.modified_files());
    assert_eq!(state.workspace.modified_files.len(), 2);
}

#[test]
fn review_open_event_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewOpen);

    assert!(state.workspace.review.open);
    assert_eq!(state.layout.workbench_tab, crate::app::WorkbenchTab::Review);
    assert_eq!(state.layout.focus, crate::app::WorkspaceFocus::Workbench);
}

#[test]
fn review_close_via_esc_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewClose);

    assert!(!state.workspace.review.open);
    assert!(state.workspace.review.diff.is_none());
}

#[test]
fn review_filter_push_pop_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('a'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('b'));
    assert_eq!(state.workspace.review.filter, "ab");

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterBackspace);
    assert_eq!(state.workspace.review.filter, "a");
}

#[test]
fn review_toggle_diff_clears_when_same_path() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
    state.workspace.review.selected_path = Some("a.rs".to_string());
    state.workspace.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.rs".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 0,
        last_error: None,
    });

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewToggleDiff);

    assert!(state.workspace.review.diff.is_none());
}

#[test]
fn review_scroll_routes_via_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.workspace.review.open = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
    state.layout.focus = crate::app::WorkspaceFocus::Workbench;
    state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
    state.workspace.review.diff = Some(crate::app::ReviewDiffState {
        path: "a.rs".to_string(),
        old_content: Some("old".to_string()),
        new_content: Some("new".to_string()),
        scroll: 5,
        last_error: None,
    });

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageUp);
    assert_eq!(state.workspace.review.diff.as_ref().unwrap().scroll, 0);

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageDown);
    assert!(state.workspace.review.diff.as_ref().unwrap().scroll > 0);
}
