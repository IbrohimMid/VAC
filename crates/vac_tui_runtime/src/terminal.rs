/// RAII guard that restores terminal state on drop.
///
/// **Note:** under `panic = "abort"` (release profile), `Drop` is not
/// called. The panic hook in `vac_cli::telemetry` handles terminal
/// restoration explicitly for that path.
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    }
}
