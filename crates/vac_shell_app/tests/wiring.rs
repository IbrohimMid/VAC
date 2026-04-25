//! Slice 20.1 — ShellApp wiring proofs.

use std::sync::Arc;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_app::{AppEvent, GlobalKey, ShellApp};
use vac_shell_approval_bar::{ApprovalBarKey, ApprovalStatus};
use vac_shell_approval_detail::DetailKey;
use vac_shell_bridge::{ProviderId, ShellAction, SurfaceTarget};
use vac_shell_composition::ShellCompositionBuilder;
use vac_shell_contracts::{
    DiffFileView, DiffHunkView, DiffLineKind, DiffLineView, DiffReviewEvent, ShellOverlay,
    VacPaths,
};
use vac_shell_diff_view::DiffReviewKey;
use vac_shell_host_approval::ApprovalRequest;
use vac_shell_host_model::{HostModel, ProviderInfo};
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_model_switcher::SwitcherKey;
use vac_shell_palette::PaletteKey;
use vac_shell_session_browser::SessionBrowserKey;
use vac_shell_shortcuts::default_shortcuts;

fn boot() -> (
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
        .with_models(vec![
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-sonnet-4.5".into(),
                label: "Claude Sonnet 4.5".into(),
                reasoning: true,
                cost_label: None,
            },
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-haiku-4".into(),
                label: "Claude Haiku 4".into(),
                reasoning: false,
                cost_label: None,
            },
        ])
        .with_fallback_active(Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )))
        .boot()
        .unwrap();
    (tmp, Arc::new(comp))
}

fn screen(app: &ShellApp) -> String {
    let backend = TestBackend::new(120, 18);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| app.render(f, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width { s.push_str(buf[(x, y)].symbol()); }
        s.push('\n');
    }
    s
}

#[test]
fn shell_popup_overlay_lights_up_on_global_key() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    app.shell_popup.visible = false;
    app.handle_global_key(GlobalKey::OpenShellPopup);
    assert_eq!(app.overlays.top(), ShellOverlay::ShellPopup);
    assert!(app.shell_popup.visible);
}

#[test]
fn approval_detail_overlay_renders_selected_approval() {
    let (_t, comp) = boot();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    let mut app = ShellApp::new(comp);
    app.refresh_approval_bar();
    app.handle_global_key(GlobalKey::OpenApprovalDetail);
    assert_eq!(app.overlays.top(), ShellOverlay::ApprovalDetail);
    assert!(app.approval_detail.detail.is_some());
    let s = screen(&app);
    assert!(s.contains("Approval Detail"));
    assert!(s.contains("Shell"));
}

#[test]
fn approval_queue_renders_approval_bar_in_shell_app() {
    let (_t, comp) = boot();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "run_command"));
    comp.approval_queue
        .enqueue(ApprovalRequest::new("b", "create"));
    let mut app = ShellApp::new(comp);
    app.refresh_approval_bar();
    let s = screen(&app);
    assert!(s.contains("Approval Required"));
    assert!(s.contains("Run Command"));
    assert!(s.contains("Create"));
}

#[test]
fn model_switcher_enter_routes_to_shell_action_select_model() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp.clone());
    // Open the switcher and seed its view with the live model list.
    app.handle_global_key(GlobalKey::OpenModelSwitcher);
    app.model_switcher.models = vec![
        vac_shell_contracts::VacModelView {
            provider: ProviderId("anthropic".into()),
            id: "claude-haiku-4".into(),
            label: "Claude Haiku 4".into(),
            active: false,
            credentials_present: true,
            reasoning: false,
            cost_label: None,
        },
    ];
    app.model_switcher.selected = 0;
    let event = app.dispatch_model_switcher_key(SwitcherKey::Enter).unwrap();
    match event.clone() {
        AppEvent::ShellAction(ShellAction::SelectModel { provider, id }) => {
            assert_eq!(provider, ProviderId("anthropic".into()));
            assert_eq!(id, "claude-haiku-4");
        }
        other => panic!("expected SelectModel, got {other:?}"),
    }
    // Apply through the host — active model should flip.
    app.apply_event(event);
    assert_eq!(
        comp.model_state.active_model().unwrap().1,
        "claude-haiku-4"
    );
}

