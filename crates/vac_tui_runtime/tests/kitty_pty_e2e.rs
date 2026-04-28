//! PTY-based (mock TTY) e2e tests for the Kitty probe chain (PR-T17 Task-9).
//!
//! `probe_with_io` accepts `impl Read + Write` so the whole detection state
//! machine is exercisable without a real terminal. We use in-memory cursors
//! as mock TTY fixtures to cover the three required paths:
//!   1. Reply matches → Supported
//!   2. Timeout before reply → Unsupported
//!   3. Reply does not contain the Kitty capability prefix → Unsupported

use std::io::{Cursor, ErrorKind, Read};
use std::time::Duration;
use vac_tui_runtime::services::kitty_image::{KittyProbe, READY_MARKER, probe_with_io};

// ── Mock TTY helper ───────────────────────────────────────────────────────

/// A mock PTY that stalls after the initial buffered bytes are read,
/// simulating a terminal that never sends any more data (timeout path).
struct MockPtyReader {
    inner: Vec<u8>,
    pos: usize,
    stall_after: usize,
}

impl MockPtyReader {
    fn empty() -> Self {
        Self {
            inner: Vec::new(),
            pos: 0,
            stall_after: 0,
        }
    }
}

impl Read for MockPtyReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.stall_after {
            // Signal WouldBlock to simulate a non-blocking TTY with nothing more to read.
            return Err(std::io::Error::new(ErrorKind::WouldBlock, "pty stalled"));
        }
        let take = out.len().min(self.stall_after - self.pos);
        if take == 0 {
            return Ok(0);
        }
        out[..take].copy_from_slice(&self.inner[self.pos..self.pos + take]);
        self.pos += take;
        Ok(take)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[test]
fn probe_detects_kitty_over_pty_when_reply_matches() {
    // Mock PTY returns: Kitty capability reply + READY marker.
    // This exercises the full happy path: write query → read reply → scan.
    let pty_reply = format!("\x1b_Gi=31;OK\x1b\\{}", READY_MARKER);
    let input = Cursor::new(pty_reply.into_bytes());
    let mut output: Vec<u8> = Vec::new();
    let result = probe_with_io(input, &mut output, Duration::from_millis(100));
    assert_eq!(
        result,
        KittyProbe::Supported,
        "probe must detect Kitty support when the mock PTY returns the capability reply"
    );
    // Verify the probe actually wrote the query to the mock PTY.
    let out_str = String::from_utf8_lossy(&output);
    assert!(
        out_str.contains("\x1b_G"),
        "probe must have written the DCS query to the output handle"
    );
}

#[test]
fn probe_rejects_kitty_over_pty_on_timeout() {
    // Mock PTY returns nothing at all — every read immediately stalls.
    // The probe must give up once the deadline expires.
    let input = MockPtyReader::empty();
    let mut output: Vec<u8> = Vec::new();
    let start = std::time::Instant::now();
    let result = probe_with_io(input, &mut output, Duration::from_millis(40));
    assert_eq!(
        result,
        KittyProbe::Unsupported,
        "probe must return Unsupported when the mock PTY never responds"
    );
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "probe must not block beyond the deadline: took {:?}",
        start.elapsed()
    );
}

#[test]
fn probe_rejects_kitty_over_pty_on_mismatched_reply() {
    // Mock PTY returns the READY marker but without any Kitty capability
    // prefix (e.g. xterm or a plain VT100 that ignores unknown DCS sequences).
    let pty_reply = format!("garbage-reply{}", READY_MARKER);
    let input = Cursor::new(pty_reply.into_bytes());
    let mut output: Vec<u8> = Vec::new();
    let result = probe_with_io(input, &mut output, Duration::from_millis(100));
    assert_eq!(
        result,
        KittyProbe::Unsupported,
        "probe must return Unsupported when the terminal reply lacks the Kitty capability prefix"
    );
}
