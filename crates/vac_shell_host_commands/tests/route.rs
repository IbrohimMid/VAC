use std::sync::Arc;

use vac_shell_app::ShellApp;
use vac_shell_bridge::ProviderId;
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, VacPaths};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_commands::{
    RecordingExecutor, ShellCommandError, ShellCommandExecutor,
    VacCommandExecutorAdapter, route_palette_command,
};

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

fn boot_app(extra: Vec<ShellCommandSpec>) -> (tempfile::TempDir, ShellApp) {
    use vac_shell_composition::ShellCompositionBuilder;
    use vac_shell_host_model::{HostModel, ProviderInfo};
    use vac_shell_host_paths::VacPathsImpl;
    let tmp = tempfile::tempdir().unwrap();
    let paths: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(tmp.path()));
    let mut commands = vec![
        cmd("/chat"),
        cmd("/runtime"),
        cmd("/model"),
        cmd("/sessions"),
    ];
    commands.extend(extra);
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
        .with_commands(commands)
        .boot()
        .unwrap();
    let mut app = ShellApp::new(Arc::new(comp));
    app.activity_log = Some(Arc::new(ActivityLog::default()));
    (tmp, app)
}

#[test]
fn unknown_slash_without_executor_records_error() {
    let (_t, mut app) = boot_app(vec![]);
    let err = route_palette_command(&mut app, None, "/never").unwrap_err();
    assert!(err.title.contains("unknown palette command"));
    let snap = app.activity_log.as_ref().unwrap().snapshot();
    assert_eq!(snap.len(), 1);
}

#[test]
fn custom_slash_executes_via_executor() {
    let (_t, mut app) = boot_app(vec![cmd("/memorize")]);
    let recorder = Arc::new(RecordingExecutor::new());
    let exec: Arc<dyn ShellCommandExecutor> = recorder.clone();
    route_palette_command(&mut app, Some(&exec), "/memorize").unwrap();
    assert_eq!(recorder.seen(), vec!["/memorize".to_string()]);
}

#[test]
fn executor_failure_records_activity_error() {
    let (_t, mut app) = boot_app(vec![cmd("/memorize")]);
    let exec: Arc<dyn ShellCommandExecutor> =
        Arc::new(RecordingExecutor::new().fail_with("explode"));
    let err = route_palette_command(&mut app, Some(&exec), "/memorize").unwrap_err();
    assert!(err.title.contains("palette command failed"));
    assert!(err.detail.unwrap_or_default().contains("explode"));
    let snap = app.activity_log.as_ref().unwrap().snapshot();
    assert_eq!(snap.len(), 1);
    assert!(snap[0].title.contains("palette command failed"));
}

#[test]
fn builtin_slash_does_not_call_executor() {
    let (_t, mut app) = boot_app(vec![]);
    let recorder = Arc::new(RecordingExecutor::new());
    let exec: Arc<dyn ShellCommandExecutor> = recorder.clone();
    route_palette_command(&mut app, Some(&exec), "/runtime").unwrap();
    assert!(
        recorder.seen().is_empty(),
        "built-in /runtime must not reach the executor"
    );
    // Surface flipped through the built-in path.
    assert_eq!(
        app.composition().unwrap().surface_state.current(),
        vac_shell_host_surface::Surface::Runtime
    );
}

#[test]
fn missing_executor_for_custom_slash_records_error() {
    let (_t, mut app) = boot_app(vec![cmd("/memorize")]);
    let err = route_palette_command(&mut app, None, "/memorize").unwrap_err();
    assert!(err.title.contains("no executor"));
    let snap = app.activity_log.as_ref().unwrap().snapshot();
    assert_eq!(snap.len(), 1);
}

// =====================================================================
// D5.1 — VacCommandExecutorAdapter stub
// =====================================================================

#[test]
fn adapter_rejects_unsupported_command_cleanly() {
    let adapter = VacCommandExecutorAdapter::new();
    let err = adapter.execute(&cmd("/memorize")).unwrap_err();
    match err {
        ShellCommandError::Unsupported(msg) => {
            assert!(msg.contains("/memorize"));
            assert!(msg.contains("D5.1 stub"));
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
fn adapter_maps_known_command_id_to_handler() {
    let adapter = VacCommandExecutorAdapter::new().allow("memorize");
    adapter.execute(&cmd("/memorize")).unwrap();
}

#[test]
fn adapter_error_is_operator_visible_through_route() {
    let (_t, mut app) = boot_app(vec![cmd("/never-supported")]);
    let exec: Arc<dyn ShellCommandExecutor> = Arc::new(VacCommandExecutorAdapter::new());
    let err =
        route_palette_command(&mut app, Some(&exec), "/never-supported").unwrap_err();
    assert!(err.title.contains("palette command failed"));
    assert!(
        err.detail
            .unwrap_or_default()
            .contains("D5.1 stub"),
        "operator must see the stub message"
    );
    let snap = app.activity_log.as_ref().unwrap().snapshot();
    assert_eq!(snap.len(), 1);
}
