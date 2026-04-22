//! Live linter for `vil-expr:` expressions in the input bar (PR-T12).
//!
//! This module is a *pure* state machine. It holds the most recently seen
//! draft expression plus the last debounce deadline, and exposes two entry
//! points:
//!
//! - [`LintState::on_input_changed`] — called by the input handler whenever
//!   the user types a character. It extracts the `vil-expr:` payload (if any)
//!   and schedules a lint for `DEBOUNCE` milliseconds later.
//! - [`LintState::tick`] — called by the event loop on every frame. If the
//!   deadline has elapsed it runs [`lint_expression`] and stores the result in
//!   `issues`. Until the deadline elapses, `issues` is unchanged, so rapid
//!   typing cannot thrash the UI.
//!
//! The default debounce is 200 ms, matching the PR-T12 plan spec. Tests
//! override the clock by injecting `Instant` values directly, so they are
//! fully deterministic without `tokio::time::pause`.

use std::time::{Duration, Instant};

use vil_expr::{SymbolTable, ValidationIssue, ValidationReport, parse, validate};

/// Debounce window between the last keystroke and the lint run.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// Prefix that activates the linter in the input bar. Anything typed after
/// this marker (until end of input / newline) is treated as a vil-expr draft.
pub const TRIGGER_PREFIX: &str = "vil-expr:";

/// Extract the trailing `vil-expr:` payload from an input buffer, if any.
///
/// Only the *last* occurrence wins — this lets the user compose a prose
/// prompt and then tack on `… vil-expr: foo.bar == 3` at the end. Leading
/// whitespace after the marker is trimmed; trailing whitespace is preserved
/// so the user's cursor position isn't visually changed.
pub fn extract_payload(raw: &str) -> Option<&str> {
    let idx = raw.rfind(TRIGGER_PREFIX)?;
    let rest = &raw[idx + TRIGGER_PREFIX.len()..];
    // Stop at the first newline — vil-expr is a single-line expression.
    let end = rest.find('\n').unwrap_or(rest.len());
    Some(rest[..end].trim_start())
}

/// Outcome of one synchronous lint run.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LintOutcome {
    /// No payload detected — caller should clear issues.
    #[default]
    NoPayload,
    /// Payload empty (just `vil-expr:` with no body) — also clear issues.
    Empty,
    /// Parser rejected the payload outright.
    ParseError { message: String },
    /// Validator returned a report (may be empty = valid).
    Validated(ValidationReport),
}

impl LintOutcome {
    /// Convenience: flatten to a `Vec<ValidationIssue>` suitable for rendering.
    pub fn issues(&self) -> Vec<ValidationIssue> {
        use vil_expr::Severity;
        match self {
            LintOutcome::NoPayload | LintOutcome::Empty => Vec::new(),
            LintOutcome::ParseError { message } => vec![ValidationIssue {
                line: 1,
                col: 1,
                severity: Severity::Error,
                message: format!("parse error: {message}"),
            }],
            LintOutcome::Validated(report) => report.issues.clone(),
        }
    }

    pub fn is_clean(&self) -> bool {
        match self {
            LintOutcome::NoPayload | LintOutcome::Empty => true,
            LintOutcome::ParseError { .. } => false,
            LintOutcome::Validated(r) => r.is_valid(),
        }
    }
}

/// Run parse + validate on a single payload string. Pure; no debounce.
pub fn lint_expression(payload: &str, symbols: &SymbolTable) -> LintOutcome {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return LintOutcome::Empty;
    }
    match parse(trimmed) {
        Ok(expr) => LintOutcome::Validated(validate(&expr, trimmed, symbols)),
        Err(err) => LintOutcome::ParseError {
            message: format!("{err:?}"),
        },
    }
}

/// Debounced lint state — owned by `AppState`.
#[derive(Debug, Clone, Default)]
pub struct LintState {
    /// Last payload extracted from the input bar (without the prefix).
    pub pending_payload: Option<String>,
    /// When `Some(t)`, a lint is scheduled to run at or after `t`.
    pub deadline: Option<Instant>,
    /// Most recent lint outcome; rendered by the view layer.
    pub last_outcome: LintOutcome,
}

impl LintState {
    pub fn new() -> Self {
        Self {
            pending_payload: None,
            deadline: None,
            last_outcome: LintOutcome::NoPayload,
        }
    }

    /// Register a new input snapshot. Re-scheduling the deadline means that
    /// rapid typing delays the actual lint until the user pauses — this is
    /// the debounce behaviour required by PR-T12.
    pub fn on_input_changed(&mut self, raw_input: &str, now: Instant) {
        match extract_payload(raw_input) {
            Some(payload) => {
                self.pending_payload = Some(payload.to_string());
                self.deadline = Some(now + DEBOUNCE);
            }
            None => {
                // No `vil-expr:` prefix anywhere; drop any pending work and
                // clear the outcome so the overlay disappears.
                self.pending_payload = None;
                self.deadline = None;
                self.last_outcome = LintOutcome::NoPayload;
            }
        }
    }

