use image::ImageFormat;
use log;
use std::path::PathBuf;
use tempfile::Builder;

/// A single pasted attachment (long text or image) tracked in the input ledger.
/// The placeholder token is rendered in the input TextArea so the user sees a
/// compact representation; the full content is expanded at submit time.
#[derive(Debug, Clone)]
pub struct PastedItem {
    pub id: String,
    pub placeholder: String,
    pub kind: PastedKind,
}

#[derive(Debug, Clone)]
pub enum PastedKind {
    /// Long text content to be inlined at submit time.
    Text {
        content: String,
        line_count: usize,
        char_count: usize,
    },
    /// Image attachment. The actual ContentPart rides in `pending_image_parts`;
    /// this variant exists so the tray can show the image alongside text pastes
    /// and an index for removal.
    Image {
        width: u32,
        height: u32,
        byte_count: usize,
    },
}

/// Threshold above which a pasted text becomes a placeholder instead of
/// being inlined directly. Keeps the input readable while preserving the
/// full content for submission.
pub const LONG_PASTE_LINE_THRESHOLD: usize = 8;
pub const LONG_PASTE_CHAR_THRESHOLD: usize = 800;

pub fn is_long_paste(text: &str) -> bool {
    text.chars().count() >= LONG_PASTE_CHAR_THRESHOLD
        || text.chars().filter(|c| *c == '\n').count() + 1 >= LONG_PASTE_LINE_THRESHOLD
}

pub fn make_paste_id(counter: usize) -> String {
    format!("p{}", counter)
}

pub fn text_placeholder(id: &str, char_count: usize, line_count: usize) -> String {
    format!(
        "«pasted:{} +{} chars, {} lines»",
        id, char_count, line_count
    )
}

pub fn image_placeholder(id: &str, width: u32, height: u32) -> String {
    format!("«image:{} {}x{}»", id, width, height)
}

/// Search common directories for an image file matching the given name (with various extensions)
pub fn find_image_file_by_name(name: &str) -> Option<PathBuf> {
    // Common image extensions to try
    const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "tif"];

    // Common directories to search (Desktop, Downloads, Documents, Pictures)
    let common_dirs = [
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(&h).join("Desktop")),
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(&h).join("Downloads")),
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(&h).join("Documents")),
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(&h).join("Pictures")),
        // Also try current directory
        std::env::current_dir().ok(),
    ];

    for dir_opt in common_dirs.iter().flatten() {
        for ext in IMAGE_EXTENSIONS {
            let candidate = dir_opt.join(format!("{}.{}", name, ext));
            if candidate.exists() && candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Errors that can occur while reading or materializing a clipboard image.
#[derive(Debug)]
pub enum PasteImageError {
    ClipboardUnavailable(String),
    NoImage(String),
    EncodeFailed(String),
    IoError(String),
}

impl std::fmt::Display for PasteImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasteImageError::ClipboardUnavailable(msg) => write!(f, "clipboard unavailable: {msg}"),
            PasteImageError::NoImage(msg) => write!(f, "no image on clipboard: {msg}"),
            PasteImageError::EncodeFailed(msg) => write!(f, "could not encode image: {msg}"),
            PasteImageError::IoError(msg) => write!(f, "io error: {msg}"),
        }
    }
}
impl std::error::Error for PasteImageError {}

/// Encoded image format inferred from a file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedImageFormat {
    Jpeg,
}

impl EncodedImageFormat {
    /// Short label used in UI placeholders (e.g., `[image 32x16 JPEG]`).
    pub fn label(self) -> &'static str {
        match self {
            EncodedImageFormat::Jpeg => "JPEG",
        }
    }
}

/// Metadata returned when reading clipboard images.
#[derive(Debug, Clone)]
pub struct PastedImageInfo {
    pub width: u32,
    pub height: u32,
    pub encoded_format: EncodedImageFormat,
}

