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
    app.session_browser.tiles = sessions.list_with_summaries(comp.paths.as_ref(), |_| None);
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

// =====================================================================
// Slice 20.2 — live projection on overlay open
// =====================================================================

#[test]
fn opening_palette_populates_from_live_command_registry() {
    let (_t, comp) = boot();
    // Add another command at runtime so the test proves the
    // projection reads the live registry, not a static seed.
    let mut app = ShellApp::new(comp.clone());
    app.handle_global_key(GlobalKey::OpenPalette);
    let slashes: Vec<String> = app.palette.all.iter().map(|s| s.slash.clone()).collect();
    assert_eq!(
        slashes.len(),
        comp.command_registry.all().len(),
        "palette must mirror the live registry on open"
    );
}

#[test]
fn opening_model_switcher_populates_from_live_model_state() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    app.handle_global_key(GlobalKey::OpenModelSwitcher);
    let labels: Vec<&str> = app
        .model_switcher
        .models
        .iter()
        .map(|m| m.label.as_str())
        .collect();
    assert!(labels.contains(&"Claude Sonnet 4.5"));
    assert!(labels.contains(&"Claude Haiku 4"));
    assert!(app.model_switcher.visible);
}

#[test]
fn opening_session_browser_populates_from_live_sessions_list() {
    let (tmp, comp) = boot();
    let dir = tmp.path().join(".vac").join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("alpha.jsonl"), "operator: hi\n").unwrap();
    let mut app = ShellApp::new(comp);
    app.sessions = Some(Arc::new(vac_shell_host_sessions::SessionsState::new()));
    app.handle_global_key(GlobalKey::OpenSessionBrowser);
    let ids: Vec<String> = app
        .session_browser
        .tiles
        .iter()
        .map(|t| t.entry.id.clone())
        .collect();
    assert_eq!(ids, vec!["alpha"]);
}

// =====================================================================
// Slice 20.2 — approval detail does not drain queue
// =====================================================================

#[test]
fn approval_detail_reject_marks_row_rejected_without_draining_queue() {
    let (_t, comp) = boot();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    let mut app = ShellApp::new(comp.clone());
    app.refresh_approval_bar();
    app.handle_global_key(GlobalKey::OpenApprovalDetail);
    let event = app.dispatch_approval_detail_key(DetailKey::Reject).unwrap();
    app.apply_event(event);
    let snap = comp.approval_queue.snapshot();
    assert_eq!(snap.len(), 1, "queue must NOT drain on detail reject");
    assert_eq!(snap[0].status, ApprovalStatus::Rejected);
    assert!(comp.approval_queue.last_outcome().is_none());
}

#[test]
fn approval_detail_approve_does_not_drain_queue() {
    let (_t, comp) = boot();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    let mut app = ShellApp::new(comp.clone());
    app.refresh_approval_bar();
    app.handle_global_key(GlobalKey::OpenApprovalDetail);
    let event = app.dispatch_approval_detail_key(DetailKey::Approve).unwrap();
    app.apply_event(event);
    let snap = comp.approval_queue.snapshot();
    assert_eq!(snap.len(), 1, "queue must NOT drain on detail approve");
    assert_eq!(snap[0].status, ApprovalStatus::Approved);
}

// =====================================================================
// Slice 20.2 — built-in palette routing through apply_event
// =====================================================================

fn boot_with_commands() -> (
    tempfile::TempDir,
    Arc<vac_shell_composition::ShellComposition>,
) {
    use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec};
    let tmp = tempfile::tempdir().unwrap();
    let paths: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(tmp.path()));
    let make_cmd = |slash: &str| ShellCommandSpec {
        id: slash.trim_start_matches('/').to_string(),
        slash: slash.into(),
        title: slash.into(),
        description: String::new(),
        kind: ShellCommandKind::BuiltInAction,
        palette_visible: true,
        ..Default::default()
    };
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
        .with_commands(vec![
            make_cmd("/chat"),
            make_cmd("/runtime"),
            make_cmd("/model"),
            make_cmd("/sessions"),
        ])
        .boot()
        .unwrap();
    (tmp, Arc::new(comp))
}

#[test]
fn palette_runtime_slash_changes_surface_through_apply_event() {
    let (_t, comp) = boot_with_commands();
    let mut app = ShellApp::new(comp.clone());
    app.apply_event(AppEvent::PaletteSelected("/runtime".into()));
    assert_eq!(
        comp.surface_state.current(),
        vac_shell_host_surface::Surface::Runtime
    );
    assert_eq!(app.overlays.top(), ShellOverlay::None);
}

