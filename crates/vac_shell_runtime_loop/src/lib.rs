//! D2.2 — minimal runtime loop harness, hardened (D-track patch).
//!
//! Connects `vac_shell_keymap::route_key` + `dispatch_routed_key`
//! to a [`ShellApp`] living inside a [`ShellRuntimeContext`].
//! Custom `PaletteSelected` slashes flow through
//! `vac_shell_host_commands::route_palette_command`, so a host
//! that attaches a `ShellCommandExecutor` (or
//! `VacCommandExecutorAdapter` stub) sees those commands fire
//! through the proper bridge.
//!
//! The interactive `run_shell_loop` uses a `TerminalGuard` so a
//! mid-setup crash cannot leave raw mode + alternate screen
//! enabled.

use std::process::ExitCode;
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use vac_shell_app::{AppError, AppEvent, ShellApp};
use vac_shell_host_commands::{ShellCommandExecutor, route_palette_command};
use vac_shell_keymap::{dispatch_routed_key, route_key};

#[derive(Debug, thiserror::Error)]
pub enum ShellLoopError {
    #[error("app dispatch failed: {0}")]
    App(#[from] AppError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct ShellLoopOptions {
    /// Tick rate for crossterm polling, in milliseconds.
    pub tick_rate_ms: u64,
    /// Plain `q` (no overlay open) exits the loop.
    pub exit_on_q: bool,
}

impl Default for ShellLoopOptions {
    fn default() -> Self {
        Self {
            tick_rate_ms: 100,
            exit_on_q: true,
        }
    }
}

/// Runtime context binding the shell-app to its optional command
/// executor. Hosts attach a `VacCommandExecutorAdapter` (D5.1
/// stub) or their own `ShellCommandExecutor` impl; the loop
/// routes non-built-in palette slashes through it.
pub struct ShellRuntimeContext {
    pub app: ShellApp,
    pub command_executor: Option<Arc<dyn ShellCommandExecutor>>,
}

impl ShellRuntimeContext {
    pub fn new(app: ShellApp) -> Self {
        Self {
            app,
            command_executor: None,
        }
    }

    pub fn with_executor(mut self, executor: Arc<dyn ShellCommandExecutor>) -> Self {
        self.command_executor = Some(executor);
        self
    }
}

/// Step a single key event through routing → dispatch → applier
/// → `prepare_frame`. `PaletteSelected` events route through
/// the command bridge so custom slashes hit the executor;
/// everything else uses `app.apply_event` directly.
pub fn handle_key_event_once(
    ctx: &mut ShellRuntimeContext,
    event: KeyEvent,
) -> Result<(), ShellLoopError> {
    let active = ctx.app.overlays.top();
    let routed = route_key(event, active);
    let maybe_event = dispatch_routed_key(&mut ctx.app, routed)?;
    if let Some(ev) = maybe_event {
        match ev {
            AppEvent::PaletteSelected(slash) => {
                route_palette_command(
                    &mut ctx.app,
                    ctx.command_executor.as_ref(),
                    &slash,
                )?;
            }
            other => ctx.app.apply_event(other)?,
        }
    }
    ctx.app.prepare_frame();
    Ok(())
}

/// Convenience helper for callers that only have a `ShellApp` —
/// no command executor wired. Custom palette slashes still flow
/// through the command bridge but with `executor = None`, so
/// they consistently produce an `AppError` recorded in the
/// activity log instead of being silently swallowed.
pub fn handle_key_event_once_app_only(
    app: &mut ShellApp,
    event: KeyEvent,
) -> Result<(), ShellLoopError> {
    let active = app.overlays.top();
    let routed = route_key(event, active);
    let maybe_event = dispatch_routed_key(app, routed)?;
    if let Some(ev) = maybe_event {
        match ev {
            AppEvent::PaletteSelected(slash) => {
                route_palette_command(app, None, &slash)?;
            }
            other => app.apply_event(other)?,
        }
    }
    app.prepare_frame();
    Ok(())
}

/// Returns `true` when the operator pressed plain `q` (no Ctrl)
/// while no overlay is on top — the loop's quit gesture.
pub fn is_quit_key(event: KeyEvent, app: &ShellApp, options: ShellLoopOptions) -> bool {
    if !options.exit_on_q {
        return false;
    }
    if app.overlays.top() != vac_shell_contracts::ShellOverlay::None {
        return false;
    }
    matches!(event.code, KeyCode::Char('q') | KeyCode::Char('Q'))
        && !event.modifiers.contains(KeyModifiers::CONTROL)
        && event.kind == KeyEventKind::Press
}

/// RAII guard for crossterm raw-mode + alternate-screen state.
/// Constructed *after* both succeed; on drop (panic, error, or
/// normal exit) the terminal is restored. Hosts that wrap their
/// own terminal lifecycle don't need this — they keep their own
/// guard and call `handle_key_event_once` directly.
struct TerminalGuard;

impl TerminalGuard {
    fn install() -> std::io::Result<Self> {
        use crossterm::execute;
        use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
        enable_raw_mode()?;
        if let Err(e) = execute!(std::io::stdout(), EnterAlternateScreen) {
            // Roll raw mode back if alt-screen failed; otherwise
            // the operator is stuck in raw mode with no UI.
            let _ = crossterm::terminal::disable_raw_mode();
            return Err(e);
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use crossterm::execute;
        use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode};
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

/// Live event loop. Sets up crossterm raw mode + an alternate
/// screen behind a `TerminalGuard`, drives the context via
/// crossterm events, and tears down cleanly on quit / panic /
/// setup error. **Manual test only.**
pub fn run_shell_loop(
    ctx: ShellRuntimeContext,
    options: ShellLoopOptions,
) -> Result<ExitCode, ShellLoopError> {
    use crossterm::event::{self, Event};
    use ratatui::Terminal;
    use ratatui::backend::CrosstermBackend;

    let _guard = TerminalGuard::install()?;
    let stdout = std::io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut ctx = ctx;
    let tick = std::time::Duration::from_millis(options.tick_rate_ms.max(10));
    loop {
        terminal.draw(|f| ctx.app.render(f, f.area()))?;
        if event::poll(tick)? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                if is_quit_key(k, &ctx.app, options) {
                    return Ok(ExitCode::SUCCESS);
                }
                handle_key_event_once(&mut ctx, k)?;
            }
        } else {
            // Tick: refresh the projected approval bar from the
            // live queue.
            ctx.app.prepare_frame();
        }
    }
}