/// Extract file paths from text that may contain other content.
///
/// This function looks for file paths in text, handling:
/// - Absolute paths (starting with / or ~)
/// - Windows paths (C:\ or \\)
/// - Paths with spaces (even unquoted)
/// - file:// URLs
pub fn extract_file_paths_from_text(text: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let text = text.trim();

    // First, try to normalize the entire text as a single path
    if let Some(path) = normalize_pasted_path(text) {
        paths.push(path);
        return paths;
    }

    // Try to find paths within the text
    // Look for absolute Unix paths (starting with /)
    let image_exts = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "tif"];

    // First, try to find image extensions and work backwards to find the path start
    for ext in &image_exts {
        let ext_pattern = format!(".{}", ext);
        let mut search_start = 0;

        while let Some(ext_pos) = text[search_start..]
            .to_lowercase()
            .find(&ext_pattern.to_lowercase())
        {
            let ext_start = search_start + ext_pos;
            let ext_end = ext_start + ext_pattern.len();

            // Check if extension is followed by whitespace, newline, or end of string
            let is_valid_end = ext_end >= text.len()
                || text
                    .chars()
                    .nth(ext_end)
                    .map(|c| c.is_whitespace() || c == '\n' || c == '\r')
                    .unwrap_or(true);

            if is_valid_end {
                // Work backwards to find where the path starts
                // Look for the last / before the extension, or ~, or beginning of text
                // Note: We continue through spaces because filenames can contain spaces
                let mut path_start = ext_start;
                let mut found_slash = false;

                // Look backwards for / or ~, continuing through spaces (filenames can have spaces)
                while path_start > 0 {
                    let prev_char = text.chars().nth(path_start - 1);
                    if let Some(c) = prev_char {
                        if c == '/' {
                            found_slash = true;
                            break;
                        } else if c == '~' {
                            path_start -= 1;
                            found_slash = true;
                            break;
                        }
                        // Continue through spaces and other characters - don't stop at whitespace
                        // because filenames can contain spaces
                    }
                    path_start -= 1;
                }

                // Only accept paths that start with / or ~, or start at beginning of text
                let is_valid_start = path_start == 0
                    || text
                        .chars()
                        .nth(path_start)
                        .map(|c| c == '/' || c == '~')
                        .unwrap_or(false)
                    || found_slash;

                if is_valid_start {
                    let path_str = text[path_start..ext_end].trim();
                    let path = PathBuf::from(path_str);
                    if path.exists() && path.is_file() {
                        paths.push(path);
                        search_start = ext_end;
                        continue; // Try to find more paths
                    }
                }
            }

            search_start = ext_end;
        }
    }

    // Try to find Windows paths (C:\ or C:/)
    let mut start = 0;
    while start < text.len() {
        // Look for drive letter pattern: [A-Za-z]:[/\\]
        if let Some(colon_pos) = text[start..].find(':') {
            let drive_start = start + colon_pos;
            if drive_start > 0 {
                let before_colon = text.chars().nth(drive_start - 1);
                if let Some(c) = before_colon {
                    if c.is_ascii_alphabetic() && drive_start + 1 < text.len() {
                        let after_colon = &text[drive_start + 1..];
                        if after_colon.starts_with('\\') || after_colon.starts_with('/') {
                            // Found potential Windows path
                            let path_start = drive_start - 1;

                            // Find where path ends - look for image extensions
                            let image_exts =
                                ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "tif"];
                            let mut found_path = false;

                            for ext in &image_exts {
                                let ext_lower = ext.to_lowercase();
                                let text_lower = text[path_start..].to_lowercase();

                                if let Some(ext_pos) = text_lower.find(&format!(".{}", ext_lower)) {
                                    let ext_start = path_start + ext_pos + 1;
                                    let ext_end = ext_start + ext.len();

                                    if ext_end >= text.len()
                                        || text
                                            .chars()
                                            .nth(ext_end)
                                            .map(|c| c.is_whitespace() || c == '\n' || c == '\r')
                                            .unwrap_or(true)
                                    {
                                        let path_str = text[path_start..ext_end].trim();
                                        let path = PathBuf::from(path_str);
                                        if path.exists() && path.is_file() {
                                            paths.push(path);
                                            found_path = true;
                                            start = ext_end;
                                            break;
                                        }
                                    }
                                }
                            }

                            if !found_path {
                                start = drive_start + 1;
                            }
                        } else {
                            start = drive_start + 1;
                        }
                    } else {
                        start = drive_start + 1;
                    }
                } else {
                    start = drive_start + 1;
                }
            } else {
                start = drive_start + 1;
            }
        } else {
            break;
        }
    }

    paths
}

