//! F7.9 — `/rewind` command + session scrubbing state.
//!
//! The operator can step back through prior user submits during the
//! current session. This module owns:
//!
//! - [`RewindState`] — cursor + active-flag state sitting next to
//!   the messages list.
//! - [`parse_rewind_command`] — parser for `/rewind [N]` that the
//!   slash dispatcher calls.
//!
//! Actual replay (resetting messages, re-priming the engine) happens
//! in the driver — this module just owns the contract.

/// Parsed form of `/rewind [N]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewindCommand {
    /// Number of user submits to step back. Defaults to 1 when the
    /// operator types `/rewind` with no argument. Always >= 1.
    pub steps: u32,
}

/// Errors from [`parse_rewind_command`].
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RewindParseError {
    NotRewind,
    InvalidArg,
    ZeroSteps,
}

impl std::fmt::Display for RewindParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRewind => f.write_str("not a rewind command"),
            Self::InvalidArg => f.write_str("argument is not a positive integer"),
            Self::ZeroSteps => f.write_str("rewind steps must be >= 1"),
        }
    }
}

impl std::error::Error for RewindParseError {}

pub fn parse_rewind_command(input: &str) -> Result<RewindCommand, RewindParseError> {
    let trimmed = input.trim_start();
    let rest = trimmed
        .strip_prefix("/rewind")
        .ok_or(RewindParseError::NotRewind)?;
    let arg = rest.trim();
    if arg.is_empty() {
        return Ok(RewindCommand { steps: 1 });
    }
    let n: u32 = arg.parse().map_err(|_| RewindParseError::InvalidArg)?;
    if n == 0 {
        return Err(RewindParseError::ZeroSteps);
    }
    Ok(RewindCommand { steps: n })
}

/// Scrubbing state — how many steps back the operator is viewing,
/// and whether a rewind preview is currently active.
#[derive(Debug, Clone, Copy, Default)]
pub struct RewindState {
    /// 0 = live (most recent). Positive N = viewing N user-submits
    /// earlier. Bounded on set by the driver against current message
    /// history length.
    pub cursor: u32,
    /// True while the operator is previewing a rewound state but has
    /// not yet committed (confirming resets the engine).
    pub previewing: bool,
}

impl RewindState {
    /// Move the cursor `steps` further back. Saturates at `max` so
    /// the driver can pass `pending_user_submit_count - 1` to stop
    /// at the first prompt.
    pub fn step_back(&mut self, steps: u32, max: u32) {
        self.cursor = self.cursor.saturating_add(steps).min(max);
        self.previewing = self.cursor > 0;
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
        self.previewing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewind_without_arg_defaults_to_one_step() {
        let r = parse_rewind_command("/rewind").unwrap();
        assert_eq!(r.steps, 1);
    }

    #[test]
    fn rewind_with_arg_parses_count() {
        let r = parse_rewind_command("/rewind 4").unwrap();
        assert_eq!(r.steps, 4);
    }

    #[test]
    fn rewind_zero_is_rejected() {
        let e = parse_rewind_command("/rewind 0").unwrap_err();
        assert_eq!(e, RewindParseError::ZeroSteps);
    }

    #[test]
    fn rewind_non_numeric_arg_is_invalid() {
        let e = parse_rewind_command("/rewind abc").unwrap_err();
        assert_eq!(e, RewindParseError::InvalidArg);
    }

    #[test]
    fn non_rewind_input_is_rejected() {
        assert_eq!(
            parse_rewind_command("/help").unwrap_err(),
            RewindParseError::NotRewind
        );
    }

    #[test]
    fn state_step_back_saturates_at_max() {
        let mut s = RewindState::default();
        s.step_back(10, 3);
        assert_eq!(s.cursor, 3);
        assert!(s.previewing);
    }

    #[test]
    fn state_reset_drops_preview() {
        let mut s = RewindState {
            cursor: 5,
            previewing: true,
        };
        s.reset();
        assert_eq!(s.cursor, 0);
        assert!(!s.previewing);
    }
}
