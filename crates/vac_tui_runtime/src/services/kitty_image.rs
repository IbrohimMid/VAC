//! Kitty graphics protocol detection + ASCII fallback (PR-T17).
//!
//! Flow:
//!   1. Call [`detect_kitty_support`] at startup. It sends the Kitty
//!      "query image capability" DCS escape (`\e_Gi=31,s=1,v=1,a=q\e\\`)
//!      followed by a READY marker, then blocks reading the terminal's
//!      response up to a fixed deadline (default 200ms).
//!   2. If the response contains the capability reply, we flag
//!      [`KittyProbe::Supported`]; on any timeout, I/O error, or plain
//!      non-kitty response we fall back to [`KittyProbe::Unsupported`].
//!   3. [`render_ascii_fallback`] returns a placeholder block describing
//!      the intended image — the renderer draws this when Kitty is
//!      unsupported.
//!
//! I/O is parameterised over [`std::io::Read`] + [`std::io::Write`] so the
//! entire detection state machine is testable with in-memory cursors.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// Sentinel printed after the DCS query. The detection loop returns as
/// soon as it sees this marker, guaranteeing the capability reply (if any)
/// arrived first and we won't block indefinitely.
pub const READY_MARKER: &str = "\x1b[?KITTY-READY\x07";

/// Kitty capability query per <https://sw.kovidgoyal.net/kitty/graphics-protocol/>.
pub const KITTY_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q\x1b\\";

/// Default round-trip deadline. Short enough to feel instant at TUI
/// startup, long enough to clear the slowest mux/ssh paths we've seen.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_millis(200);

/// Outcome of probing the terminal for Kitty graphics support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyProbe {
    Supported,
    Unsupported,
}

/// Run one detection round against an arbitrary I/O pair. Returns
/// `KittyProbe::Unsupported` on timeout, short reads, or any transport
/// error — detection failures must never crash the UI.
///
/// The algorithm:
///   1. Write `KITTY_QUERY` then `READY_MARKER`.
///   2. Loop-read into a small buffer until either the READY marker is
///      observed in the accumulated reply or the deadline expires.
///   3. Scan the pre-marker slice for the Kitty reply prefix `\x1b_Gi=31;`.
pub fn probe_with_io<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    deadline: Duration,
) -> KittyProbe {
    if output.write_all(KITTY_QUERY.as_bytes()).is_err() {
        return KittyProbe::Unsupported;
    }
    if output.write_all(READY_MARKER.as_bytes()).is_err() {
        return KittyProbe::Unsupported;
    }
    if output.flush().is_err() {
        return KittyProbe::Unsupported;
    }

    let start = Instant::now();
    let mut buf = Vec::with_capacity(256);
    let mut chunk = [0u8; 64];
    while start.elapsed() < deadline {
        match input.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(marker_pos) = find_subslice(&buf, READY_MARKER.as_bytes()) {
                    return scan_reply(&buf[..marker_pos]);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Non-blocking stream: yield briefly then retry.
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return KittyProbe::Unsupported,
        }
    }
    // Timed out. Even if we have *some* reply, without the marker we can't
    // be sure we're not reading the user's typeahead — safer to fall back.
    KittyProbe::Unsupported
}

