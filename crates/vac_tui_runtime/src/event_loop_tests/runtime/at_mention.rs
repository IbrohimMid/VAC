#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::make_state;
use crate::app::InputEvent;

#[test]
fn at_trigger_activates_on_at_char() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.layout.focus = crate::app::WorkspaceFocus::Input;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));

    assert!(state.composer.at_mention.trigger_active);
    assert!(state.composer.at_mention.query.is_empty());
}

#[test]
fn at_trigger_updates_query_on_subsequent_chars() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.layout.focus = crate::app::WorkspaceFocus::Input;

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('s'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('c'));

    assert!(state.composer.at_mention.trigger_active);
    assert_eq!(state.composer.at_mention.query, "src");
}

#[test]
fn at_trigger_deactivates_on_esc() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.layout.focus = crate::app::WorkspaceFocus::Input;
    state.composer.at_mention.trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.composer.at_mention.query = "src".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);

    assert!(!state.composer.at_mention.trigger_active);
    assert!(state.composer.at_mention.query.is_empty());
}

#[test]
fn at_trigger_deactivates_on_space() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.layout.focus = crate::app::WorkspaceFocus::Input;
    state.composer.at_mention.trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.composer.at_mention.query = "src".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged(' '));

    assert!(!state.composer.at_mention.trigger_active);
}

#[test]
fn at_trigger_backspace_pops_query() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.layout.focus = crate::app::WorkspaceFocus::Input;
    state.composer.at_mention.trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.composer.at_mention.query = "sr".to_string();

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
    assert_eq!(state.composer.at_mention.query, "s");
    assert!(state.composer.at_mention.trigger_active);

    // Backspace on empty query deactivates
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace); // removes '@'
    assert!(!state.composer.at_mention.trigger_active);
}

#[test]
fn at_trigger_enter_creates_context_chip() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
    state.layout.focus = crate::app::WorkspaceFocus::Input;
    state.composer.at_mention.trigger_active = true;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
    state.composer.at_mention.query = "src".to_string();
    state.composer.at_mention.results = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
    state.composer.at_mention.selected_idx = 0;
    // Simulate @src already in input
    state.composer.input.insert_str("@src");

    crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

    // PR-T7: @mention now creates a context chip instead of inserting into input buffer.
    assert!(!state.composer.at_mention.trigger_active);
    assert!(state.composer.at_mention.query.is_empty());
    assert_eq!(state.composer.context_chips.len(), 1);
    assert_eq!(state.composer.context_chips[0].label, "main.rs");
    // Input buffer should be cleared of the @src token
    let content = state.composer.input.get_content();
    assert!(!content.contains("@src"));
}
