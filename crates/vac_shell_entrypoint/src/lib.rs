//! D1 — opt-in entrypoint seam for the new `ShellApp` cockpit.
//!
//! See `docs/adr/ADR-shellapp-runtime-integration.md` for the
//! decision log. This crate is deliberately tiny: a single public
//! function that hosts can call once they're ready to stand up the
//! new shell stack from a project root. It does **not** open a
//! crossterm event loop (D2 / D2.1) and it does **not** read VAC
//! engine state (D3+).
//!
//! # Boundary
//!
//! Allowed runtime deps:
//!
//! * `vac_shell_app`           — the orchestrator
//! * `vac_shell_composition`   — boot composition builder
//! * `vac_shell_host_paths`    — `VacPathsImpl`
//! * `vac_shell_host_model`    — `ProviderInfo` / `HostModel`
//! * `vac_shell_host_activity` — `ActivityLog`
//! * `vac_shell_host_sessions` — `SessionsState`
//! * `vac_shell_bridge`        — `ProviderId`
//! * `vac_shell_contracts`     — `VacPaths`
//! * `vac_shell_host_status_command` — `/status` command metadata only; provider-driven executor is wired by host/example.
//! * `vac_shell_host_recovery` — checkpoint recovery projector (D16).
//!
//! Forbidden: `vac_core`, `vac_session_engine`, `vac_tui_runtime`,
//! `stakai`, donor crates, `.stakpak` path composition, secret
//! managers, auto-approve managers.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use vac_shell_app::ShellApp;
use vac_shell_bridge::ProviderId;
use vac_shell_composition::{ShellComposition, ShellCompositionBuilder};
use vac_shell_contracts::VacPaths;
use vac_shell_host_activity::ActivityLog;
use vac_shell_host_model::{HostModel, ProviderInfo};
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_host_recovery::project_recovery_for_session;
use vac_shell_host_sessions::SessionsState;

/// Build a `ShellApp` from a project root with safe fixture
/// providers/models/commands. Returns the built app and the
/// composition the test harness can poke at.
///
/// This is the inner guts of [`run_shell_app`], split out so tests
/// can assert state without driving a full process exit. Hosts
/// that want to embed the shell into their own runtime do the same
/// build steps inline.
pub fn build_shell_app(project_root: impl AsRef<Path>) -> ShellApp {
    let paths: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(project_root.as_ref()));
    // The activity log is the canonical sink for boot warnings
    // (e.g. config snapshot unreadable). Construct it before any
    // fallible step so the first error path can record cleanly.
    let activity_log = Arc::new(ActivityLog::default());

    // D3.1 — try the read-only `VacConfigModelSource` first.
    //
    // * snapshot present + valid → use it.
    // * snapshot missing         → fall back to fixture, no warn
    //   (this is the documented "first boot, no engine probe yet"
    //   path).
    // * snapshot present + unreadable / corrupt → fall back AND
    //   record an activity warning so the operator sees what
    //   went wrong.
    let (providers, models, fallback_active) =
        match vac_shell_host_vac_config::load_from_paths(paths.as_ref()) {
            Ok(Some(src)) => {
                use vac_shell_host_model::ModelSource;
                let providers = src.providers();
                let models = src.models();
                // Hardening — never let the snapshot boot the
                // cockpit with a no-credentials active model.
                let raw_active = src.active_model();
                let sanitized = vac_shell_host_vac_config::sanitize_active_model(
                    &providers,
                    &models,
                    raw_active.clone(),
                );
                if raw_active.is_some() && sanitized.is_none() {
                    let id = format!("config-warn-{}", now_unix());
                    activity_log.record_error(
                        id,
                        now_unix(),
                        "snapshot active model dropped",
                        Some(
                            "active model from .vac/model_config.json was either \
                             not in the model list or its provider had no \
                             credentials; falling back"
                                .into(),
                        ),
                    );
                }
                (providers, models, sanitized)
            }
            Ok(None) => fixture_inputs(),
            Err(err) => {
                let id = format!("config-warn-{}", now_unix());
                activity_log.record_error(
                    id,
                    now_unix(),
                    "model config snapshot unreadable; using fixture",
                    Some(format!("{err}")),
                );
                fixture_inputs()
            }
        };

    let commands = default_commands();
    let composition: Arc<ShellComposition> = Arc::new(
        ShellCompositionBuilder::new(paths)
            .with_providers(providers)
            .with_models(models)
            .with_fallback_active(fallback_active)
            .with_commands(commands)
            .boot()
            .expect("ShellCompositionBuilder::boot must succeed"),
    );

    let mut app = ShellApp::new(composition.clone());
    app.activity_log = Some(activity_log);
    app.sessions = Some(Arc::new(SessionsState::new()));
    // D16 — attach checkpoint recovery provider
    let paths_for_recovery = composition.paths.clone();
    app = app.with_session_recovery_provider(Arc::new(move |session_id| {
        Some(project_recovery_for_session(
            paths_for_recovery.as_ref(),
            session_id,
        ))
    }));
    app.prepare_frame();
    app
}