fn scan_reply(pre_marker: &[u8]) -> KittyProbe {
    // A successful reply looks like: `\x1b_Gi=31;OK\x1b\\`
    // We only need to confirm the opening token; the tail is terminal-specific.
    if find_subslice(pre_marker, b"\x1b_Gi=31;").is_some() {
        KittyProbe::Supported
    } else {
        KittyProbe::Unsupported
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

/// A minimal ASCII placeholder the renderer can draw inside a `Rect`
/// allocated for an image. Returns `height` lines of `width` characters.
/// The `label` (e.g. a filename) is centered; a border frames the block so
/// the user can tell an image “would go here” on non-Kitty terminals.
pub fn render_ascii_fallback(width: u16, height: u16, label: &str) -> Vec<String> {
    let width = width.max(4) as usize;
    let height = height.max(3) as usize;
    let inner_width = width - 2;
    let top = format!("┌{}┐", "─".repeat(inner_width));
    let bottom = format!("└{}┘", "─".repeat(inner_width));

    let mut lines = Vec::with_capacity(height);
    lines.push(top);

    let body_rows = height - 2;
    let label_row = body_rows / 2;
    for i in 0..body_rows {
        let content = if i == label_row {
            centered_label(label, inner_width)
        } else {
            " ".repeat(inner_width)
        };
        lines.push(format!("│{content}│"));
    }
    lines.push(bottom);
    lines
}

fn centered_label(label: &str, width: usize) -> String {
    // Truncate with an ellipsis if the label is too long for the frame.
    let (text, actual) = if label.chars().count() > width {
        if width <= 1 {
            ("…".to_string(), 1)
        } else {
            let take = width - 1;
            let truncated: String = label.chars().take(take).collect();
            (format!("{truncated}…"), take + 1)
        }
    } else {
        (label.to_string(), label.chars().count())
    };
    let pad = width.saturating_sub(actual);
    let left = pad / 2;
    let right = pad - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    /// Helper: build an in-memory reader that returns the given bytes
    /// slowly enough to force the timeout path. Because `Cursor` is
    /// always-ready, we use a wrapper that returns WouldBlock after the
    /// buffered bytes drain.
    struct SlowReader {
        buf: Vec<u8>,
        pos: usize,
        stall_after: usize,
    }
    impl SlowReader {
        fn new(buf: Vec<u8>, stall_after: usize) -> Self {
            Self { buf, pos: 0, stall_after }
        }
    }
    impl Read for SlowReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.stall_after {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "would block",
                ));
            }
            let take = out
                .len()
                .min(self.stall_after - self.pos)
                .min(self.buf.len() - self.pos);
            if take == 0 {
                return Ok(0);
            }
            out[..take].copy_from_slice(&self.buf[self.pos..self.pos + take]);
            self.pos += take;
            Ok(take)
        }
    }

    #[test]
    fn kitty_detect_succeeds_on_valid_reply() {
        // Kitty-style response: capability reply followed by our READY marker.
        let reply = format!("\x1b_Gi=31;OK\x1b\\{}", READY_MARKER);
        let input = Cursor::new(reply.into_bytes());
        let mut output: Vec<u8> = Vec::new();
        let result = probe_with_io(input, &mut output, Duration::from_millis(100));
        assert_eq!(result, KittyProbe::Supported);
        // Sanity: we sent the query and the marker.
        assert!(output.windows(KITTY_QUERY.len()).any(|w| w == KITTY_QUERY.as_bytes()));
        assert!(output
            .windows(READY_MARKER.len())
            .any(|w| w == READY_MARKER.as_bytes()));
    }

    #[test]
    fn kitty_detect_times_out_gracefully() {
        // Reader yields nothing — every read returns WouldBlock. Must
        // return Unsupported before panicking or blocking forever.
        let input = SlowReader::new(Vec::new(), 0);
        let mut output: Vec<u8> = Vec::new();
        let start = Instant::now();
        let result = probe_with_io(input, &mut output, Duration::from_millis(30));
        assert_eq!(result, KittyProbe::Unsupported);
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "detection must not block past the deadline"
        );
    }

    #[test]
    fn kitty_detect_non_matching_reply_is_unsupported() {
        // Terminal echoes the READY marker but never sent a capability reply
        // (e.g. xterm, gnome-terminal). Result must be Unsupported.
        let reply = READY_MARKER.to_string();
        let input = Cursor::new(reply.into_bytes());
        let mut output: Vec<u8> = Vec::new();
        let result = probe_with_io(input, &mut output, Duration::from_millis(50));
        assert_eq!(result, KittyProbe::Unsupported);
    }

    #[test]
    fn kitty_detect_write_failure_is_unsupported() {
        // Zero-capacity writer fails on the first write — must surface as
        // Unsupported rather than propagating the error.
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "nope"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let result = probe_with_io(Cursor::new(Vec::new()), FailingWriter, Duration::from_millis(10));
        assert_eq!(result, KittyProbe::Unsupported);
    }

    #[test]
    fn fallback_renders_ascii() {
        let block = render_ascii_fallback(20, 5, "logo.png");
        assert_eq!(block.len(), 5);
        assert!(block[0].starts_with('┌') && block[0].ends_with('┐'));
        assert!(block[4].starts_with('└') && block[4].ends_with('┘'));
        // One of the middle rows must contain the label.
        let has_label = block.iter().any(|l| l.contains("logo.png"));
        assert!(has_label, "ASCII fallback must display the label: {block:?}");
        // Every row has the same visible width.
        let widths: Vec<usize> = block.iter().map(|l| l.chars().count()).collect();
        assert!(widths.iter().all(|w| *w == 20), "inconsistent widths: {widths:?}");
    }

    #[test]
    fn fallback_truncates_oversized_labels() {
        let block = render_ascii_fallback(10, 3, "a-really-long-filename.png");
        // Label row contains an ellipsis character and fits the frame.
        let widths: Vec<usize> = block.iter().map(|l| l.chars().count()).collect();
        assert!(widths.iter().all(|w| *w == 10));
        assert!(block.iter().any(|l| l.contains('…')));
    }

    #[test]
    fn fallback_enforces_minimum_size() {
        // width<4, height<3 — must not panic or produce broken frames.
        let block = render_ascii_fallback(2, 1, "x");
        assert_eq!(block.len(), 3);
        for row in &block {
            assert_eq!(row.chars().count(), 4);
        }
    }
}
