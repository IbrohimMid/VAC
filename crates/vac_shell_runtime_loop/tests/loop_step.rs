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
    ShellLoopOptions, ShellRuntimeContext, handle_key_event_once, is_quit_key,
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
    let (_t, app) = boot_app();
    let mut ctx = ShellRuntimeContext::new(app);
    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    assert_eq!(ctx.app.overlays.top(), ShellOverlay::Palette);
}

#[test]
fn handle_ctrl_m_opens_model_switcher() {
    let (_t, app) = boot_app();
    let mut ctx = ShellRuntimeContext::new(app);
    handle_key_event_once(&mut ctx, ctrl('m')).unwrap();
    assert_eq!(ctx.app.overlays.top(), ShellOverlay::ModelSwitcher);
    assert!(!ctx.app.model_switcher.models.is_empty());
}

#[test]
fn handle_palette_runtime_changes_surface() {
    let (_t, app) = boot_app();
    let mut ctx = ShellRuntimeContext::new(app);
    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('/'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('r'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Enter)).unwrap();
    assert_eq!(
        ctx.app.composition().unwrap().surface_state.current(),
        vac_shell_host_surface::Surface::Runtime
    );
    let _ = SurfaceTarget::Runtime;
}

#[test]
fn handle_esc_closes_overlay() {
    let (_t, app) = boot_app();
    let mut ctx = ShellRuntimeContext::new(app);
    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Esc)).unwrap();
    assert_eq!(ctx.app.overlays.top(), ShellOverlay::None);
}

#[test]
fn handle_approval_submit_drains_queue() {
    let (_t, app) = boot_app();
    let mut ctx = ShellRuntimeContext::new(app);
    let comp = ctx.app.composition().unwrap().clone();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "shell"));
    ctx.app.refresh_approval_bar();
    // Plain Enter when no overlay is on top falls through to
    // ApprovalBar::Enter → SubmitApprovals.
    handle_key_event_once(&mut ctx, plain(KeyCode::Enter)).unwrap();
    assert!(comp.approval_queue.snapshot().is_empty());
    assert!(comp.approval_queue.last_outcome().is_some());
}

#[test]
fn handle_error_goes_to_activity_log() {
    let (_t, app) = boot_app();
    let mut ctx = ShellRuntimeContext::new(app);
    let log = Arc::new(ActivityLog::default());
    ctx.app.activity_log = Some(log.clone());
    // Open detail with no approval row → Approve emits an
    // ApprovalDecision against an unknown id → recorded as error.
    ctx.app.handle_global_key(vac_shell_app::GlobalKey::OpenApprovalDetail);
    ctx.app.approval_detail.detail = Some(
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
    let _ = handle_key_event_once(&mut ctx, plain(KeyCode::Char('y')));
    let snap = log.snapshot();
    assert!(snap.iter().any(|e| matches!(
        e.kind,
        vac_shell_contracts::ShellActivityKind::Error
    )));
}

#[test]
fn quit_key_only_fires_in_no_overlay() {
    let (_t, app) = boot_app();
    let mut ctx = ShellRuntimeContext::new(app);
    let opts = ShellLoopOptions::default();
    assert!(is_quit_key(plain(KeyCode::Char('q')), &ctx.app, opts));
    // With overlay open, q is NOT a quit gesture (it should
    // travel into widget search, etc.).
    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    assert!(!is_quit_key(plain(KeyCode::Char('q')), &ctx.app, opts));
}

#[test]
fn quit_key_disabled_when_option_off() {
    let (_t, app) = boot_app();
    let ctx = ShellRuntimeContext::new(app);
    let opts = ShellLoopOptions {
        exit_on_q: false,
        ..ShellLoopOptions::default()
    };
    assert!(!is_quit_key(plain(KeyCode::Char('q')), &ctx.app, opts));
}

// =====================================================================
// D-track hardening — runtime context routing PaletteSelected
// =====================================================================

#[test]
fn runtime_loop_routes_custom_palette_slash_to_executor() {
    use vac_shell_host_commands::{RecordingExecutor, ShellCommandExecutor};
    let (_t, app) = boot_app();
    // Add a non-built-in slash to the registry.
    let comp = app.composition().unwrap().clone();
    use vac_shell_bridge::InMemoryCommandRegistry;
    let registry = InMemoryCommandRegistry::new(vec![
        cmd("/runtime"),
        cmd("/chat"),
        cmd("/memorize"),
    ]);
    let registry: std::sync::Arc<dyn vac_shell_contracts::VacCommandRegistry> =
        std::sync::Arc::new(registry);
    let _ = comp; // keep alive
    // Reattach via a fresh app whose composition has /memorize.
    drop(app);
    let tmp = tempfile::tempdir().unwrap();
    let paths: std::sync::Arc<dyn VacPaths> = std::sync::Arc::new(VacPathsImpl::new(tmp.path()));
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
            cmd("/runtime"),
            cmd("/chat"),
            cmd("/model"),
            cmd("/sessions"),
            cmd("/memorize"),
        ])
        .boot()
        .unwrap();
    let _ = registry;
    let mut app = ShellApp::new(std::sync::Arc::new(comp));
    app.activity_log = Some(Arc::new(ActivityLog::default()));
    let recorder = Arc::new(RecordingExecutor::new());
    let mut ctx = ShellRuntimeContext::new(app)
        .with_executor(recorder.clone() as Arc<dyn ShellCommandExecutor>);

    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('/'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('m'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('e'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Enter)).unwrap();

    assert_eq!(recorder.seen(), vec!["/memorize".to_string()]);
}

