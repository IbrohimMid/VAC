use super::*;
use std::io::{Cursor, Read, Write};
use std::time::{Duration, Instant};

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
        Self {
            buf,
            pos: 0,
            stall_after,
        }
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
    assert!(
        output
            .windows(KITTY_QUERY.len())
            .any(|w| w == KITTY_QUERY.as_bytes())
    );
    assert!(
        output
            .windows(READY_MARKER.len())
            .any(|w| w == READY_MARKER.as_bytes())
    );
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
    let result = probe_with_io(
        Cursor::new(Vec::new()),
        FailingWriter,
        Duration::from_millis(10),
    );
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
    assert!(
        has_label,
        "ASCII fallback must display the label: {block:?}"
    );
    // Every row has the same visible width.
    let widths: Vec<usize> = block.iter().map(|l| l.chars().count()).collect();
    assert!(
        widths.iter().all(|w| *w == 20),
        "inconsistent widths: {widths:?}"
    );
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
    let (used_kitty, frame) = render_image_or_fallback(&base_caps(false), 16, 4, "hero.png");
    assert!(!used_kitty, "kitty_graphics=false must return false");
    assert_eq!(frame.len(), 4, "frame height must match requested height");
    assert!(
        frame.iter().any(|l| l.contains("hero.png")),
        "fallback must include label: {frame:?}"
    );

    // Supported terminal: dispatcher reports `true`. We still return a
    // pre-rendered fallback so the caller has something to draw even
    // if their Kitty emission path fails downstream.
    let (used_kitty, frame) = render_image_or_fallback(&base_caps(true), 16, 4, "hero.png");
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
    assert!(
        out.is_empty(),
        "empty input must produce no output: {out:?}"
    );
}

#[test]
fn kitty_emitter_single_chunk_has_direct_header_without_m_key() {
    // Payload small enough to fit in one chunk (base64 of 4 bytes = 8 chars).
    let png = [0xDEu8, 0xAD, 0xBE, 0xEF];
    let out = emit_kitty_inline_image(&png);
    assert!(
        out.starts_with(b"\x1b_G"),
        "missing DCS open: {:?}",
        &out[..4.min(out.len())]
    );
    assert!(out.ends_with(b"\x1b\\"), "missing DCS close");
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
    assert_eq!(
        opens, 3,
        "expected 3 chunks for ~9k byte input: got {opens}"
    );
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
fn emit_positioned_kitty_image_prefixes_csi_cursor_position_and_dcs() {
    // Non-empty PNG → output begins with CSI CUP (1-based row;col)
    // followed by the exact bytes that emit_kitty_inline_image would
    // produce on its own. Positioned wrapper must not mutate the DCS
    // payload in any way.
    let png = [0xDEu8, 0xAD, 0xBE, 0xEF];
    let positioned = emit_positioned_kitty_image(12, 5, &png);
    let plain = emit_kitty_inline_image(&png);
    // CSI CUP for (col=12, row=5) → ESC [ 6 ; 13 H (1-based add).
    let expected_prefix = b"\x1b[6;13H";
    assert!(
        positioned.starts_with(expected_prefix),
        "positioned emission must start with CSI CUP: got {:?}",
        &positioned[..expected_prefix.len().min(positioned.len())]
    );
    // Remainder after the CSI prefix must match the un-positioned DCS
    // bytes byte-for-byte.
    assert_eq!(
        &positioned[expected_prefix.len()..],
        plain.as_slice(),
        "positioned tail must equal emit_kitty_inline_image output"
    );
}

#[test]
fn emit_positioned_kitty_image_empty_input_emits_nothing() {
    // Empty PNG → no bytes at all (not even the CSI prefix). Prevents
    // cursor-jumps on terminals where the image failed to load.
    let out = emit_positioned_kitty_image(0, 0, &[]);
    assert!(out.is_empty(), "empty PNG must yield empty buffer: {out:?}");
}

#[test]
fn emit_positioned_kitty_image_zero_origin_uses_1_1_csi() {
    // (0,0) is the zero-based cell at the top-left corner. CSI CUP is
    // 1-based so the emitted sequence must be ESC [ 1 ; 1 H.
    let png = [0u8, 1, 2, 3];
    let out = emit_positioned_kitty_image(0, 0, &png);
    assert!(
        out.starts_with(b"\x1b[1;1H"),
        "top-left origin must emit ESC[1;1H: got {:?}",
        &out[..6.min(out.len())]
    );
}

#[test]
fn hash_png_payload_is_stable_and_collision_resistant() {
    // Stable: same bytes → same hash across calls.
    let a = [0x89u8, b'P', b'N', b'G', 0, 1, 2, 3];
    assert_eq!(hash_png_payload(&a), hash_png_payload(&a));
    // Different bytes → different hash (sanity, not guaranteed by
    // contract but overwhelmingly likely for DefaultHasher).
    let b = [0x89u8, b'P', b'N', b'G', 0, 1, 2, 4];
    assert_ne!(
        hash_png_payload(&a),
        hash_png_payload(&b),
        "adjacent-byte payloads collided; dedup guard would misfire"
    );
    // Empty is well-defined and distinct from a one-byte payload.
    assert_ne!(hash_png_payload(&[]), hash_png_payload(&[0]));
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