fn fixture_inputs() -> (
    Vec<ProviderInfo>,
    Vec<HostModel>,
    Option<(ProviderId, String)>,
) {
    let providers = vec![ProviderInfo {
        id: ProviderId("anthropic".into()),
        credentials_present: true,
    }];
    let models = vec![HostModel {
        provider: ProviderId("anthropic".into()),
        id: "claude-sonnet-4.5".into(),
        label: "Claude Sonnet 4.5".into(),
        reasoning: true,
        cost_label: None,
    }];
    let fallback_active = Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into()));
    (providers, models, fallback_active)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Boot the new `ShellApp` cockpit from a project root.
///
/// D1 scope: instantiate, attach the activity log + sessions
/// controller, run a single `prepare_frame()`, return cleanly.
/// No interactive crossterm loop (D2 concern). No engine
/// integration (D3+).
///
/// Hosts that need to drive the cockpit interactively will replace
/// this with a richer entrypoint once D2 lands; until then this
/// function is a smoke-test shim plus the seam the future loop
/// will be built around.
pub fn run_shell_app(project_root: impl AsRef<Path>) -> ExitCode {
    let _app = build_shell_app(project_root);
    // Any future error path returns a non-zero ExitCode; for now
    // the boot itself either succeeds or the underlying builder
    // will have panicked, which we surface to the caller as a
    // non-zero exit at the host level.
    ExitCode::SUCCESS
}

/// Default command set the entrypoint registers. Matches the
/// built-in palette routes in `vac_shell_app::apply_event` so the
/// palette can be exercised end-to-end without the embedding host
/// supplying its own list yet.
fn default_commands() -> Vec<vac_shell_contracts::ShellCommandSpec> {
    use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec};
    let built_in = |slash: &str, title: &str| ShellCommandSpec {
        id: slash.trim_start_matches('/').to_string(),
        slash: slash.into(),
        title: title.into(),
        description: String::new(),
        kind: ShellCommandKind::BuiltInAction,
        palette_visible: true,
        ..Default::default()
    };
    // D7B — register the two dogfood custom commands so the
    // registry stays in sync with `AdapterConfig::dogfood`. The
    // entrypoint library only adds metadata; no engine dep is
    // pulled in. Hosts that prefer a leaner registry can still
    // drop these by passing their own command list to
    // `ShellCompositionBuilder` directly.
    let dogfood = |slash: &str, title: &str, description: &str| ShellCommandSpec {
        id: slash.trim_start_matches('/').to_string(),
        slash: slash.into(),
        title: title.into(),
        description: description.into(),
        kind: ShellCommandKind::PromptTemplate,
        palette_visible: true,
        category: Some("Dogfood".into()),
        ..Default::default()
    };
    let diagnostics = |slash: &str, title: &str, description: &str| ShellCommandSpec {
        id: slash.trim_start_matches('/').to_string(),
        slash: slash.into(),
        title: title.into(),
        description: description.into(),
        kind: ShellCommandKind::PromptTemplate,
        palette_visible: true,
        category: Some("Diagnostics".into()),
        ..Default::default()
    };
    vec![
        built_in("/chat", "Chat"),
        built_in("/runtime", "Runtime"),
        built_in("/model", "Model switcher"),
        built_in("/sessions", "Sessions"),
        built_in("/logs", "Logs browser"),
        built_in("/init", "Init checklist"),
        vac_shell_host_status_command::status_command_spec(),
        diagnostics(
            "/doctor",
            "Doctor",
            "Run readiness checks for local environment",
        ),
        dogfood(
            "/memorize",
            "Memorize",
            "Submit a memory-oriented dogfood prompt through the D7B command adapter.",
        ),
        dogfood(
            "/ultraplan",
            "Ultraplan",
            "Submit a planning-oriented dogfood prompt through the D7B command adapter.",
        ),
    ]
}
