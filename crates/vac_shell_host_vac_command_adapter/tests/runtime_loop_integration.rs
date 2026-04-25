//! D7B — runtime-loop integration: prove a custom palette
//! slash routed through `ShellRuntimeContext` reaches the real
//! engine-backed adapter and produces a transcript.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use vac_shell_app::ShellApp;
use vac_shell_bridge::ProviderId;
use vac_shell_composition::ShellCompositionBuilder;
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, VacPaths};
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_commands::ShellCommandExecutor;
use vac_shell_host_model::{HostModel, ProviderInfo};
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_host_vac_command_adapter::{
    AdapterCommandSpec, AdapterConfig, VacCommandExecutorAdapter,
};
use vac_shell_runtime_loop::{ShellRuntimeContext, handle_key_event_once};

fn ctrl(c: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn plain(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn cmd_spec(slash: &str) -> ShellCommandSpec {
    let id = slash.trim_start_matches('/').to_string();
    ShellCommandSpec {
        id,
        slash: slash.to_string(),
        title: format!("{slash} title"),
        description: format!("{slash} description"),
        kind: ShellCommandKind::PromptTemplate,
        palette_visible: true,
        shortcut: None,
        category: None,
        aliases: vec![],
        keywords: vec![],
        disabled_reason: None,
    }
}

fn build_runtime_context(
    project_root: std::path::PathBuf,
    adapter: Arc<dyn ShellCommandExecutor>,
) -> ShellRuntimeContext {
    let paths: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(project_root));
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
            cmd_spec("/runtime"),
            cmd_spec("/chat"),
            cmd_spec("/model"),
            cmd_spec("/sessions"),
            cmd_spec("/memorize"),
        ])
        .boot()
        .unwrap();
    let mut app = ShellApp::new(Arc::new(comp));
    app.activity_log = Some(Arc::new(ActivityLog::default()));
    ShellRuntimeContext::new(app).with_executor(adapter)
}

#[test]
fn runtime_loop_custom_command_reaches_real_adapter() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = Arc::new(VacCommandExecutorAdapter::new(
        AdapterConfig::new(tmp.path().to_path_buf()).with_command(
            AdapterCommandSpec::new(
                "memorize",
                "/memorize",
                "Memorize the current operator context.",
            ),
        ),
    ));
    let exec_handle = adapter.clone();
    let mut ctx = build_runtime_context(tmp.path().to_path_buf(), adapter);

    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('/'))).unwrap();
    for ch in "memorize".chars() {
        handle_key_event_once(&mut ctx, plain(KeyCode::Char(ch))).unwrap();
    }
    handle_key_event_once(&mut ctx, plain(KeyCode::Enter))
        .expect("real adapter must succeed for mapped /memorize");

    let path = exec_handle
        .last_transcript()
        .expect("adapter recorded transcript path");
    assert!(path.exists(), "transcript must exist on disk: {path:?}");
}

#[test]
fn runtime_loop_unmapped_command_records_unsupported_in_activity_log() {
    // No mapping for /memorize in this adapter → Unsupported.
    let tmp = tempfile::tempdir().unwrap();
    let adapter: Arc<dyn ShellCommandExecutor> = Arc::new(VacCommandExecutorAdapter::new(
        AdapterConfig::new(tmp.path().to_path_buf()),
    ));
    let mut ctx = build_runtime_context(tmp.path().to_path_buf(), adapter);

    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('/'))).unwrap();
    for ch in "memorize".chars() {
        handle_key_event_once(&mut ctx, plain(KeyCode::Char(ch))).unwrap();
    }
    let result = handle_key_event_once(&mut ctx, plain(KeyCode::Enter));
    assert!(result.is_err());

    let log = ctx.app.activity_log.as_ref().unwrap();
    let snap = log.snapshot();
    assert!(
        snap.iter().any(|e| e.title.contains("/memorize")),
        "activity log must record the rejected slash: {snap:?}"
    );
}

#[test]
fn runtime_loop_built_in_runtime_does_not_invoke_real_adapter() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = Arc::new(VacCommandExecutorAdapter::new(
        AdapterConfig::dogfood(tmp.path().to_path_buf()),
    ));
    let exec_handle = adapter.clone();
    let mut ctx = build_runtime_context(tmp.path().to_path_buf(), adapter);

    handle_key_event_once(&mut ctx, ctrl('p')).unwrap();
    handle_key_event_once(&mut ctx, plain(KeyCode::Char('/'))).unwrap();
    for ch in "runtime".chars() {
        handle_key_event_once(&mut ctx, plain(KeyCode::Char(ch))).unwrap();
    }
    handle_key_event_once(&mut ctx, plain(KeyCode::Enter)).unwrap();

    assert!(
        exec_handle.last_transcript().is_none(),
        "built-in /runtime must not reach the engine adapter"
    );
}
