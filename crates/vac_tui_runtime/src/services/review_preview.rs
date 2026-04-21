//! Image preview data prep for the review tab (PR-T17 / R8b).
//!
//! Responsibilities:
//!   1. Detect whether a selected review path is an image by extension.
//!   2. Read the image bytes from disk with a hard size cap so an
//!      accidentally-staged 500 MiB asset never blocks the TUI.
//!   3. Decode just enough to learn the pixel dimensions so the render
//!      layer can size the ASCII frame correctly (and, once R8c wires
//!      the post-frame DCS emission hook, so the Kitty path knows how
//!      many cells the image should occupy).
//!
//! This module is intentionally *pure*: it does not touch terminal
//! state or ratatui. The renderer in `workbench/review.rs` consumes the
//! returned [`ImagePreview`] and either draws the ASCII frame inline
//! (today) or hands the bytes to
//! [`crate::services::kitty_image::emit_kitty_inline_image`] once the
//! post-frame emission hook lands.

use std::path::Path;

/// Maximum on-disk image size we will materialise into memory for the
/// preview pane. Files bigger than this surface as
/// [`ImagePreviewError::TooLarge`] so the caller can show a
/// "file too large to preview" notice instead of reading the whole
/// thing. 10 MiB comfortably covers UI screenshots / logos while
/// rejecting accidentally-staged binary blobs.
pub const IMAGE_PREVIEW_MAX_BYTES: usize = 10 * 1024 * 1024;

/// Extensions we treat as "previewable image". We keep this deliberately
/// narrow: it drives both (a) renderer routing in the review tab and
/// (b) which paths avoid the text-diff loader that would otherwise fail
/// on non-UTF-8 bytes. Adding TIFF/SVG should be a separate, explicit
/// change so the routing table stays auditable.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// A prepared image ready for rendering. Owns its bytes so downstream
/// callers can hand them to the Kitty emitter without re-reading disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePreview {
    /// Raw file bytes, unmodified. For PNG this is the valid PNG stream
    /// that `emit_kitty_inline_image` expects under `f=100,t=d`.
    pub bytes: Vec<u8>,
    /// Intrinsic pixel width reported by the decoder. `0` when the
    /// decoder could not determine dimensions but bytes are otherwise
    /// valid for the Kitty protocol (rare).
    pub width: u32,
    /// Intrinsic pixel height.
    pub height: u32,
    /// Lower-cased extension that triggered routing (e.g. `"png"`).
    pub extension: String,
}

/// Failure modes for [`prepare_image_preview`]. Every variant carries
/// enough context that the UI can render a one-line reason instead of
/// an opaque "error".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImagePreviewError {
    /// Path has no extension, or the extension is not in
    /// [`IMAGE_EXTENSIONS`]. Callers should not route non-image paths
    /// through the preview service; this variant exists as a defensive
    /// check for misuse.
    NotAnImage,
    /// File system read failed (not found, permission denied, etc.).
    /// The message is the `std::io::Error` display output.
    Io(String),
    /// File exceeds the configured byte cap.
    TooLarge { bytes: usize, cap: usize },
    /// `image` crate could not determine dimensions from the bytes
    /// (corrupt header, truncated stream, unsupported subformat).
    Decode(String),
}

impl std::fmt::Display for ImagePreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnImage => write!(f, "not an image path"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::TooLarge { bytes, cap } => {
                write!(f, "image too large: {bytes} bytes > cap {cap}")
            }
            Self::Decode(e) => write!(f, "decode error: {e}"),
        }
    }
}

impl std::error::Error for ImagePreviewError {}

/// Return `true` when `path` ends in a known image extension. Matching
/// is case-insensitive and ignores query strings / fragments (callers
/// should pass filesystem paths, not URLs, but we defensively strip
/// anything past the first `?` or `#`).
pub fn is_image_path(path: &str) -> bool {
    extension_of(path)
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or(false)
}