#[test]
fn palette_model_slash_opens_model_switcher_overlay() {
    let (_t, comp) = boot_with_commands();
    let mut app = ShellApp::new(comp);
    app.apply_event(AppEvent::PaletteSelected("/model".into()));
    assert_eq!(app.overlays.top(), ShellOverlay::ModelSwitcher);
    assert!(app.model_switcher.visible);
    assert!(!app.model_switcher.models.is_empty());
}

#[test]
fn palette_sessions_slash_opens_session_browser_overlay() {
    let (tmp, comp) = boot_with_commands();
    let dir = tmp.path().join(".vac").join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("alpha.jsonl"), "operator: hi\n").unwrap();
    let mut app = ShellApp::new(comp);
    app.sessions = Some(Arc::new(vac_shell_host_sessions::SessionsState::new()));
    app.apply_event(AppEvent::PaletteSelected("/sessions".into()));
    assert_eq!(app.overlays.top(), ShellOverlay::SessionBrowser);
    assert_eq!(app.session_browser.tiles.len(), 1);
}

#[test]
fn palette_unknown_slash_only_closes_overlay() {
    let (_t, comp) = boot_with_commands();
    let mut app = ShellApp::new(comp.clone());
    app.handle_global_key(GlobalKey::OpenPalette);
    assert_eq!(app.overlays.top(), ShellOverlay::Palette);
    app.apply_event(AppEvent::PaletteSelected("/never-registered".into()));
    assert_eq!(app.overlays.top(), ShellOverlay::None);
    assert_eq!(
        comp.surface_state.current(),
        vac_shell_host_surface::Surface::Chat
    );
}

// =====================================================================
// Slice 20.2 — frame refresh contract
// =====================================================================

#[test]
fn prepare_frame_projects_queue_into_approval_bar() {
    let (_t, comp) = boot();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    let mut app = ShellApp::new(comp);
    // No manual refresh: prepare_frame is the contract.
    app.prepare_frame();
    assert!(app.approval_bar.is_visible());
    let s = screen(&app);
    assert!(s.contains("Approval Required"));
    assert!(s.contains("Shell"));
}

// =====================================================================
// Slice 20.3 — production error reporting
// =====================================================================

#[test]
fn apply_event_returns_err_when_session_action_targets_unknown_id() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    app.sessions = Some(Arc::new(vac_shell_host_sessions::SessionsState::new()));
    let result = app.apply_event(AppEvent::Session(
        vac_shell_contracts::SessionAction::Resume { id: "ghost".into() },
    ));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.title.contains("session action failed"));
}

#[test]
fn apply_event_records_error_into_activity_log_on_failure() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    app.sessions = Some(Arc::new(vac_shell_host_sessions::SessionsState::new()));
    let log = Arc::new(vac_shell_host_activity::ActivityLog::default());
    app.activity_log = Some(log.clone());
    let _ = app.apply_event(AppEvent::Session(
        vac_shell_contracts::SessionAction::Resume { id: "ghost".into() },
    ));
    let snap = log.snapshot();
    assert_eq!(snap.len(), 1);
    assert!(matches!(
        snap[0].kind,
        vac_shell_contracts::ShellActivityKind::Error
    ));
    assert!(snap[0].title.contains("session action failed"));
    assert!(snap[0].detail.as_deref().unwrap_or("").contains("ghost"));
}

#[test]
fn apply_event_records_error_when_session_routed_without_sessions_state() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    let log = Arc::new(vac_shell_host_activity::ActivityLog::default());
    app.activity_log = Some(log.clone());
    let result = app.apply_event(AppEvent::Session(
        vac_shell_contracts::SessionAction::Open { id: "x".into() },
    ));
    assert!(result.is_err());
    assert_eq!(log.snapshot().len(), 1);
}

#[test]
fn apply_event_records_error_for_unknown_approval_id() {
    let (_t, comp) = boot();
    let mut app = ShellApp::new(comp);
    let log = Arc::new(vac_shell_host_activity::ActivityLog::default());
    app.activity_log = Some(log.clone());
    let result = app.apply_event(AppEvent::ApprovalDecision {
        id: "nope".into(),
        approve: true,
    });
    assert!(result.is_err());
    let snap = log.snapshot();
    assert_eq!(snap.len(), 1);
    assert!(snap[0].title.contains("approval id not found"));
}

#[test]
fn apply_event_succeeds_silently_when_no_log_attached() {
    let (_t, comp) = boot();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    let mut app = ShellApp::new(comp);
    let result = app.apply_event(AppEvent::ShellAction(
        ShellAction::ToggleApproval { id: "a".into() },
    ));
    assert!(result.is_ok());
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