/// Normalize pasted text that may represent a filesystem path.
///
/// Supports:
/// - `file://` URLs (converted to local paths)
/// - Windows/UNC paths
/// - shell‑escaped single paths (via `shlex`)
/// - Unquoted paths with spaces (tries direct path first)
pub fn normalize_pasted_path(pasted: &str) -> Option<PathBuf> {
    let pasted = pasted.trim();

    // file:// URL → filesystem path
    if let Ok(url) = url::Url::parse(pasted)
        && url.scheme() == "file"
    {
        return url.to_file_path().ok();
    }

    // Detect unquoted Windows paths and bypass POSIX shlex which
    // treats backslashes as escapes (e.g., C:\Users\Alice\file.png).
    // Also handles UNC paths (\\server\share\path).
    let looks_like_windows_path = {
        // Drive letter path: C:\ or C:/
        let drive = pasted
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
            && pasted.get(1..2) == Some(":")
            && pasted
                .get(2..3)
                .map(|s| s == "\\" || s == "/")
                .unwrap_or(false);
        // UNC path: \\server\share
        let unc = pasted.starts_with("\\\\");
        drive || unc
    };
    if looks_like_windows_path {
        let path = PathBuf::from(pasted);
        if path.exists() && path.is_file() {
            return Some(path);
        }
    }

    // Try direct path first (handles unquoted paths with spaces)
    if pasted.starts_with('/') || pasted.starts_with('~') {
        let path = PathBuf::from(pasted);
        if path.exists() && path.is_file() {
            return Some(path);
        }
    }

    // shell‑escaped single path → unescaped
    let parts: Vec<String> = shlex::Shlex::new(pasted).collect();
    if parts.len() == 1 {
        let path = PathBuf::from(&parts[0]);
        if path.exists() && path.is_file() {
            return Some(path);
        }
    }

    None
}

/// Encode a DynamicImage to JPEG format.
///
/// Returns the JPEG bytes or an error if encoding fails.
///
/// Note: This is kept here for clipboard image data processing (raw RGBA data),
/// which is different from file-based processing handled by image_upload module.
fn encode_image_to_jpeg(dyn_img: &image::DynamicImage) -> Result<Vec<u8>, PasteImageError> {
    let mut jpeg: Vec<u8> = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut jpeg);
        dyn_img
            .write_to(&mut cursor, ImageFormat::Jpeg)
            .map_err(|e| PasteImageError::EncodeFailed(e.to_string()))?;
    }

    if jpeg.is_empty() {
        return Err(PasteImageError::EncodeFailed(
            "encoded JPEG is empty".into(),
        ));
    }

    Ok(jpeg)
}

