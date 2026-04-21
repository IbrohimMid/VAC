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

/// Probe stdin/stdout for Kitty graphics support, gated on both handles
/// being TTYs. Returns `false` when either handle is redirected (CI,
/// piped input, captured tests) — this keeps the probe out of non-
/// interactive runs where the DCS bytes would leak into stdout.
///
/// Called once at TUI startup (after `enable_raw_mode()` and before
/// `EnterAlternateScreen`) so the raw bytes go to the real terminal,
/// not the alternate screen buffer. The result is then stored on
/// [`crate::capabilities::TerminalCapabilities::kitty_graphics`] and
/// [`crate::app::types::StartupSnapshot::kitty_graphics`] for renderers
/// to consult.
pub fn probe_terminal_kitty_support(deadline: Duration) -> bool {
    use std::io::IsTerminal;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return false;
    }
    matches!(
        probe_with_io(stdin.lock(), stdout.lock(), deadline),
        KittyProbe::Supported
    )
}

/// Dispatcher used by every image-rendering call site. When
/// `kitty_graphics` is true the caller is responsible for emitting the
/// Kitty protocol sequence itself (not implemented in this module); we
/// still return the ASCII frame so the caller has a guaranteed visual
/// fallback to overlay on or return directly.
///
/// Returns `(used_kitty, lines)`:
///   - `used_kitty=true` means the capability is present; callers should
///     prefer their native Kitty emission and may discard `lines`.
///   - `used_kitty=false` means the capability is absent; callers must
///     render the returned ASCII `lines` (already framed and centered).
pub fn render_image_or_fallback(
    caps: &crate::capabilities::TerminalCapabilities,
    width: u16,
    height: u16,
    label: &str,
) -> (bool, Vec<String>) {
    let lines = render_ascii_fallback(width, height, label);
    (caps.kitty_graphics, lines)
}

/// Max base64 payload length per Kitty graphics chunk, per
/// <https://sw.kovidgoyal.net/kitty/graphics-protocol/#a-minimal-example>.
/// Chunks larger than this may be silently dropped by some terminals.
pub const KITTY_CHUNK_BASE64_MAX: usize = 4096;

