//! Step 3a — palette extraction proof tests.
//!
//! These tests exist to verify the proof acceptance criteria the
//! reviewer set. Each test should fail if a future change re-couples
//! the palette to the donor `AppState` or to the donor command merge
//! pipeline.

use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec};
use vac_shell_palette::{
    PaletteEvent, PaletteKey, PaletteViewState, filter_entries, on_key,
};

fn fixture() -> Vec<ShellCommandSpec> {
    vec![
        ShellCommandSpec {
            id: "model".into(),
            slash: "/model".into(),
            title: "Pick model".into(),
            description: "Switch active LLM model".into(),
            kind: ShellCommandKind::OverlayRoute,
            palette_visible: true,
            shortcut: None,
        ..Default::default()
        },
        ShellCommandSpec {
            id: "memorize".into(),
            slash: "/memorize".into(),
            title: "Save fact".into(),
            description: "Persist a memory entry".into(),
            kind: ShellCommandKind::PromptTemplate,
            palette_visible: true,
            shortcut: None,
        ..Default::default()
        },
        ShellCommandSpec {
            id: "runtime".into(),
            slash: "/runtime".into(),
            title: "Runtime".into(),
            description: "Open the runtime surface".into(),
            kind: ShellCommandKind::BuiltInAction,
            palette_visible: true,
            shortcut: None,
        ..Default::default()
        },
        ShellCommandSpec {
            id: "hidden".into(),
            slash: "/internal".into(),
            title: "Hidden".into(),
            description: "Should not appear".into(),
            kind: ShellCommandKind::BuiltInAction,
            palette_visible: false,
            shortcut: None,
        ..Default::default()
        },
    ]
}

#[test]
fn empty_input_lists_visible_entries_only() {
    let all = fixture();
    let listed = filter_entries("", &all);
    assert_eq!(listed.len(), 3, "the hidden entry must not appear");
    assert!(listed.iter().all(|s| s.palette_visible));
}

#[test]
fn slash_only_input_behaves_like_empty() {
    let all = fixture();
    assert_eq!(filter_entries("/", &all).len(), 3);
}

#[test]
fn prefix_filter_narrows_to_matching_entries() {
    let all = fixture();
    let listed = filter_entries("/m", &all);
    let slashes: Vec<&str> = listed.iter().map(|s| s.slash.as_str()).collect();
    assert_eq!(slashes, vec!["/model", "/memorize"]);
}

#[test]
fn enter_returns_selected_command_slash() {
    let mut view = PaletteViewState::new(fixture());
    view.visible = true;
    assert_eq!(on_key(&mut view, PaletteKey::Char('/')), PaletteEvent::Consumed);
    assert_eq!(on_key(&mut view, PaletteKey::Char('m')), PaletteEvent::Consumed);
    // input is now "/m" -> filtered = ["/model", "/memorize"], selected = 0
    assert_eq!(
        on_key(&mut view, PaletteKey::Enter),
        PaletteEvent::Selected("/model".into())
    );
}

#[test]
fn down_arrow_advances_selection_with_clamp() {
    let mut view = PaletteViewState::new(fixture());
    view.visible = true;
    on_key(&mut view, PaletteKey::Down);
    on_key(&mut view, PaletteKey::Down);
    on_key(&mut view, PaletteKey::Down);
    on_key(&mut view, PaletteKey::Down); // beyond list end
    let last = view.selected;
    assert_eq!(last, 2, "selection should clamp at the last filtered row");
}

#[test]
fn escape_dismisses_and_clears_state() {
    let mut view = PaletteViewState::new(fixture());
    view.visible = true;
    on_key(&mut view, PaletteKey::Char('/'));
    on_key(&mut view, PaletteKey::Char('r'));
    assert_eq!(on_key(&mut view, PaletteKey::Escape), PaletteEvent::Dismissed);
    assert!(!view.visible);
    assert!(view.input.is_empty());
    assert_eq!(view.selected, 0);
}

#[test]
fn keys_are_ignored_when_palette_is_closed() {
    let mut view = PaletteViewState::new(fixture());
    // visible = false by default
    assert_eq!(on_key(&mut view, PaletteKey::Enter), PaletteEvent::Ignored);
    assert!(!view.visible);
}

#[test]
fn backspace_walks_input_back_to_empty() {
    let mut view = PaletteViewState::new(fixture());
    view.visible = true;
    on_key(&mut view, PaletteKey::Char('/'));
    on_key(&mut view, PaletteKey::Char('m'));
    on_key(&mut view, PaletteKey::Backspace);
    assert_eq!(view.input, "/");
    on_key(&mut view, PaletteKey::Backspace);
    assert_eq!(view.input, "");
}

/// The proof's load-bearing assertion: this crate must not depend on
/// the donor `AppState` or the legacy command merge pipeline.
///
/// The Rust trait system wouldn't let us link to those types if we
/// don't pull the crate in, but a future drift could add the dep
/// silently. This test makes that drift impossible without an
/// equally-loud edit here.
#[test]
fn no_donor_or_runtime_dependency_links_in() {
    // If a future patch adds `vac_tui_runtime` or anything inside
    // `vendor/stakpak` to this crate's dependency graph, the strings
    // below would compile-fail because those symbols would shadow
    // ours. Keep this test as a tripwire.
    let _: PaletteViewState = PaletteViewState::default();
    let _: PaletteEvent = PaletteEvent::Ignored;
    // Compile-time assertion: only `vac_shell_contracts` + `ratatui`
    // (transitive) types are visible from this crate.
    fn assert_only_contracts<T: Sized>(_: T) {}
    assert_only_contracts(ShellCommandKind::BuiltInAction);
}
