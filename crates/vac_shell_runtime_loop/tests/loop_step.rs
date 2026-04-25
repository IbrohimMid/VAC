//! D2.2 — `handle_key_event_once` proofs.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use vac_shell_app::ShellApp;
use vac_shell_bridge::{ProviderId, SurfaceTarget};
use vac_shell_composition::ShellCompositionBuilder;
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, ShellOverlay, VacPaths};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_approval::ApprovalRequest;
use vac_shell_host_model::{HostModel, ProviderInfo};
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_runtime_loop::{
    ShellLoopOptions, handle_key_event_once, is_quit_key,
};

fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: mods,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ctrl(c: char) -> KeyEvent {
    k(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn plain(code: KeyCode) -> KeyEvent {
    k(code, KeyModifiers::NONE)
}

fn cmd(slash: &str) -> ShellCommandSpec {
    ShellCommandSpec {
        id: slash.trim_start_matches('/').into(),
        slash: slash.into(),
        title: slash.into(),
        description: String::new(),
        kind: ShellCommandKind::BuiltInAction,
        palette_visible: true,
        ..Default::default()
    }
}

fn boot_app() -> (tempfile::TempDir, ShellApp) {
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
        .with_commands(vec![cmd("/runtime"), cmd("/chat")])
        .boot()
        .unwrap();
    (tmp, ShellApp::new(Arc::new(comp)))
}

#[test]
fn handle_ctrl_p_opens_palette() {
    let (_t, mut app) = boot_app();
    handle_key_event_once(&mut app, ctrl('p')).unwrap();
    assert_eq!(app.overlays.top(), ShellOverlay::Palette);
}

#[test]
fn handle_ctrl_m_opens_model_switcher() {
    let (_t, mut app) = boot_app();
    handle_key_event_once(&mut app, ctrl('m')).unwrap();
    assert_eq!(app.overlays.top(), ShellOverlay::ModelSwitcher);
    assert!(!app.model_switcher.models.is_empty());
}

#[test]
fn handle_palette_runtime_changes_surface() {
    let (_t, mut app) = boot_app();
    handle_key_event_once(&mut app, ctrl('p')).unwrap();
    handle_key_event_once(&mut app, plain(KeyCode::Char('/'))).unwrap();
    handle_key_event_once(&mut app, plain(KeyCode::Char('r'))).unwrap();
    handle_key_event_once(&mut app, plain(KeyCode::Enter)).unwrap();
    assert_eq!(
        app.composition().unwrap().surface_state.current(),
        vac_shell_host_surface::Surface::Runtime
    );
    let _ = SurfaceTarget::Runtime;
}

#[test]
fn handle_esc_closes_overlay() {
    let (_t, mut app) = boot_app();
    handle_key_event_once(&mut app, ctrl('p')).unwrap();
    handle_key_event_once(&mut app, plain(KeyCode::Esc)).unwrap();
    assert_eq!(app.overlays.top(), ShellOverlay::None);
}

#[test]
fn handle_approval_submit_drains_queue() {
    let (_t, mut app) = boot_app();
    let comp = app.composition().unwrap().clone();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    app.refresh_approval_bar();
    // Plain Enter when no overlay is on top falls through to
    // ApprovalBar::Enter → SubmitApprovals.
    handle_key_event_once(&mut app, plain(KeyCode::Enter)).unwrap();
    assert!(comp.approval_queue.snapshot().is_empty());
    assert!(comp.approval_queue.last_outcome().is_some());
}

#[test]
fn handle_error_goes_to_activity_log() {
    let (_t, mut app) = boot_app();
    let log = Arc::new(ActivityLog::default());
    app.activity_log = Some(log.clone());
    // Open detail with no approval row → Approve emits an
    // ApprovalDecision against an unknown id → recorded as error.
    app.handle_global_key(vac_shell_app::GlobalKey::OpenApprovalDetail);
    app.approval_detail.detail = Some(
        vac_shell_contracts::ApprovalDetailView {
            id: "ghost".into(),
            tool_name: "shell".into(),
            risk_level: vac_shell_contracts::RiskLevel::Medium,
            reason: String::new(),
            command_preview: None,
            file_preview: None,
            policy_source: None,
        },
    );
    let _ = handle_key_event_once(&mut app, plain(KeyCode::Char('y')));
    let snap = log.snapshot();
    assert!(snap.iter().any(|e| matches!(
        e.kind,
        vac_shell_contracts::ShellActivityKind::Error
    )));
}

#[test]
fn quit_key_only_fires_in_no_overlay() {
    let (_t, mut app) = boot_app();
    let opts = ShellLoopOptions::default();
    assert!(is_quit_key(plain(KeyCode::Char('q')), &app, opts));
    // With overlay open, q is NOT a quit gesture (it should
    // travel into widget search, etc.).
    handle_key_event_once(&mut app, ctrl('p')).unwrap();
    assert!(!is_quit_key(plain(KeyCode::Char('q')), &app, opts));
}

#[test]
fn quit_key_disabled_when_option_off() {
    let (_t, app) = boot_app();
    let opts = ShellLoopOptions {
        exit_on_q: false,
        ..ShellLoopOptions::default()
    };
    assert!(!is_quit_key(plain(KeyCode::Char('q')), &app, opts));
}