/// Capture image from system clipboard, encode to PNG, and return bytes + info.
#[cfg(not(target_os = "android"))]
pub fn paste_image_as_png() -> Result<(Vec<u8>, PastedImageInfo), PasteImageError> {
    // Note: Function name says "png" but we now encode as JPEG for better compression
    log::info!("attempting clipboard image read");
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| PasteImageError::ClipboardUnavailable(e.to_string()))?;

    // On macOS, when copying a file from Finder, the clipboard might contain:
    // 1. Image data (which could be the file icon/preview, not the actual image)
    // 2. File path/reference in clipboard text
    // We should check for file paths FIRST and prefer reading the actual file
    // over clipboard image data, because the clipboard image might just be an icon.

    // First, check if there's a file path in clipboard text (prefer this over clipboard image)
    if let Ok(text) = cb.get_text() {
        let trimmed = text.trim();

        // Try to normalize as a full path first
        let path_opt = normalize_pasted_path(trimmed).and_then(|path_buf| {
            let path = path_buf.as_path();
            // Check if it's an image file and exists
            if path
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| {
                    matches!(
                        ext.to_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif"
                    )
                })
                .unwrap_or(false)
                && path.exists()
                && path.is_file()
            {
                Some(path.to_path_buf())
            } else {
                None
            }
        });

        // If no full path found, try searching for filename in common directories
        // (macOS Finder copies often put just the filename in clipboard)
        let path_opt = if path_opt.is_none() && !trimmed.contains('/') && !trimmed.contains('\\') {
            find_image_file_by_name(trimmed)
        } else {
            path_opt
        };

        if let Some(path) = path_opt {
            log::info!("Found image file path in clipboard: {}", path.display());

            // Read the image file directly without resizing
            match std::fs::read(&path) {
                Ok(data) => {
                    // Get dimensions
                    if let Ok((width, height)) = image::image_dimensions(&path) {
                        log::info!(
                            "clipboard image from file: {}x{}, {} bytes",
                            width,
                            height,
                            data.len()
                        );
                        // Use JPEG as the format
                        let encoded_format = EncodedImageFormat::Jpeg;
                        return Ok((
                            data,
                            PastedImageInfo {
                                width,
                                height,
                                encoded_format,
                            },
                        ));
                    }
                }
                Err(e) => {
                    log::warn!("Failed to read image file {}: {}", path.display(), e);
                    // Fall through to try clipboard image data
                }
            }
        }
    }

    // If no file path found or file processing failed, try clipboard image data
    let img_result = cb.get_image();

    let img = match img_result {
        Ok(img) => img,
        Err(_) => {
            log::info!("No direct image data in clipboard");
            return Err(PasteImageError::NoImage(
                "The clipboard contents were not available in the requested format or the clipboard is empty.".to_string(),
            ));
        }
    };
    let w = img.width as u32;
    let h = img.height as u32;

    // Validate image dimensions
    if w == 0 || h == 0 {
        return Err(PasteImageError::EncodeFailed(
            "invalid image dimensions".into(),
        ));
    }

    // Validate we have enough bytes for the image
    let expected_bytes = (w as usize) * (h as usize) * 4; // RGBA = 4 bytes per pixel
    let actual_bytes = img.bytes.len();
    if actual_bytes < expected_bytes {
        log::warn!(
            "Image buffer size mismatch: expected {} bytes, got {} bytes",
            expected_bytes,
            actual_bytes
        );
        return Err(PasteImageError::EncodeFailed(format!(
            "invalid image buffer size: expected {} bytes, got {} bytes",
            expected_bytes, actual_bytes
        )));
    }

    let Some(rgba_img) = image::RgbaImage::from_raw(w, h, img.bytes.into_owned()) else {
        return Err(PasteImageError::EncodeFailed("invalid RGBA buffer".into()));
    };
    let dyn_img = image::DynamicImage::ImageRgba8(rgba_img);
    log::info!(
        "clipboard image decoded RGBA {w}x{h} ({} bytes)",
        actual_bytes
    );

    // Encode to JPEG for better compression (no resizing)
    let jpeg = encode_image_to_jpeg(&dyn_img)?;

    log::info!(
        "clipboard image encoded to JPEG {w}x{h}, {} bytes",
        jpeg.len()
    );
    Ok((
        jpeg,
        PastedImageInfo {
            width: w,
            height: h,
            encoded_format: EncodedImageFormat::Jpeg,
        },
    ))
}

