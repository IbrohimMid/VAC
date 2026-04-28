//! D5 / D5.1 — palette command executor bridge.
//!
//! `ShellApp::apply_event(AppEvent::PaletteSelected(slash))`
//! routes the four built-in slashes (`/chat`, `/runtime`,
//! `/model`, `/sessions`) directly. Anything else is "embedding
//! host's problem". This crate gives the embedding host that
//! seam:
//!
//! ```text
//! ShellRuntimeContext { app, command_executor }
//!   on PaletteSelected(slash):
//!     1. Try the built-in route via app.apply_event.
//!     2. If still unhandled, look up the spec in
//!        comp.command_registry.
//!     3. If a `command_executor` is bound, call it.
//!     4. On error, record an activity entry; return AppError.
//! ```
//!
//! No engine coupling here yet; D5.1 ships a `VacCommandExecutorAdapter`
//! stub that *rejects* every command with a clear message. A
//! future slice (≥ D7) replaces the stub with a real bridge into
//! `vac_session_engine` / `vac_cli::commands`.

use std::sync::{Arc, Mutex};

use vac_shell_app::{AppError, AppEvent, ShellApp};
use vac_shell_contracts::ShellCommandSpec;

#[derive(Debug, thiserror::Error)]
pub enum ShellCommandError {
    #[error("command unsupported: {0}")]
    Unsupported(String),
    #[error("command failed: {0}")]
    Failed(String),
}

/// Host-side executor for palette slashes the shell built-ins do
/// not handle. Hosts implement this against their own command
/// pipeline; the shell crate stays free of product semantics.
pub trait ShellCommandExecutor: Send + Sync {
    fn execute(&self, command: &ShellCommandSpec) -> Result<(), ShellCommandError>;
}

/// Built-in slashes handled directly by `ShellApp::apply_event`.
/// Kept here so the executor router can shortcut without going
/// through the app on every keystroke.
const BUILT_IN_SLASHES: &[&str] = &["/chat", "/runtime", "/model", "/sessions", "/logs", "/init"];

fn is_built_in(slash: &str) -> bool {
    BUILT_IN_SLASHES.contains(&slash)
}

/// Route a `PaletteSelected` slash through ShellApp's built-in
/// path first, then through the supplied executor (if any).
/// Errors propagate as `AppError` and are recorded into the app's
/// activity log via the same channel `apply_event` uses.
pub fn route_palette_command(
    app: &mut ShellApp,
    executor: Option<&Arc<dyn ShellCommandExecutor>>,
    slash: &str,
) -> Result<(), AppError> {
    // Built-ins always go through the shell-app applier so
    // surface flips and overlay opens land via the exact same
    // path as a Ctrl+P selection.
    if is_built_in(slash) {
        return app.apply_event(AppEvent::PaletteSelected(slash.to_string()));
    }

    // Non-built-in: resolve the spec from the live registry.
    let comp = match app.composition() {
        Some(c) => c,
        None => {
            return Err(report(
                app,
                "palette command rejected: composition not attached",
                None,
            ));
        }
    };
    let spec = match comp.command_registry.by_slash(slash) {
        Some(s) => s,
        None => {
            return Err(report(
                app,
                &format!("unknown palette command: {slash}"),
                None,
            ));
        }
    };
    let executor = match executor {
        Some(e) => e.clone(),
        None => {
            return Err(report(
                app,
                &format!("palette command has no executor: {slash}"),
                None,
            ));
        }
    };
    if let Err(e) = executor.execute(&spec) {
        return Err(report(
            app,
            &format!("palette command failed: {slash}"),
            Some(e.to_string()),
        ));
    }
    Ok(())
}

/// Helper that records the failure into the app's activity log
/// (when attached) and returns an `AppError` carrying the same
/// title/detail.
fn report(app: &ShellApp, title: &str, detail: Option<String>) -> AppError {
    if let Some(log) = &app.activity_log {
        let id = format!("cmd-err-{}", now_unix());
        log.record_error(id, now_unix(), title, detail.clone());
    }
    AppError {
        title: title.to_string(),
        detail,
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// =====================================================================
// In-memory recording executor — fixture for tests / bring-up
// =====================================================================

#[derive(Debug, Clone, Default)]
pub struct RecordingExecutor {
    inner: Arc<Mutex<RecordingInner>>,
}

#[derive(Debug, Default)]
struct RecordingInner {
    seen: Vec<String>,
    fail_with: Option<String>,
}

impl RecordingExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fail_with(self, msg: impl Into<String>) -> Self {
        {
            let mut g = self.inner.lock().expect("recorder lock");
            g.fail_with = Some(msg.into());
        }
        self
    }

    pub fn seen(&self) -> Vec<String> {
        self.inner.lock().expect("recorder lock").seen.clone()
    }
}

impl ShellCommandExecutor for RecordingExecutor {
    fn execute(&self, command: &ShellCommandSpec) -> Result<(), ShellCommandError> {
        let mut g = self.inner.lock().expect("recorder lock");
        if let Some(msg) = &g.fail_with {
            let m = msg.clone();
            return Err(ShellCommandError::Failed(m));
        }
        g.seen.push(command.slash.clone());
        Ok(())
    }
}

// =====================================================================
// D5.1 — VAC command executor adapter STUB
// =====================================================================
//
// Placeholder for the future engine bridge. Until the bridge
// lands (≥ D7), this rejects every command with a clear
// "unsupported" error and a hint about which slice will fill it
// in. Hosts that need richer behaviour today supply their own
// executor.

/// Stub adapter pointing at the future `vac_session_engine` /
/// `vac_cli::commands` bridge. Rejects every command with
/// `ShellCommandError::Unsupported(...)` until the real bridge
/// lands. Holding this as a separate type so the migration patch
/// only swaps the impl, not the call sites.
#[derive(Debug, Clone, Default)]
pub struct VacCommandExecutorAdapter {
    /// Allowlist of command IDs the adapter will pretend to
    /// handle (used by tests; production initially empty).
    allow: Vec<String>,
}

impl VacCommandExecutorAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a command id the adapter will accept. Returns `Ok(())`
    /// on `execute`, simulating the eventual real handler. Used
    /// by tests to verify the routing wiring without a real
    /// engine.
    pub fn allow(mut self, id: impl Into<String>) -> Self {
        self.allow.push(id.into());
        self
    }
}

impl ShellCommandExecutor for VacCommandExecutorAdapter {
    fn execute(&self, command: &ShellCommandSpec) -> Result<(), ShellCommandError> {
        if self.allow.iter().any(|id| id == &command.id) {
            return Ok(());
        }
        Err(ShellCommandError::Unsupported(format!(
            "{} — VAC command executor adapter is a D5.1 stub; engine bridge lands in a later slice",
            command.slash
        )))
    }
}