fn extension_of(path: &str) -> Option<String> {
    // Defensively strip URL-style tails so `foo.png?v=1` still matches.
    let cleaned = path
        .split(['?', '#'])
        .next()
        .unwrap_or(path);
    Path::new(cleaned)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

/// Read `path` from disk, enforce the size cap, and decode its intrinsic
/// dimensions. Returns the bytes + metadata as an [`ImagePreview`].
///
/// This is synchronous I/O by design: the review tab already reads
/// diffs via `std::fs::read_to_string` on the render path, so adding
/// another blocking read here is consistent with existing scheduling.
/// When the caller is on a tokio reactor and needs to avoid stalling
/// the runtime, wrap the call in `tokio::task::spawn_blocking`.
pub fn prepare_image_preview(
    path: &Path,
    max_bytes: usize,
) -> Result<ImagePreview, ImagePreviewError> {
    let path_str = path.to_string_lossy();
    let ext = extension_of(&path_str).ok_or(ImagePreviewError::NotAnImage)?;
    if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        return Err(ImagePreviewError::NotAnImage);
    }

    // Fast size check via metadata before pulling bytes into memory.
    match std::fs::metadata(path) {
        Ok(meta) => {
            let len = meta.len() as usize;
            if len > max_bytes {
                return Err(ImagePreviewError::TooLarge {
                    bytes: len,
                    cap: max_bytes,
                });
            }
        }
        Err(e) => return Err(ImagePreviewError::Io(e.to_string())),
    }

    let bytes = std::fs::read(path).map_err(|e| ImagePreviewError::Io(e.to_string()))?;
    if bytes.len() > max_bytes {
        // Race: file grew between metadata and read. Reject conservatively.
        return Err(ImagePreviewError::TooLarge {
            bytes: bytes.len(),
            cap: max_bytes,
        });
    }

    // Dimension probing via the `image` crate. We use
    // `image_dimensions` rather than a full decode so a 4K PNG does not
    // cost a full RGBA buffer just to draw a placeholder.
    let (width, height) = match image::image_dimensions(path) {
        Ok((w, h)) => (w, h),
        Err(e) => return Err(ImagePreviewError::Decode(e.to_string())),
    };

    Ok(ImagePreview {
        bytes,
        width,
        height,
        extension: ext,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal 1x1 red PNG, hand-assembled so tests do not depend on
    /// the `image` crate's encoder ever producing a specific byte
    /// sequence.
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk length + type
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, // bit depth/color/filter/CRC
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, // IDAT
        0x08, 0x99, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, // compressed RGB(255,0,0)
        0x00, 0x03, 0x00, 0x01,
        0x5B, 0x6E, 0x2A, 0xA8, // IDAT CRC
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND
        0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn is_image_path_detects_common_extensions() {
        // Positive cases cover the entire IMAGE_EXTENSIONS table plus
        // case-insensitivity and URL-style suffixes.
        assert!(is_image_path("logo.png"));
        assert!(is_image_path("photo.JPG"));
        assert!(is_image_path("animation.gif"));
        assert!(is_image_path("wallpaper.webp"));
        assert!(is_image_path("bitmap.BMP"));
        assert!(is_image_path("portrait.JpEg"));
        assert!(is_image_path("assets/logo.png?v=hash42"));
        assert!(is_image_path("/abs/path/to/hero.png"));

        // Negative cases: text files, extensionless files, look-alike tails.
        assert!(!is_image_path("src/main.rs"));
        assert!(!is_image_path("Cargo.toml"));
        assert!(!is_image_path("README"));
        assert!(!is_image_path("notes.png.bak"));
        assert!(!is_image_path(""));
    }

    #[test]
    fn prepare_image_preview_reads_valid_png_and_reports_dimensions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tiny.png");
        std::fs::File::create(&path)
            .and_then(|mut f| f.write_all(TINY_PNG))
            .expect("write tiny.png");

        let preview = prepare_image_preview(&path, IMAGE_PREVIEW_MAX_BYTES)
            .expect("valid PNG must decode");
        assert_eq!(preview.width, 1, "tiny.png is 1x1");
        assert_eq!(preview.height, 1);
        assert_eq!(preview.extension, "png");
        assert_eq!(preview.bytes, TINY_PNG, "bytes must round-trip untouched");
    }

    #[test]
    fn prepare_image_preview_rejects_non_image_extension() {
        // An existing .rs file is not previewable even if its bytes
        // happen to decode as something else. The extension gate must
        // short-circuit before any disk I/O that could surface a
        // non-deterministic error.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("hello.rs");
        std::fs::File::create(&path)
            .and_then(|mut f| f.write_all(b"fn main() {}"))
            .expect("write hello.rs");
        let err = prepare_image_preview(&path, IMAGE_PREVIEW_MAX_BYTES)
            .expect_err(".rs must be rejected");
        assert_eq!(err, ImagePreviewError::NotAnImage);
    }

    #[test]
    fn prepare_image_preview_enforces_size_cap() {
        // File is labelled .png but is 2x larger than the cap we pass
        // in. We don't care about the body being a real PNG here
        // because the metadata gate should reject before decode.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("huge.png");
        let body = vec![0u8; 2048];
        std::fs::File::create(&path)
            .and_then(|mut f| f.write_all(&body))
            .expect("write huge.png");
        let err = prepare_image_preview(&path, 1024)
            .expect_err("oversize must error");
        match err {
            ImagePreviewError::TooLarge { bytes, cap } => {
                assert_eq!(bytes, 2048);
                assert_eq!(cap, 1024);
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn prepare_image_preview_reports_decode_error_for_corrupt_png() {
        // .png extension routes us past the gate, but the body is
        // deliberately not a valid PNG. `image::image_dimensions` must
        // surface a Decode error rather than panic.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("broken.png");
        std::fs::File::create(&path)
            .and_then(|mut f| f.write_all(b"not really a png"))
            .expect("write broken.png");
        let err = prepare_image_preview(&path, IMAGE_PREVIEW_MAX_BYTES)
            .expect_err("corrupt PNG must error");
        assert!(
            matches!(err, ImagePreviewError::Decode(_)),
            "expected Decode error, got {err:?}"
        );
    }

    #[test]
    fn prepare_image_preview_reports_io_error_for_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.png");
        let err = prepare_image_preview(&path, IMAGE_PREVIEW_MAX_BYTES)
            .expect_err("missing file must error");
        assert!(
            matches!(err, ImagePreviewError::Io(_)),
            "expected Io error, got {err:?}"
        );
    }
}
