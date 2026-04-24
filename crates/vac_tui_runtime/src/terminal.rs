/// RAII guard that restores terminal state on drop.
///
/// **Note:** under `panic = "abort"` (release profile), `Drop` is not
/// called. The panic hook in `vac_cli::telemetry` handles terminal
/// restoration explicitly for that path.
///
/// Must match the enables in `event_loop::run_tui`:
///   `enable_raw_mode`, `EnterAlternateScreen`, `EnableMouseCapture`,
///   `EnableBracketedPaste`. Any enable without a matching disable
///   here leaks escape sequences back into the cooked terminal after
///   TUI exit — e.g. mouse tracking bytes printed as plain text.
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Order matters: disable mouse / bracketed paste *before*
        // leaving the alternate screen so the terminating sequences
        // land in the alt-screen context crossterm emitted them in.
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
            crossterm::terminal::LeaveAlternateScreen,
        );
        let _ = crossterm::terminal::disable_raw_mode();
    }
}