/// Android/Termux does not support arboard; return a clear error.
#[cfg(target_os = "android")]
pub fn paste_image_as_png() -> Result<(Vec<u8>, PastedImageInfo), PasteImageError> {
    Err(PasteImageError::ClipboardUnavailable(
        "clipboard image paste is unsupported on Android".into(),
    ))
}

/// Convenience: write clipboard image to a temp JPEG file and return its path + info.
#[cfg(not(target_os = "android"))]
pub fn paste_image_to_temp_png() -> Result<(PathBuf, PastedImageInfo), PasteImageError> {
    let (jpeg, info) = paste_image_as_png()?;
    // Create a unique temporary file with a .jpg suffix to avoid collisions.
    let tmp = Builder::new()
        .prefix("stakpak-clipboard-")
        .suffix(".jpg")
        .tempfile()
        .map_err(|e| PasteImageError::IoError(e.to_string()))?;
    std::fs::write(tmp.path(), &jpeg).map_err(|e| PasteImageError::IoError(e.to_string()))?;
    // Persist the file (so it remains after the handle is dropped) and return its PathBuf.
    let (_file, path) = tmp
        .keep()
        .map_err(|e| PasteImageError::IoError(e.error.to_string()))?;
    // Canonicalize the path to ensure it's absolute and resolves any symlinks
    let canonical_path = path
        .canonicalize()
        .map_err(|e| PasteImageError::IoError(format!("Failed to canonicalize path: {}", e)))?;
    log::info!("Clipboard image saved to: {}", canonical_path.display());
    Ok((canonical_path, info))
}

#[cfg(target_os = "android")]
pub fn paste_image_to_temp_png() -> Result<(PathBuf, PastedImageInfo), PasteImageError> {
    Err(PasteImageError::ClipboardUnavailable(
        "clipboard image paste is unsupported on Android".into(),
    ))
}

/// Copy text to the system clipboard.
#[cfg(not(target_os = "android"))]
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {}", e))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Failed to copy to clipboard: {}", e))
}

#[cfg(target_os = "android")]
pub fn copy_to_clipboard(_text: &str) -> Result<(), String> {
    Err("Clipboard is unsupported on Android".to_string())
}

// ===== Unit 5 (Wave 3.1) — Attachment tray preview & reorder helpers =====
//
// These helpers power the paste tray UI: per-paste kind badge, size, token
// estimate, preview, and the cursor/reorder/removal state transitions.
// Kept as pure functions over `PastedItem` / `Vec<PastedItem>` so they are
// trivially unit-testable without building a full `AppState`.

/// Fixed tile cost used to estimate image token budget (Anthropic default).
pub const IMAGE_TOKEN_ESTIMATE: usize = 85;

/// Heuristic: 4 characters ≈ 1 token. Used when the Unit-3 tokenizer is not
/// merged yet so this unit is independently shippable.
pub fn estimate_text_tokens(char_count: usize) -> usize {
    char_count.div_ceil(4)
}

/// Kind badge rendered in the tray card (`[TEXT]` / `[FILE]` / `[IMG]`).
pub fn kind_badge(kind: &PastedKind) -> &'static str {
    match kind {
        PastedKind::Text { .. } => "[TEXT]",
        PastedKind::Image { .. } => "[IMG]",
    }
}

/// Human-friendly size for the kind. `chars/bytes` for text, `WxH, KB` for img.
pub fn size_label(kind: &PastedKind) -> String {
    match kind {
        PastedKind::Text { char_count, .. } => {
            format!("{}c", char_count)
        }
        PastedKind::Image {
            width,
            height,
            byte_count,
        } => format!("{}x{} {}KB", width, height, byte_count / 1024),
    }
}

/// Token estimate label for the tray (`~N tok`).
pub fn token_estimate(kind: &PastedKind) -> usize {
    match kind {
        PastedKind::Text { char_count, .. } => estimate_text_tokens(*char_count),
        PastedKind::Image { .. } => IMAGE_TOKEN_ESTIMATE,
    }
}

