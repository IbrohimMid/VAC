#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::make_state;
use crate::app::InputEvent;

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
