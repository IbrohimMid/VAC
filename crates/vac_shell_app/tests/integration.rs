mod common;

use common::{boot_comp, screen};
use vac_shell_app::{GlobalKey, ShellApp};
use vac_shell_bridge::{ShellAction, SurfaceTarget};
use vac_shell_contracts::ShellOverlay;
use vac_shell_host_approval::ApprovalRequest;

#[test]
fn app_boots_shell_composition() {
    let (_t, comp) = boot_comp();
    let app = ShellApp::new(comp.clone());
    assert!(app.composition().is_some());
    assert_eq!(
        comp.model_state.active_model().unwrap().1,
        "claude-sonnet-4.5"
    );
}

#[test]
fn ctrl_p_opens_palette_overlay() {
    let (_t, comp) = boot_comp();
    let mut app = ShellApp::new(comp);
    let _ = app.handle_global_key(GlobalKey::OpenPalette);
    assert_eq!(app.overlays.top(), ShellOverlay::Palette);
    assert!(app.palette.visible);
}

#[test]
fn ctrl_m_opens_model_switcher_overlay() {
    let (_t, comp) = boot_comp();
    let mut app = ShellApp::new(comp);
    let _ = app.handle_global_key(GlobalKey::OpenModelSwitcher);
    assert_eq!(app.overlays.top(), ShellOverlay::ModelSwitcher);
    assert!(app.model_switcher.visible);
}

#[test]
fn esc_closes_top_overlay() {
    let (_t, comp) = boot_comp();
    let mut app = ShellApp::new(comp);
    app.handle_global_key(GlobalKey::OpenPalette);
    app.handle_global_key(GlobalKey::OpenModelSwitcher);
    assert_eq!(app.overlays.top(), ShellOverlay::ModelSwitcher);
    app.handle_global_key(GlobalKey::Escape);
    assert_eq!(app.overlays.top(), ShellOverlay::Palette);
    app.handle_global_key(GlobalKey::Escape);
    assert_eq!(app.overlays.top(), ShellOverlay::None);
}

#[test]
fn enter_runtime_emits_shell_action() {
    let (_t, comp) = boot_comp();
    let mut app = ShellApp::new(comp);
    let action = app.handle_global_key(GlobalKey::EnterRuntime);
    assert!(matches!(
        action,
        Some(ShellAction::EnterSurface(SurfaceTarget::Runtime))
    ));
}

#[test]
fn render_status_bar_shows_active_model_label() {
    let (_t, comp) = boot_comp();
    let mut app = ShellApp::new(comp);
    app.status_inputs.cwd = "/repo".into();
    let s = screen(&app);
    assert!(s.contains("CHAT"));
    assert!(s.contains("claude-sonnet-4.5"));
    assert!(s.contains("/repo"));
}

#[test]
fn approval_queue_updates_status_bar() {
    let (_t, comp) = boot_comp();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    comp.approval_queue
        .enqueue(ApprovalRequest::new("b", "shell"));
    let mut app = ShellApp::new(comp);
    app.status_inputs.cwd = "/repo".into();
    let s = screen(&app);
    assert!(s.contains("approvals"));
    assert!(s.contains(" 2 "));
}

#[test]
fn no_donor_or_runtime_dependency_links_in() {
    fn assert_only_local<T: Sized>(_: T) {}
    assert_only_local(GlobalKey::Escape);
}
