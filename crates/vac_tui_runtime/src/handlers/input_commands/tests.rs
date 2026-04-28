use super::*;
use crate::action_registry::spec_by_slash_alias;
use crate::app::{AppState, AppStateOptions};

/// Every BuiltIn slash command in the registry must have a spec in ACTION_SPECS.
/// This test locks the command map — additions require updating ACTION_SPECS.
#[test]
fn slash_dispatch_matches_palette() {
    let state = AppState::new(AppStateOptions {
        model: None,
        session_id: None,
        checkpoint_path: None,
        project_root: std::env::current_dir().unwrap(),
    });
    let builtin_commands: Vec<&str> = state
        .layout
        .commands
        .iter()
        .filter(|c| c.source == crate::app::CommandSource::BuiltIn)
        .map(|c| c.command.as_str())
        .collect();

    let mut unregistered: Vec<&str> = Vec::new();
    for cmd in &builtin_commands {
        if spec_by_slash_alias(cmd).is_none() {
            unregistered.push(cmd);
        }
    }
    assert!(
        unregistered.is_empty(),
        "BuiltIn commands missing from ACTION_SPECS: {:?}",
        unregistered
    );
}

#[test]
fn unknown_slash_shows_suggestions() {
    let mut state = AppState::new(AppStateOptions {
        model: None,
        session_id: None,
        checkpoint_path: None,
        project_root: std::env::current_dir().unwrap(),
    });
    let before = state.transcript.messages.len();
    // "/shelll" is one char off from "/shell" — should get a suggestion
    show_unknown_slash_suggestions(&mut state, "/shelll");
    assert!(
        state.transcript.messages.len() > before,
        "should add a suggestion message"
    );
    let content = &state.transcript.messages.last().unwrap().content;
    assert!(
        content.contains("/shell"),
        "suggestion should include /shell, got: {content}"
    );
}

#[test]
fn unknown_slash_prefix_matches() {
    let mut state = AppState::new(AppStateOptions {
        model: None,
        session_id: None,
        checkpoint_path: None,
        project_root: std::env::current_dir().unwrap(),
    });
    // "/she" is a prefix of "/shell" — should get a suggestion
    show_unknown_slash_suggestions(&mut state, "/she");
    let content = &state.transcript.messages.last().unwrap().content;
    assert!(
        content.contains("/shell"),
        "prefix match should suggest /shell, got: {content}"
    );
}

#[test]
fn truly_unknown_slash_shows_help_hint() {
    let mut state = AppState::new(AppStateOptions {
        model: None,
        session_id: None,
        checkpoint_path: None,
        project_root: std::env::current_dir().unwrap(),
    });
    show_unknown_slash_suggestions(&mut state, "/xyzzy_nomatch_at_all");
    let content = &state.transcript.messages.last().unwrap().content;
    assert!(
        content.contains("/help"),
        "no match should suggest /help, got: {content}"
    );
}
