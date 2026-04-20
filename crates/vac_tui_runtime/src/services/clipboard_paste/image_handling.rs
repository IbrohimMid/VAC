use image::ImageFormat;
use log;
use std::path::PathBuf;
use tempfile::Builder;

use super::path_extraction::normalize_pasted_path;
use super::path_extraction::find_image_file_by_name;

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
