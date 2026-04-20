use std::path::PathBuf;

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

/// Extract file paths from text that may contain other content.
///
/// This function looks for file paths in text, handling:
/// - Absolute paths (starting with / or ~)
/// - Windows paths (C:\\ or \\\\)
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

    // Try to find Windows paths (C:\\ or C:/)
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
    // treats backslashes as escapes (e.g., C:\\Users\\Alice\\file.png).
    // Also handles UNC paths (\\\\server\\share\\path).
    let looks_like_windows_path = {
        // Drive letter path: C:\\ or C:/
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
        // UNC path: \\\\server\\share
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
