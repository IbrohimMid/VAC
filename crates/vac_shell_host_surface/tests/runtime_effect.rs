//! Step 2 slice 2 — first real VAC effect, end-to-end proof.
//!
//! Reviewer's acceptance path: palette → registry → bridge → effect,
//! where the effect is a real VAC-owned state change. Asserts that
//! pressing Enter on `/runtime` in the palette flips
//! `SurfaceState::current()` from `Chat` to `Runtime`, and that
//! `/chat` flips it back.

use std::sync::Arc;

use vac_shell_bridge::{
    CommandDispatcher, InMemoryCommandRegistry, SurfaceController, surface_dispatcher,
};
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, VacCommandRegistry};
use vac_shell_host_surface::{Surface, SurfaceState, SurfaceStateController};
use vac_shell_palette::{PaletteEvent, PaletteKey, PaletteViewState, on_key};

fn fixture() -> Vec<ShellCommandSpec> {
    vec![
        ShellCommandSpec {
            id: "runtime".into(),
            slash: "/runtime".into(),
            title: "Runtime".into(),
            description: "Open runtime surface".into(),
            kind: ShellCommandKind::BuiltInAction,
            palette_visible: true,
            shortcut: None,
        },
        ShellCommandSpec {
            id: "chat".into(),
            slash: "/chat".into(),
            title: "Chat".into(),
            description: "Return to conversation surface".into(),
            kind: ShellCommandKind::BuiltInAction,
            palette_visible: true,
            shortcut: None,
        },
    ]
}

fn build() -> (CommandDispatcher, SurfaceState) {
    let registry: Arc<dyn VacCommandRegistry> = Arc::new(InMemoryCommandRegistry::new(fixture()));
    let state = SurfaceState::new(Surface::Chat);
    let controller = Arc::new(SurfaceStateController::new(state.clone()));
    let handler = surface_dispatcher(controller as Arc<dyn SurfaceController>);
    (CommandDispatcher::new(registry, handler), state)
}

#[test]
fn slash_runtime_flips_surface_through_full_chain() {
    let (dispatcher, state) = build();
    assert_eq!(state.current(), Surface::Chat);

    let mut view = PaletteViewState::new(dispatcher.registry().all());
    view.visible = true;

    // Operator types `/r` then Enter — `/runtime` is the only match.
    on_key(&mut view, PaletteKey::Char('/'));
    on_key(&mut view, PaletteKey::Char('r'));
    let event = on_key(&mut view, PaletteKey::Enter);
    let slash = match event {
        PaletteEvent::Selected(s) => s,
        other => panic!("expected Selected, got {other:?}"),
    };
    assert_eq!(slash, "/runtime");

    let spec = dispatcher.dispatch(&slash).expect("dispatch must succeed");
    assert_eq!(spec.id, "runtime");
    assert_eq!(state.current(), Surface::Runtime, "real VAC effect must land");
}

#[test]
fn slash_chat_flips_back() {
    let (dispatcher, state) = build();
    dispatcher.dispatch("/runtime").unwrap();
    assert_eq!(state.current(), Surface::Runtime);
    dispatcher.dispatch("/chat").unwrap();
    assert_eq!(state.current(), Surface::Chat);
}

#[test]
fn unknown_slash_does_not_mutate_surface() {
    let (dispatcher, state) = build();
    let _ = dispatcher.dispatch("/never");
    assert_eq!(state.current(), Surface::Chat, "surface must not move");
}