/// Short preview: first 80 chars for text; "image WxH" for image.
pub fn preview_text(kind: &PastedKind) -> String {
    match kind {
        PastedKind::Text { content, .. } => {
            let one_line: String = content
                .chars()
                .map(|c| if c == '\n' { ' ' } else { c })
                .collect();
            let trimmed = one_line.trim();
            let preview: String = trimmed.chars().take(80).collect();
            if trimmed.chars().count() > 80 {
                format!("{}…", preview)
            } else {
                preview
            }
        }
        PastedKind::Image { width, height, .. } => format!("image {}x{}", width, height),
    }
}

/// Clamp a selected index into `pending_pastes`. Returns 0 when empty.
pub fn clamp_selected(selected: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        selected.min(len.saturating_sub(1))
    }
}

/// Move cursor to the next paste (wrapping).
pub fn select_next(selected: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (selected + 1) % len }
}

/// Move cursor to the previous paste (wrapping).
pub fn select_prev(selected: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else if selected == 0 {
        len - 1
    } else {
        selected - 1
    }
}

/// Remove the paste at `selected`. Returns the new clamped cursor.
pub fn remove_at(pastes: &mut Vec<PastedItem>, selected: usize) -> usize {
    if pastes.is_empty() || selected >= pastes.len() {
        return 0;
    }
    pastes.remove(selected);
    clamp_selected(selected, pastes.len())
}

/// Swap the paste at `selected` with its successor. Returns the new cursor
/// (tracking the moved paste). No-op if at the last index.
pub fn swap_with_next(pastes: &mut [PastedItem], selected: usize) -> usize {
    if pastes.is_empty() || selected + 1 >= pastes.len() {
        return selected.min(pastes.len().saturating_sub(1));
    }
    pastes.swap(selected, selected + 1);
    selected + 1
}

