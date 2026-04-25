//! Step 2 acceptance proof.
//!
//! Reviewer-required integration test for the
//! `palette → registry → bridge → effect` path. Builds a real
//! `InMemoryCommandRegistry`, feeds its specs into the
//! `vac_shell_palette` view state, simulates an Enter-on-selection,
//! and asserts the dispatch callback fired against the right spec.
//!
//! The host effect is captured into an `Arc<Mutex<Vec<…>>>` so the
//! test asserts identity (which slash dispatched), not just that
//! something happened.

use std::sync::{Arc, Mutex};

use vac_shell_bridge::{
    CommandDispatcher, DispatchError, DispatchHandler, InMemoryCommandRegistry,
};
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, VacCommandRegistry};
use vac_shell_palette::{PaletteEvent, PaletteKey, PaletteViewState, on_key};

fn fixture() -> Vec<ShellCommandSpec> {
    vec![
        ShellCommandSpec {
            id: "model".into(),
            slash: "/model".into(),
            title: "Model picker".into(),
            description: "Switch active model".into(),
            kind: ShellCommandKind::OverlayRoute,
            palette_visible: true,
            shortcut: None,
        },
        ShellCommandSpec {
            id: "memorize".into(),
            slash: "/memorize".into(),
            title: "Memorize".into(),
            description: "Save a fact".into(),
            kind: ShellCommandKind::PromptTemplate,
            palette_visible: true,
            shortcut: None,
        },
        ShellCommandSpec {
            id: "runtime".into(),
            slash: "/runtime".into(),
            title: "Runtime".into(),
            description: "Open runtime surface".into(),
            kind: ShellCommandKind::BuiltInAction,
            palette_visible: true,
            shortcut: None,
        },
    ]
}

fn build_dispatcher() -> (CommandDispatcher, Arc<Mutex<Vec<String>>>) {
    let registry: Arc<dyn VacCommandRegistry> = Arc::new(InMemoryCommandRegistry::new(fixture()));
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log_for_handler = Arc::clone(&log);
    let handler: DispatchHandler = Arc::new(move |spec| {
        log_for_handler.lock().unwrap().push(spec.id.clone());
        Ok(())
    });
    (CommandDispatcher::new(registry, handler), log)
}

#[test]
fn palette_enter_drives_bridge_dispatch() {
    let (dispatcher, log) = build_dispatcher();

    // Seed the palette directly from the registry — same source of
    // truth, no donor merge layer.
    let mut view = PaletteViewState::new(dispatcher.registry().all());
    view.visible = true;

    // Operator types `/m` then Enter. With strict prefix filter the
    // top of the filtered list is `/memorize` after typing past the
    // `/m` overlap with `/model` — but with bare `/m` both match,
    // and `/model` sorts first by registry order. We use one Down to
    // hit `/memorize` and confirm the bridge resolves the slash that
    // *the operator selected*, not the input string.
    assert_eq!(on_key(&mut view, PaletteKey::Char('/')), PaletteEvent::Consumed);
    assert_eq!(on_key(&mut view, PaletteKey::Char('m')), PaletteEvent::Consumed);
    assert_eq!(on_key(&mut view, PaletteKey::Down), PaletteEvent::Consumed);
    let event = on_key(&mut view, PaletteKey::Enter);

    let chosen = match event {
        PaletteEvent::Selected(slash) => slash,
        other => panic!("expected Selected, got {other:?}"),
    };
    assert_eq!(chosen, "/memorize");

    let spec = dispatcher.dispatch(&chosen).expect("dispatch must succeed");
    assert_eq!(spec.id, "memorize");
    assert_eq!(spec.kind, ShellCommandKind::PromptTemplate);

    let recorded = log.lock().unwrap();
    assert_eq!(recorded.as_slice(), &["memorize".to_string()]);
}

#[test]
fn unknown_slash_propagates_unknownslash_error() {
    let (dispatcher, log) = build_dispatcher();
    let err = dispatcher.dispatch("/never-registered").unwrap_err();
    assert_eq!(err, DispatchError::UnknownSlash("/never-registered".into()));
    assert!(log.lock().unwrap().is_empty(), "handler must not fire");
}

#[test]
fn host_handler_failure_is_surfaced_as_host_error() {
    let registry: Arc<dyn VacCommandRegistry> = Arc::new(InMemoryCommandRegistry::new(fixture()));
    let handler: DispatchHandler = Arc::new(|_| Err(DispatchError::Host("boom".into())));
    let dispatcher = CommandDispatcher::new(registry, handler);
    let err = dispatcher.dispatch("/runtime").unwrap_err();
    assert_eq!(err, DispatchError::Host("boom".into()));
}
