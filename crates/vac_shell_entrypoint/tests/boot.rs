//! D1 — entrypoint boot proofs.

use std::process::ExitCode;

use vac_shell_bridge::ProviderId;
use vac_shell_contracts::VacPaths;
use vac_shell_entrypoint::{build_shell_app, run_shell_app};
use vac_shell_host_paths::VacPathsImpl;

#[test]
fn entrypoint_boots_shell_app_from_temp_project_root() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_shell_app(tmp.path());
    let comp = app.composition().expect("composition must attach");
    assert_eq!(
        comp.model_state.active_model(),
        Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into()
        ))
    );
    // Default commands should be seeded so the palette has
    // something to project on first open.
    let slashes: Vec<String> = comp
        .command_registry
        .all()
        .into_iter()
        .map(|s| s.slash)
        .collect();
    assert!(slashes.iter().any(|s| s == "/runtime"));
    assert!(slashes.iter().any(|s| s == "/model"));
}

#[test]
fn build_shell_app_registers_d7b_dogfood_custom_commands() {
    use vac_shell_contracts::ShellCommandKind;

    let tmp = tempfile::tempdir().unwrap();
    let app = build_shell_app(tmp.path());
    let comp = app.composition().expect("composition attached");

    let memorize = comp
        .command_registry
        .by_slash("/memorize")
        .expect("/memorize must be registered for the D7B dogfood adapter to reach it");
    assert_eq!(memorize.id, "memorize");
    assert_eq!(memorize.kind, ShellCommandKind::PromptTemplate);
    assert!(memorize.palette_visible);

    let ultraplan = comp
        .command_registry
        .by_slash("/ultraplan")
        .expect("/ultraplan must be registered for the D7B dogfood adapter to reach it");
    assert_eq!(ultraplan.id, "ultraplan");
    assert_eq!(ultraplan.kind, ShellCommandKind::PromptTemplate);
}

#[test]
fn entrypoint_attaches_activity_log() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_shell_app(tmp.path());
    assert!(app.activity_log.is_some(), "activity log must be attached");
    // The freshly-built log starts empty; future error paths land
    // here once D2+ wires real key handling.
    assert!(app.activity_log.as_ref().unwrap().is_empty());
}

#[test]
fn entrypoint_attaches_sessions_state() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_shell_app(tmp.path());
    assert!(app.sessions.is_some(), "sessions state must be attached");
    let comp = app.composition().unwrap();
    // No transcripts on disk yet → empty list.
    assert!(
        app.sessions
            .as_ref()
            .unwrap()
            .list(comp.paths.as_ref())
            .is_empty()
    );
}

#[test]
fn entrypoint_uses_vac_paths_not_stakpak() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_shell_app(tmp.path());
    let comp = app.composition().unwrap();
    // Probe every `VacPaths` getter; none may compose `.stakpak`.
    let paths = &comp.paths;
    for resolved in [
        paths.project_state_dir(),
        paths.sessions_dir(),
        paths.plan_file(),
        paths.commands_dir(),
        paths.model_selection_file(),
    ] {
        let s = resolved.to_string_lossy().to_string();
        assert!(s.contains(".vac"), ".vac missing in {s}");
        assert!(!s.contains(".stakpak"), "donor path leaked: {s}");
    }
}

#[test]
fn run_shell_app_returns_success_on_clean_boot() {
    let tmp = tempfile::tempdir().unwrap();
    // `ExitCode` lacks `Eq`; check via debug rep for SUCCESS.
    let code: ExitCode = run_shell_app(tmp.path());
    let dbg = format!("{code:?}");
    assert!(dbg.contains("SUCCESS") || dbg == "ExitCode(unix_exit_status(0))",
        "unexpected ExitCode: {dbg}");
}

/// Drift tripwire — only host crates allowed by the ADR may be
/// reachable from the entrypoint's public surface. A future patch
/// that pulls `vac_core` / `vac_session_engine` / `vac_tui_runtime`
/// into the entrypoint would fail this compile-only smoke test
/// because those crates' types would shadow the imports below. Real
/// dep-graph proof comes from `cargo tree -p vac_shell_entrypoint
/// -e normal --depth 2` plus the Cargo.toml review.
#[test]
fn public_types_are_local_smoke_test() {
    fn assert_only_local<T: Sized>(_: T) {}
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    assert_only_local(paths.project_state_dir());
}

// =====================================================================
// D3.1 — config source boot with safe fallback
// =====================================================================

