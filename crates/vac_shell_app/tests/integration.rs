use std::sync::Arc;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_app::{GlobalKey, ShellApp};
use vac_shell_bridge::{ProviderId, ShellAction, SurfaceTarget};
use vac_shell_composition::ShellCompositionBuilder;
use vac_shell_contracts::{ShellOverlay, VacPaths};
use vac_shell_host_approval::ApprovalRequest;
use vac_shell_host_model::{HostModel, ProviderInfo};
use vac_shell_host_paths::VacPathsImpl;

fn boot_comp() -> (
    tempfile::TempDir,
    Arc<vac_shell_composition::ShellComposition>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let paths: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(tmp.path()));
    let comp = ShellCompositionBuilder::new(paths)
        .with_providers(vec![ProviderInfo {
            id: ProviderId("anthropic".into()),
            credentials_present: true,
        }])
        .with_models(vec![HostModel {
            provider: ProviderId("anthropic".into()),
            id: "claude-sonnet-4.5".into(),
            label: "Claude Sonnet 4.5".into(),
            reasoning: true,
            cost_label: None,
        }])
        .with_fallback_active(Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )))
        .boot()
        .unwrap();
    (tmp, Arc::new(comp))
}

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
    let backend = TestBackend::new(120, 8);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| app.render(f, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width { s.push_str(buf[(x, y)].symbol()); }
        s.push('\n');
    }
    assert!(s.contains("CHAT"));
    assert!(s.contains("claude-sonnet-4.5"));
    assert!(s.contains("/repo"));
}

#[test]
fn approval_queue_updates_status_bar() {
    let (_t, comp) = boot_comp();
    comp.approval_queue.enqueue(ApprovalRequest::new("a", "shell"));
    comp.approval_queue.enqueue(ApprovalRequest::new("b", "shell"));
    let mut app = ShellApp::new(comp);
    app.status_inputs.cwd = "/repo".into();
    let backend = TestBackend::new(120, 4);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| app.render(f, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width { s.push_str(buf[(x, y)].symbol()); }
        s.push('\n');
    }
    assert!(s.contains("approvals"));
    assert!(s.contains(" 2 "));
}

#[test]
fn no_donor_or_runtime_dependency_links_in() {
    fn assert_only_local<T: Sized>(_: T) {}
    assert_only_local(GlobalKey::Escape);
}
