//! D2.2 — minimal runtime loop harness.
//!
//! Connects `vac_shell_keymap::route_key` + `dispatch_routed_key`
//! to a `ShellApp`. Hosts that want to drive the cockpit
//! interactively call `run_shell_loop`; tests exercise the
//! per-tick step via `handle_key_event_once`.
//!
//! No engine coupling, no legacy runtime, no donor. The loop
//! crate exists so `vac_shell_app` and the widget crates stay
//! free of ratatui Terminal / crossterm raw-mode I/O.

use std::process::ExitCode;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use vac_shell_app::{AppError, ShellApp};
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

/// Step a single key event through routing → dispatch →
/// `apply_event` → `prepare_frame`. Tests drive the loop one key
/// at a time via this entry; the live loop calls it from the
/// crossterm poller.
pub fn handle_key_event_once(
    app: &mut ShellApp,
    event: KeyEvent,
) -> Result<(), ShellLoopError> {
    let active = app.overlays.top();
    let routed = route_key(event, active);
    let maybe_event = dispatch_routed_key(app, routed)?;
    if let Some(ev) = maybe_event {
        app.apply_event(ev)?;
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

/// Live event loop. Sets up crossterm raw mode + an alternate
/// screen, drives the app via crossterm events, and tears down
/// cleanly on quit. Hosts that already own a terminal and a
/// raw-mode RAII guard should drive `handle_key_event_once`
/// directly from their existing loop instead.
///
/// **Manual test only.** No automated test exercises the
/// blocking poller; all event handling is covered by the unit
/// tests for `handle_key_event_once`.
pub fn run_shell_loop(
    mut app: ShellApp,
    options: ShellLoopOptions,
) -> Result<ExitCode, ShellLoopError> {
    use crossterm::event::{self, Event};
    use crossterm::execute;
    use crossterm::terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    };
    use ratatui::Terminal;
    use ratatui::backend::CrosstermBackend;

    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick = std::time::Duration::from_millis(options.tick_rate_ms.max(10));
    let result = (|| -> Result<ExitCode, ShellLoopError> {
        loop {
            terminal.draw(|f| app.render(f, f.area()))?;
            if event::poll(tick)? {
                if let Event::Key(k) = event::read()? {
                    if k.kind != KeyEventKind::Press {
                        continue;
                    }
                    if is_quit_key(k, &app, options) {
                        break Ok(ExitCode::SUCCESS);
                    }
                    handle_key_event_once(&mut app, k)?;
                }
            } else {
                // Tick: still refresh the projected approval bar
                // from the live queue.
                app.prepare_frame();
            }
        }
    })();

    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    result
}
