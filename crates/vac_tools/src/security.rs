//! Security utilities for tool input validation.

use crate::error::ToolError;
use std::path::{Path, PathBuf};

/// Validate that a file path is within the project root (no path traversal).
/// Canonicalizes both the working directory and the target path to handle symlinks.
pub fn validate_path_within_root(working_dir: &Path, file: &str) -> Result<PathBuf, ToolError> {
    let canonical_root = working_dir
        .canonicalize()
        .unwrap_or_else(|_| working_dir.to_path_buf());
    let abs_path = canonical_root.join(file);
    let canonical = abs_path
        .canonicalize()
        .map_err(|_| ToolError::ExecutionFailed(format!("File not found: {}", file)))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ToolError::ExecutionFailed(
            "Path traversal denied: file must be within project root".into(),
        ));
    }
    Ok(canonical)
}