#[test]
fn runtime_loop_records_error_when_custom_slash_has_no_executor() {
    let tmp = tempfile::tempdir().unwrap();
    let paths: std::sync::Arc<dyn VacPaths> = std::sync::Arc::new(VacPathsImpl::new(tmp.path()));
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
            cmd("/runtime"),
            cmd("/chat"),
            cmd("/model"),
            cmd("/sessions"),
            cmd("/memorize"),
        ])
        .boot()
        .unwrap();
    let mut app = ShellApp::new(std::sync::Arc::new(comp));
    let log = Arc::new(ActivityLog::default());
    app.activity_log = Some(log.clone());
    let mut ctx = ShellRuntimeContext::new(app);

    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('/'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('m'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('e'))).unwrap();
    let result = handle_key_event_once(&mut ctx, plain(KeyCode::Enter));
    assert!(result.is_err(), "missing executor must surface as Err");
    let snap = log.snapshot();
    assert!(snap.iter().any(|e| e.title.contains("no executor")));
}

#[test]
fn runtime_loop_builtin_runtime_does_not_call_executor() {
    use vac_shell_host_commands::{RecordingExecutor, ShellCommandExecutor};
    let (_t, app) = boot_app();
    let recorder = Arc::new(RecordingExecutor::new());
    let mut ctx = ShellRuntimeContext::new(app)
        .with_executor(recorder.clone() as Arc<dyn ShellCommandExecutor>);

    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('/'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('r'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Enter)).unwrap();

    assert!(
        recorder.seen().is_empty(),
        "built-in /runtime must not reach the executor"
    );
    assert_eq!(
        ctx.app.composition().unwrap().surface_state.current(),
        vac_shell_host_surface::Surface::Runtime
    );
}

#[test]
fn dogfood_context_uses_vac_command_executor_adapter_stub() {
    use vac_shell_host_commands::{ShellCommandExecutor, VacCommandExecutorAdapter};
    let tmp = tempfile::tempdir().unwrap();
    let paths: std::sync::Arc<dyn VacPaths> = std::sync::Arc::new(VacPathsImpl::new(tmp.path()));
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
            cmd("/runtime"),
            cmd("/chat"),
            cmd("/model"),
            cmd("/sessions"),
            cmd("/memorize"),
        ])
        .boot()
        .unwrap();
    let mut app = ShellApp::new(std::sync::Arc::new(comp));
    let log = Arc::new(ActivityLog::default());
    app.activity_log = Some(log.clone());
    let adapter: Arc<dyn ShellCommandExecutor> =
        Arc::new(VacCommandExecutorAdapter::new());
    let mut ctx = ShellRuntimeContext::new(app).with_executor(adapter);

    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('/'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('m'))).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('e'))).unwrap();
    let result = handle_key_event_once(&mut ctx, plain(KeyCode::Enter));
    // Adapter rejects with Unsupported(...) → AppError surfaces.
    assert!(result.is_err());
    let snap = log.snapshot();
    assert!(
        snap.iter().any(|e| e.detail.as_deref().unwrap_or("").contains("D5.1 stub")),
        "operator must see the D5.1-stub message via activity log"
    );
}