    /// Returns `true` if `tick` would run the linter right now.
    pub fn is_ready(&self, now: Instant) -> bool {
        match (self.deadline, &self.pending_payload) {
            (Some(d), Some(_)) => now >= d,
            _ => false,
        }
    }

    /// Run the pending lint if the debounce window has elapsed. Returns
    /// `true` if a lint was actually executed (callers can use this to decide
    /// whether to redraw).
    pub fn tick(&mut self, symbols: &SymbolTable, now: Instant) -> bool {
        if !self.is_ready(now) {
            return false;
        }
        // Copy out the payload so we can consume the borrow.
        let payload = self.pending_payload.clone().unwrap_or_default();
        self.last_outcome = lint_expression(&payload, symbols);
        // Clear the deadline so subsequent ticks are no-ops until the next
        // keystroke; the payload stays so the outcome keeps rendering.
        self.deadline = None;
        true
    }

    /// Issues to render in the overlay.
    pub fn issues(&self) -> Vec<ValidationIssue> {
        self.last_outcome.issues()
    }

    /// True if the current outcome is "clean" (no errors).
    pub fn is_clean(&self) -> bool {
        self.last_outcome.is_clean()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use vil_expr::Severity;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn extract_payload_handles_prefix_and_newlines() {
        assert_eq!(extract_payload("vil-expr: foo"), Some("foo"));
        assert_eq!(
            extract_payload("please lint vil-expr: a + b"),
            Some("a + b")
        );
        assert_eq!(
            extract_payload("vil-expr: first\nvil-expr: second"),
            Some("second")
        );
        assert_eq!(extract_payload("vil-expr: body\ntrailing"), Some("body"));
        assert_eq!(extract_payload("no marker here"), None);
    }

    #[test]
    fn lint_ok_on_valid_expression() {
        let symbols = SymbolTable::new();
        let out = lint_expression("foo + 1", &symbols);
        match &out {
            LintOutcome::Validated(report) => assert!(
                report.is_valid(),
                "expected valid report, got {:?}",
                report.issues
            ),
            other => panic!("expected Validated, got {other:?}"),
        }
        assert!(out.is_clean());
        assert!(out.issues().is_empty());
    }

    #[test]
    fn lint_detects_unknown_identifier() {
        let symbols = SymbolTable::new();
        let out = lint_expression("unknown_ident", &symbols);
        let issues = out.issues();
        assert_eq!(issues.len(), 1, "expected exactly one issue: {issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("Unknown identifier"),
            "unexpected message: {}",
            issues[0].message
        );
        assert!(!out.is_clean());
    }

    #[test]
    fn lint_debounces_rapid_typing() {
        let symbols = SymbolTable::new();
        let start = t0();
        let mut state = LintState::new();

        // Burst of keystrokes 50 ms apart — each bumps the deadline forward.
        // Final payload is `unknown_ident` which the stub SymbolTable
        // explicitly rejects, so a successful lint will yield one error.
        state.on_input_changed("vil-expr: u", start);
        state.on_input_changed("vil-expr: un", start + Duration::from_millis(50));
        state.on_input_changed("vil-expr: unkno", start + Duration::from_millis(100));
        state.on_input_changed(
            "vil-expr: unknown_ident",
            start + Duration::from_millis(150),
        );

        // Total elapsed: 150 ms. Deadline = 150 + 200 = 350 ms from start.
        // Ticking at 250 ms (100 ms into the debounce window) must NOT lint.
        assert!(
            !state.tick(&symbols, start + Duration::from_millis(250)),
            "tick fired before debounce window elapsed"
        );
        // last_outcome must still be the initial NoPayload — no premature run.
        assert!(matches!(state.last_outcome, LintOutcome::NoPayload));

        // Ticking at 400 ms (past the 350 ms deadline) must lint exactly once.
        assert!(state.tick(&symbols, start + Duration::from_millis(400)));
        // Deadline cleared — a second tick is a no-op.
        assert!(!state.tick(&symbols, start + Duration::from_millis(500)));

        // Payload at the moment the debounce fired was `unknown_ident`, which
        // the stub symbol table explicitly rejects, so we expect exactly one
        // error.
        let issues = state.last_outcome.issues();
        assert_eq!(issues.len(), 1, "got {issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
    }

    #[test]
    fn removing_prefix_clears_state() {
        let symbols = SymbolTable::new();
        let start = t0();
        let mut state = LintState::new();

        state.on_input_changed("vil-expr: unknown_ident", start);
        state.tick(&symbols, start + Duration::from_millis(300));
        assert!(!state.is_clean());

        // User deletes the marker — everything should reset synchronously.
        state.on_input_changed("just prose now", start + Duration::from_millis(400));
        assert!(state.is_clean());
        assert!(state.issues().is_empty());
        assert!(state.pending_payload.is_none());
    }

    #[test]
    fn empty_payload_is_clean() {
        let symbols = SymbolTable::new();
        let out = lint_expression("   ", &symbols);
        assert!(matches!(out, LintOutcome::Empty));
        assert!(out.is_clean());
        assert!(out.issues().is_empty());
    }
}
