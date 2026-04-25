//! Slice 11 — overlay stack behaviour proofs.

use vac_shell_contracts::{OverlayIntent, ShellOverlay};
use vac_shell_overlay::OverlayStack;

#[test]
fn empty_stack_top_is_none() {
    let s = OverlayStack::new();
    assert_eq!(s.top(), ShellOverlay::None);
    assert!(s.is_empty());
}

#[test]
fn esc_closes_top_overlay_only() {
    let s = OverlayStack::new();
    s.apply_intent(OverlayIntent::Open(ShellOverlay::Palette));
    s.apply_intent(OverlayIntent::Open(ShellOverlay::ModelSwitcher));
    assert_eq!(s.top(), ShellOverlay::ModelSwitcher);
    s.apply_intent(OverlayIntent::CloseTop);
    assert_eq!(s.top(), ShellOverlay::Palette);
    s.apply_intent(OverlayIntent::CloseTop);
    assert_eq!(s.top(), ShellOverlay::None);
}

#[test]
fn toggle_same_overlay_closes_it() {
    let s = OverlayStack::new();
    s.apply_intent(OverlayIntent::Toggle(ShellOverlay::Palette));
    assert_eq!(s.top(), ShellOverlay::Palette);
    s.apply_intent(OverlayIntent::Toggle(ShellOverlay::Palette));
    assert_eq!(s.top(), ShellOverlay::None);
}

#[test]
fn opening_model_switcher_pushes_above_palette() {
    let s = OverlayStack::new();
    s.apply_intent(OverlayIntent::Open(ShellOverlay::Palette));
    s.apply_intent(OverlayIntent::Open(ShellOverlay::ModelSwitcher));
    assert_eq!(
        s.snapshot(),
        vec![ShellOverlay::Palette, ShellOverlay::ModelSwitcher]
    );
}

#[test]
fn open_same_overlay_is_idempotent() {
    let s = OverlayStack::new();
    s.apply_intent(OverlayIntent::Open(ShellOverlay::Palette));
    s.apply_intent(OverlayIntent::Open(ShellOverlay::Palette));
    assert_eq!(s.snapshot(), vec![ShellOverlay::Palette]);
}

#[test]
fn close_all_drops_every_overlay() {
    let s = OverlayStack::new();
    s.apply_intent(OverlayIntent::Open(ShellOverlay::Palette));
    s.apply_intent(OverlayIntent::Open(ShellOverlay::Shortcuts));
    s.apply_intent(OverlayIntent::Open(ShellOverlay::ModelSwitcher));
    s.apply_intent(OverlayIntent::CloseAll);
    assert!(s.is_empty());
}

#[test]
fn open_none_is_a_noop() {
    let s = OverlayStack::new();
    s.apply_intent(OverlayIntent::Open(ShellOverlay::None));
    assert!(s.is_empty());
    s.apply_intent(OverlayIntent::Toggle(ShellOverlay::None));
    assert!(s.is_empty());
}
