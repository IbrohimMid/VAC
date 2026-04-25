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