/// Emit an inline Kitty graphics sequence for a PNG payload using the
/// "direct transmission" protocol (`f=100,t=d,a=T,q=2`):
///   - `f=100`: PNG data
///   - `t=d`: direct transmission (payload is the image itself, base64-encoded)
///   - `a=T`: immediate display at the cursor
///   - `q=2`: suppress terminal responses (we don't poll for acks)
///
/// The base64-encoded payload is chunked into runs of at most
/// [`KITTY_CHUNK_BASE64_MAX`] bytes. A single-chunk image omits the `m`
/// key; a multi-chunk image uses `m=1` for every non-final chunk and
/// `m=0` on the terminator. Every chunk is wrapped in the Kitty DCS
/// bracket `\x1b_G<keys>;<payload>\x1b\\`.
///
/// Returns an empty vector when the input is empty so callers can
/// shortcut without emitting a stray escape sequence.
pub fn emit_kitty_inline_image(png_bytes: &[u8]) -> Vec<u8> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let mut out = Vec::new();
    if png_bytes.is_empty() {
        return out;
    }
    let encoded = STANDARD.encode(png_bytes);
    let bytes = encoded.as_bytes();
    let total = bytes.len();
    let single_chunk = total <= KITTY_CHUNK_BASE64_MAX;
    let mut offset: usize = 0;
    let mut first = true;
    while offset < total {
        let end = (offset + KITTY_CHUNK_BASE64_MAX).min(total);
        let chunk = &bytes[offset..end];
        let is_last = end == total;
        let header = if single_chunk {
            "f=100,t=d,a=T,q=2".to_string()
        } else if first {
            "f=100,t=d,a=T,q=2,m=1".to_string()
        } else if is_last {
            "m=0".to_string()
        } else {
            "m=1".to_string()
        };
        out.extend_from_slice(b"\x1b_G");
        out.extend_from_slice(header.as_bytes());
        out.push(b';');
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
        offset = end;
        first = false;
    }
    out
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

    #[test]
    fn dispatcher_reports_kitty_flag_and_returns_fallback_frame() {
        use crate::capabilities::{ImageProtocol, TerminalCapabilities, TerminalId};

        fn base_caps(kitty_graphics: bool) -> TerminalCapabilities {
            TerminalCapabilities {
                terminal_id: TerminalId::Unknown,
                truecolor: false,
                color_256: false,
                mouse: false,
                bracketed_paste: false,
                image_protocol: ImageProtocol::None,
                is_dark_theme: true,
                kitty_graphics,
            }
        }

        // Unsupported terminal: dispatcher must report `false` and still
        // hand back a non-empty ASCII frame sized to the requested rect.
        let (used_kitty, frame) =
            render_image_or_fallback(&base_caps(false), 16, 4, "hero.png");
        assert!(!used_kitty, "kitty_graphics=false must return false");
        assert_eq!(frame.len(), 4, "frame height must match requested height");
        assert!(
            frame.iter().any(|l| l.contains("hero.png")),
            "fallback must include label: {frame:?}"
        );

        // Supported terminal: dispatcher reports `true`. We still return a
        // pre-rendered fallback so the caller has something to draw even
        // if their Kitty emission path fails downstream.
        let (used_kitty, frame) =
            render_image_or_fallback(&base_caps(true), 16, 4, "hero.png");
        assert!(used_kitty, "kitty_graphics=true must return true");
        assert_eq!(frame.len(), 4);
    }

    #[test]
    fn probe_terminal_kitty_support_is_false_when_not_tty() {
        // The test harness captures stdout, so `IsTerminal` on stdout is
        // false here. The probe helper must short-circuit to `false`
        // without writing the DCS bytes or blocking on stdin.
        let start = Instant::now();
        let supported = probe_terminal_kitty_support(Duration::from_millis(200));
        assert!(
            !supported,
            "probe must return false when stdin/stdout is not a TTY"
        );
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "non-TTY probe must short-circuit before hitting the deadline: took {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn kitty_emitter_empty_input_emits_nothing() {
        // No PNG bytes → no DCS bracket at all. Prevents stray escapes from
        // bleeding into the terminal when a caller has nothing to draw.
        let out = emit_kitty_inline_image(&[]);
        assert!(out.is_empty(), "empty input must produce no output: {out:?}");
    }

    #[test]
    fn kitty_emitter_single_chunk_has_direct_header_without_m_key() {
        // Payload small enough to fit in one chunk (base64 of 4 bytes = 8 chars).
        let png = [0xDEu8, 0xAD, 0xBE, 0xEF];
        let out = emit_kitty_inline_image(&png);
        assert!(out.starts_with(b"\x1b_G"), "missing DCS open: {:?}", &out[..4.min(out.len())]);
        assert!(out.ends_with(b"\x1b\\"), "missing DCS close");
        let text = std::str::from_utf8(&out).expect("emitter output is ASCII");
        // Direct transmission header must include f=100, t=d, a=T, q=2.
        assert!(text.contains("f=100"), "missing f=100: {text:?}");
        assert!(text.contains("t=d"), "missing t=d: {text:?}");
        assert!(text.contains("a=T"), "missing a=T: {text:?}");
        assert!(text.contains("q=2"), "missing q=2: {text:?}");
        // Single-chunk frames must not carry the m= continuation flag.
        assert!(!text.contains("m="), "single-chunk must omit m= key: {text:?}");
        // Exactly one DCS bracket pair.
        let opens = text.matches("\x1b_G").count();
        let closes = text.matches("\x1b\\").count();
        assert_eq!(opens, 1, "expected exactly one DCS open: {opens}");
        assert_eq!(closes, 1, "expected exactly one DCS close: {closes}");
    }

    #[test]
    fn kitty_emitter_multi_chunk_uses_m1_then_m0_and_respects_chunk_size() {
        // Raw input large enough that base64 encoding exceeds the chunk
        // ceiling (4096 base64 chars ≈ 3072 raw bytes). 4000 bytes of raw
        // data encode to ~5332 base64 chars → exactly 2 chunks.
        let png: Vec<u8> = (0u16..4000u16).map(|i| (i & 0xFF) as u8).collect();
        let out = emit_kitty_inline_image(&png);
        let text = std::str::from_utf8(&out).expect("emitter output is ASCII");
        // Exactly 2 DCS brackets.
        let opens = text.matches("\x1b_G").count();
        assert_eq!(opens, 2, "expected 2 chunks for this payload: got {opens}");
        // First chunk must carry the full header *and* m=1.
        let first_open = text.find("\x1b_G").unwrap();
        let first_semi = first_open + text[first_open..].find(';').unwrap();
        let first_header = &text[first_open + 3..first_semi];
        assert!(first_header.contains("f=100"), "first header must carry f=100: {first_header:?}");
        assert!(first_header.contains("m=1"), "first header must carry m=1: {first_header:?}");
        // Last chunk must carry m=0 and no f=/t=/a=/q= (continuation only).
        let last_open = text.rfind("\x1b_G").unwrap();
        let last_semi = last_open + text[last_open..].find(';').unwrap();
        let last_header = &text[last_open + 3..last_semi];
        assert_eq!(last_header, "m=0", "terminator header must be exactly m=0: {last_header:?}");
        // Every chunk's base64 payload must respect the 4096-byte ceiling.
        let mut cursor = 0usize;
        while let Some(rel_open) = text[cursor..].find("\x1b_G") {
            let open = cursor + rel_open;
            let semi = open + text[open..].find(';').unwrap();
            let close = open + text[open..].find("\x1b\\").unwrap();
            let payload = &text[semi + 1..close];
            assert!(
                payload.len() <= KITTY_CHUNK_BASE64_MAX,
                "chunk base64 payload {} exceeds cap {}",
                payload.len(),
                KITTY_CHUNK_BASE64_MAX
            );
            cursor = close + 2;
        }
    }

    #[test]
    fn kitty_emitter_every_chunk_wraps_in_dcs_bracket() {
        // Force a payload that spans 3 chunks (~9k raw bytes → ~12k base64).
        let png: Vec<u8> = (0..9000u32).map(|i| (i & 0xFF) as u8).collect();
        let out = emit_kitty_inline_image(&png);
        let text = std::str::from_utf8(&out).expect("emitter output is ASCII");
        let opens = text.matches("\x1b_G").count();
        let closes = text.matches("\x1b\\").count();
        assert_eq!(opens, closes, "every DCS open must have a matching close");
        assert_eq!(opens, 3, "expected 3 chunks for ~9k byte input: got {opens}");
        // Chunks 2..N-1 must be pure m=1 continuations; chunk N must be m=0.
        let mut headers = Vec::new();
        let mut cursor = 0usize;
        while let Some(rel_open) = text[cursor..].find("\x1b_G") {
            let open = cursor + rel_open;
            let semi = open + text[open..].find(';').unwrap();
            headers.push(text[open + 3..semi].to_string());
            cursor = semi + 1;
        }
        assert!(headers[0].contains("m=1") && headers[0].contains("f=100"));
        assert_eq!(headers[1], "m=1");
        assert_eq!(headers[2], "m=0");
    }

    #[test]
    fn with_kitty_graphics_flips_field_without_touching_others() {
        use crate::capabilities::TerminalCapabilities;
        let detected = TerminalCapabilities::detect_uncached();
        assert!(
            !detected.kitty_graphics,
            "detect_uncached must default kitty_graphics to false"
        );
        let flipped = detected.clone().with_kitty_graphics(true);
        assert!(flipped.kitty_graphics);
        // Other fields remain identical.
        assert_eq!(flipped.terminal_id, detected.terminal_id);
        assert_eq!(flipped.truecolor, detected.truecolor);
        assert_eq!(flipped.image_protocol, detected.image_protocol);
    }
}