#[test]
fn session_browser_delete_routes_to_sessions_state_after_confirm() {
    let (tmp, comp) = boot();
    // Seed a transcript so SessionsState::apply finds it.
    let dir = tmp.path().join(".vac").join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("alpha.jsonl"), "operator: hi\n").unwrap();

    let sessions = Arc::new(vac_shell_host_sessions::SessionsState::new());
    let mut app = ShellApp::new(comp.clone());
    app.sessions = Some(sessions.clone());
    app.handle_global_key(GlobalKey::OpenSessionBrowser);
    app.session_browser.entries = sessions.list(comp.paths.as_ref());
    app.session_browser.visible = true;

    // First Delete primes; second commits.
    let primed = app.dispatch_session_browser_key(SessionBrowserKey::Delete);
    assert!(primed.is_none(), "first Delete must not emit an action");
    let committed = app
        .dispatch_session_browser_key(SessionBrowserKey::Delete)
        .unwrap();
    if let AppEvent::Session(action) = committed.clone() {
        app.apply_event(AppEvent::Session(action));
    } else {
        panic!("expected Session AppEvent, got {committed:?}");
    }
    assert!(sessions.deleted_ids().contains(&"alpha".to_string()));
}

#[test]
fn diff_review_approve_routes_event() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    app.diff_review.visible = true;
    app.diff_review.files = vec![DiffFileView {
        path: "a.rs".into(),
        added: 1,
        removed: 0,
        hunks: vec![DiffHunkView {
            header: "@@".into(),
            lines: vec![DiffLineView {
                kind: DiffLineKind::Added,
                text: "fn hi() {}".into(),
            }],
        }],
    }];
    let event = app.dispatch_diff_review_key(DiffReviewKey::Approve).unwrap();
    assert_eq!(
        event,
        AppEvent::DiffReview(DiffReviewEvent::ApproveFile("a.rs".into()))
    );
}

#[test]
fn approval_detail_approve_routes_controller() {
    let (_t, comp) = boot();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    let mut app = ShellApp::new(comp.clone());
    app.refresh_approval_bar();
    app.handle_global_key(GlobalKey::OpenApprovalDetail);
    let event = app.dispatch_approval_detail_key(DetailKey::Approve).unwrap();
    match &event {
        AppEvent::ApprovalDecision { id, approve } => {
            assert_eq!(id, "a");
            assert!(*approve);
        }
        other => panic!("expected ApprovalDecision, got {other:?}"),
    }
    app.apply_event(event);
    let snap = comp.approval_queue.snapshot();
    assert_eq!(snap[0].status, ApprovalStatus::Approved);
}

#[test]
fn palette_enter_emits_palette_selected_app_event() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    app.handle_global_key(GlobalKey::OpenPalette);
    app.palette = vac_shell_palette::PaletteViewState::new(vec![
        vac_shell_contracts::ShellCommandSpec {
            id: "runtime".into(),
            slash: "/runtime".into(),
            title: "Runtime".into(),
            description: "Open runtime".into(),
            kind: vac_shell_contracts::ShellCommandKind::BuiltInAction,
            palette_visible: true,
            ..Default::default()
        },
    ]);
    app.palette.visible = true;
    app.dispatch_palette_key(PaletteKey::Char('/'));
    let event = app.dispatch_palette_key(PaletteKey::Enter).unwrap();
    assert_eq!(event, AppEvent::PaletteSelected("/runtime".into()));
}

#[test]
fn shortcuts_overlay_renders_default_catalogue() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    app.shortcuts = vac_shell_shortcuts::ShortcutsView::new(vec![], default_shortcuts());
    app.handle_global_key(GlobalKey::OpenShortcuts);
    let s = screen(&app);
    assert!(s.contains("Command Palette"));
    assert!(s.contains("Shortcuts"));
}

#[test]
fn pending_approvals_count_reflects_queue_length() {
    let (_t, comp) = boot();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    comp.approval_queue
        .enqueue(ApprovalRequest::new("b", "shell"));
    let mut app = ShellApp::new(comp);
    app.status_inputs.cwd = "/repo".into();
    let s = screen(&app);
    assert!(s.contains("approvals"));
    assert!(s.contains(" 2 "), "expected '2' approvals badge in: {s}");
}