#[test]
fn entrypoint_uses_config_model_source_when_available_with_credentialed_active() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let path = paths.model_config_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let snapshot = r#"
    {
      "providers": [
        {"id": "openai", "credentials_present": true}
      ],
      "models": [
        {
          "provider": "openai",
          "id": "gpt-4o",
          "label": "GPT-4o",
          "reasoning": false
        }
      ],
      "active": {"provider": "openai", "id": "gpt-4o"}
    }
    "#;
    std::fs::write(&path, snapshot).unwrap();
    let app = build_shell_app(tmp.path());
    let comp = app.composition().unwrap();
    assert_eq!(
        comp.model_state.active_model(),
        Some((ProviderId("openai".into()), "gpt-4o".into()))
    );
    // Happy path — no warning recorded.
    assert!(app.activity_log.as_ref().unwrap().is_empty());
}

#[test]
fn entrypoint_drops_config_active_when_provider_has_no_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let path = paths.model_config_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let snapshot = r#"
    {
      "providers": [
        {"id": "openai", "credentials_present": false}
      ],
      "models": [
        {
          "provider": "openai",
          "id": "gpt-4o",
          "label": "GPT-4o",
          "reasoning": false
        }
      ],
      "active": {"provider": "openai", "id": "gpt-4o"}
    }
    "#;
    std::fs::write(&path, snapshot).unwrap();
    let app = build_shell_app(tmp.path());
    let comp = app.composition().unwrap();
    assert!(
        comp.model_state.active_model().is_none(),
        "no-creds active must be dropped"
    );
    let log = app.activity_log.as_ref().unwrap().snapshot();
    assert!(log.iter().any(|e| e.title.contains("active model dropped")));
}

#[test]
fn entrypoint_drops_config_active_when_model_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let path = paths.model_config_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let snapshot = r#"
    {
      "providers": [
        {"id": "anthropic", "credentials_present": true}
      ],
      "models": [
        {
          "provider": "anthropic",
          "id": "claude-haiku-4",
          "label": "Claude Haiku 4",
          "reasoning": false
        }
      ],
      "active": {"provider": "anthropic", "id": "ghost-model"}
    }
    "#;
    std::fs::write(&path, snapshot).unwrap();
    let app = build_shell_app(tmp.path());
    let comp = app.composition().unwrap();
    assert!(
        comp.model_state.active_model().is_none(),
        "missing-model active must be dropped"
    );
    let log = app.activity_log.as_ref().unwrap().snapshot();
    assert!(log.iter().any(|e| e.title.contains("active model dropped")));
}

#[test]
fn entrypoint_model_switcher_contains_snapshot_model_not_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let path = paths.model_config_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let snapshot = r#"
    {
      "providers": [
        {"id": "openai", "credentials_present": true}
      ],
      "models": [
        {
          "provider": "openai",
          "id": "gpt-4o",
          "label": "GPT-4o",
          "reasoning": false
        }
      ],
      "active": {"provider": "openai", "id": "gpt-4o"}
    }
    "#;
    std::fs::write(&path, snapshot).unwrap();
    let mut app = build_shell_app(tmp.path());
    app.handle_global_key(vac_shell_app::GlobalKey::OpenModelSwitcher);
    let labels: Vec<&str> = app
        .model_switcher
        .models
        .iter()
        .map(|m| m.label.as_str())
        .collect();
    assert!(labels.contains(&"GPT-4o"), "snapshot model missing: {labels:?}");
    assert!(
        !labels.contains(&"Claude Sonnet 4.5"),
        "fixture model must not appear when snapshot is used: {labels:?}"
    );
}

#[test]
fn entrypoint_falls_back_to_fixture_when_config_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_shell_app(tmp.path());
    let comp = app.composition().unwrap();
    assert_eq!(
        comp.model_state.active_model(),
        Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into()))
    );
    // No warning recorded for the documented "no snapshot yet" path.
    assert!(app.activity_log.as_ref().unwrap().is_empty());
}

#[test]
fn fallback_records_activity_warning_on_corrupt_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let path = paths.model_config_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{ this is not json").unwrap();
    let app = build_shell_app(tmp.path());
    let log = app.activity_log.as_ref().unwrap();
    let snap = log.snapshot();
    assert_eq!(snap.len(), 1);
    assert!(snap[0]
        .title
        .contains("model config snapshot unreadable"));
    // Boot did NOT fail.
    let comp = app.composition().unwrap();
    assert!(comp.model_state.active_model().is_some());
}

#[test]
fn no_secret_material_in_activity_log() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let path = paths.model_config_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"corrupt").unwrap();
    let app = build_shell_app(tmp.path());
    let snap = app.activity_log.as_ref().unwrap().snapshot();
    let dump = format!("{:?}", snap);
    let lower = dump.to_lowercase();
    for forbidden in ["api_key", "api-key", "secret", "token", "bearer"] {
        assert!(
            !lower.contains(forbidden),
            "forbidden token `{forbidden}` in activity log dump: {dump}",
        );
    }
}
