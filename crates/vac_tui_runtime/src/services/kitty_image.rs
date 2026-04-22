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
    haystack.windows(needle.len()).position(|w| w == needle)
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

/// PR-T17 M3/L5 — compute a stable 64-bit content hash for a PNG byte
/// slice. Used by the event-loop Kitty flush to dedup identical emissions
/// across consecutive frames (see `AppState::last_kitty_emission`). We use
/// `DefaultHasher` because (a) it's already in std, (b) PNG bytes are not
/// security-sensitive here (we only care about collision probability on
/// *accidental* different payloads), and (c) hashing a few MB once per
/// frame is still cheaper by orders of magnitude than re-encoding base64
/// and writing ~4/3× the bytes back to stdout.
pub fn hash_png_payload(png_bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    png_bytes.hash(&mut hasher);
    hasher.finish()
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

/// R8c: build a single byte buffer that first positions the cursor at
/// `(col, row)` using an ANSI CSI `CUP` sequence (1-based) and then
/// appends the full Kitty DCS emission returned by
/// [`emit_kitty_inline_image`]. Callers write the combined buffer to
/// stdout *after* `terminal.draw()` finishes so ratatui never sees the
/// escape bytes; the alt-screen buffer and ratatui's internal diff are
/// unaffected.
///
/// Returns an empty buffer when `png_bytes` is empty (matches the
/// behaviour of `emit_kitty_inline_image` so the caller can no-op
/// without special casing).
///
/// `col` and `row` are zero-based cell coordinates inside the terminal
/// viewport — the helper adds 1 internally to conform to CSI CUP.
pub fn emit_positioned_kitty_image(col: u16, row: u16, png_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if png_bytes.is_empty() {
        return out;
    }
    // CSI CUP: ESC [ row ; col H (1-based).
    let cursor = format!("\x1b[{};{}H", row.saturating_add(1), col.saturating_add(1));
    out.extend_from_slice(cursor.as_bytes());
    out.extend_from_slice(&emit_kitty_inline_image(png_bytes));
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

pub mod cache;

#[cfg(test)]
mod tests;