/// Swap the paste at `selected` with its predecessor. Returns the new cursor.
pub fn swap_with_prev(pastes: &mut [PastedItem], selected: usize) -> usize {
    if pastes.is_empty() || selected == 0 {
        return 0;
    }
    pastes.swap(selected, selected - 1);
    selected - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_item(id: &str, content: &str) -> PastedItem {
        let chars = content.chars().count();
        let lines = content.chars().filter(|c| *c == '\n').count() + 1;
        PastedItem {
            id: id.to_string(),
            placeholder: format!("«{}»", id),
            kind: PastedKind::Text {
                content: content.to_string(),
                line_count: lines,
                char_count: chars,
            },
        }
    }

    fn image_item(id: &str, w: u32, h: u32) -> PastedItem {
        PastedItem {
            id: id.to_string(),
            placeholder: format!("«{}»", id),
            kind: PastedKind::Image {
                width: w,
                height: h,
                byte_count: 4096,
            },
        }
    }

    #[test]
    fn kind_badge_labels() {
        assert_eq!(kind_badge(&text_item("p1", "hi").kind), "[TEXT]");
        assert_eq!(kind_badge(&image_item("p2", 100, 50).kind), "[IMG]");
    }

    #[test]
    fn text_token_heuristic_rounds_up() {
        assert_eq!(estimate_text_tokens(0), 0);
        assert_eq!(estimate_text_tokens(1), 1);
        assert_eq!(estimate_text_tokens(3), 1);
        assert_eq!(estimate_text_tokens(4), 1);
        assert_eq!(estimate_text_tokens(5), 2);
        assert_eq!(estimate_text_tokens(400), 100);
    }

    #[test]
    fn image_token_estimate_is_fixed_tile() {
        assert_eq!(token_estimate(&image_item("p1", 100, 200).kind), 85);
    }

    #[test]
    fn preview_text_flattens_newlines_and_truncates() {
        let item = text_item("p1", "line one\nline two");
        assert_eq!(preview_text(&item.kind), "line one line two");

        let long: String = "x".repeat(200);
        let item = text_item("p1", &long);
        let preview = preview_text(&item.kind);
        // 80 chars + ellipsis
        assert_eq!(preview.chars().count(), 81);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn select_next_prev_wrap() {
        assert_eq!(select_next(0, 3), 1);
        assert_eq!(select_next(2, 3), 0); // wrap
        assert_eq!(select_prev(0, 3), 2); // wrap
        assert_eq!(select_prev(1, 3), 0);
        // empty slice
        assert_eq!(select_next(0, 0), 0);
        assert_eq!(select_prev(0, 0), 0);
    }

    #[test]
    fn remove_selected_shrinks_vec_and_clamps_cursor() {
        // Pre-populate with 3 items
        let mut pastes = vec![
            text_item("p1", "one"),
            text_item("p2", "two"),
            text_item("p3", "three"),
        ];
        // Remove middle
        let new_sel = remove_at(&mut pastes, 1);
        assert_eq!(pastes.len(), 2);
        assert_eq!(pastes[0].id, "p1");
        assert_eq!(pastes[1].id, "p3");
        assert_eq!(new_sel, 1); // still in range
        // Remove last → cursor clamps down to 0 (only 1 item left)
        let new_sel = remove_at(&mut pastes, 1);
        assert_eq!(pastes.len(), 1);
        assert_eq!(new_sel, 0);
        // Remove the last one → empty + cursor 0
        let new_sel = remove_at(&mut pastes, 0);
        assert!(pastes.is_empty());
        assert_eq!(new_sel, 0);
    }

    #[test]
    fn remove_at_out_of_range_is_noop() {
        let mut pastes = vec![text_item("p1", "one")];
        let new_sel = remove_at(&mut pastes, 5);
        assert_eq!(pastes.len(), 1);
        assert_eq!(new_sel, 0);
    }

    #[test]
    fn reorder_j_twice_matches_expected_order() {
        // Pre-populate with 3 items
        let mut pastes = vec![
            text_item("p1", "one"),
            text_item("p2", "two"),
            text_item("p3", "three"),
        ];
        // Simulate selected=0, press "J" twice (swap with next)
        let sel = swap_with_next(&mut pastes, 0); // [p2, p1, p3], cursor=1
        assert_eq!(
            pastes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["p2", "p1", "p3"]
        );
        assert_eq!(sel, 1);
        let sel = swap_with_next(&mut pastes, sel); // [p2, p3, p1], cursor=2
        assert_eq!(
            pastes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["p2", "p3", "p1"]
        );
        assert_eq!(sel, 2);
    }

    #[test]
    fn reorder_k_moves_up() {
        let mut pastes = vec![
            text_item("p1", "one"),
            text_item("p2", "two"),
            text_item("p3", "three"),
        ];
        let sel = swap_with_prev(&mut pastes, 2); // [p1, p3, p2], cursor=1
        assert_eq!(
            pastes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["p1", "p3", "p2"]
        );
        assert_eq!(sel, 1);
    }

    #[test]
    fn swap_noops_at_boundaries() {
        let mut pastes = vec![text_item("p1", "one"), text_item("p2", "two")];
        // Can't go up from 0
        let sel = swap_with_prev(&mut pastes, 0);
        assert_eq!(sel, 0);
        assert_eq!(pastes[0].id, "p1");
        // Can't go down from last
        let sel = swap_with_next(&mut pastes, 1);
        assert_eq!(sel, 1);
        assert_eq!(pastes[1].id, "p2");
    }

    #[test]
    fn size_label_text_vs_image() {
        let t = text_item("p1", "hello");
        assert_eq!(size_label(&t.kind), "5c");
        let i = image_item("p2", 200, 100);
        // 4096 / 1024 = 4
        assert_eq!(size_label(&i.kind), "200x100 4KB");
    }
}
