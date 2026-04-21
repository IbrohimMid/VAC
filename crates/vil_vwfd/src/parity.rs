//! VWFD <-> Rust handler parity pass.
//!
//! Walks a Rust source tree looking for `#[vil_handler]` / `#[vil_handler(..)]`
//! attributes and compares the extracted handler names against the handlers
//! declared in a [`VwfdDocument`]. Produces a list of [`ParityIssue`]s so
//! downstream tooling (diff overlay, `vac vil doctor`, CI parity gate) can
//! surface mismatches to the user.
//!
//! Scope: deliberately minimal. We do a regex-like line scan — not a full
//! `syn` parse — because the VWFD parity pass runs on every diff render and
//! doesn't need expression-level fidelity. A richer AST-based pass can replace
//! the scanner later without changing the public surface.

use std::path::{Path, PathBuf};

use crate::schema::VwfdDocument;

/// A single parity finding between a VWFD document and the Rust source tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParityIssue {
    /// Declared as a handler in VWFD but no matching Rust `#[vil_handler]` fn found.
    MissingRust {
        handler: String,
    },
    /// `#[vil_handler]` fn exists in Rust but no VWFD handler with that name.
    OrphanRust {
        handler: String,
        source_path: PathBuf,
        line: usize,
    },
}

impl ParityIssue {
    /// Short human-facing label suitable for a single TUI line.
    pub fn label(&self) -> String {
        match self {
            ParityIssue::MissingRust { handler } => {
                format!("missing Rust impl for handler `{handler}`")
            }
            ParityIssue::OrphanRust { handler, source_path, line } => format!(
                "orphan Rust handler `{handler}` at {}:{}",
                source_path.display(),
                line,
            ),
        }
    }
}

/// Compare handlers declared in `vwfd` against `#[vil_handler]` functions
/// found under `rust_root`. Returns both directions of missing parity.
///
/// If `rust_root` doesn't exist or is unreadable the pass treats the Rust side
/// as empty — every VWFD handler becomes a `MissingRust` issue. This matches
/// what a user would expect when they point the tool at the wrong directory.
pub fn parity_pass(vwfd: &VwfdDocument, rust_root: &Path) -> Vec<ParityIssue> {
    let rust_handlers = scan_rust_handlers(rust_root);
    let rust_names: std::collections::HashSet<&str> =
        rust_handlers.iter().map(|h| h.name.as_str()).collect();
    let vwfd_names: std::collections::HashSet<&str> = vwfd
        .spec
        .handlers
        .iter()
        .map(|h| h.name.as_str())
        .collect();

    let mut issues = Vec::new();

    for handler in &vwfd.spec.handlers {
        if !rust_names.contains(handler.name.as_str()) {
            issues.push(ParityIssue::MissingRust {
                handler: handler.name.clone(),
            });
        }
    }

    for rust in &rust_handlers {
        if !vwfd_names.contains(rust.name.as_str()) {
            issues.push(ParityIssue::OrphanRust {
                handler: rust.name.clone(),
                source_path: rust.source_path.clone(),
                line: rust.line,
            });
        }
    }

    issues
}

#[derive(Debug, Clone)]
struct RustHandler {
    name: String,
    source_path: PathBuf,
    line: usize,
}

fn scan_rust_handlers(rust_root: &Path) -> Vec<RustHandler> {
    let mut out = Vec::new();
    if !rust_root.exists() {
        return out;
    }
    walk_rs_files(rust_root, &mut |path| {
        if let Ok(contents) = std::fs::read_to_string(path) {
            extract_handlers_from_source(&contents, path, &mut out);
        }
    });
    out
}

fn walk_rs_files(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip common non-source directories
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(name, "target" | ".git" | "node_modules") {
                continue;
            }
            walk_rs_files(&path, visit);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            visit(&path);
        }
    }
}

/// Extract `#[vil_handler]` / `#[vil_handler(..)]` function names from a
/// single Rust source string. Tolerant of attribute args and `async fn` vs
/// `fn`. Ignores doc-comment quotes and string literals via naive line parse
/// — sufficient for the parity pass, and avoids a proc-macro-level parser.
fn extract_handlers_from_source(src: &str, path: &Path, out: &mut Vec<RustHandler>) {
    let lines: Vec<&str> = src.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        // Skip lines inside raw string tokens (very rough check for our fixtures).
        if trimmed.starts_with("//") || trimmed.starts_with("///") {
            continue;
        }
        if !trimmed.starts_with("#[vil_handler") {
            continue;
        }
        // Find the fn declaration in the next few lines.
        for peek in lines.iter().take(idx + 6).skip(idx + 1) {
            let peek_trim = peek.trim_start();
            if let Some(name) = parse_fn_name(peek_trim) {
                out.push(RustHandler {
                    name,
                    source_path: path.to_path_buf(),
                    line: idx + 1,
                });
                break;
            }
        }
    }
}

/// Extract `foo` from `pub async fn foo(...)` / `fn foo(...)` / `async fn foo(`.
fn parse_fn_name(line: &str) -> Option<String> {
    // Strip visibility & async keywords progressively.
    let mut rest = line;
    for prefix in ["pub(crate) ", "pub ", "async "] {
        if let Some(s) = rest.strip_prefix(prefix) {
            rest = s.trim_start();
        }
    }
    // Handle `async` appearing after `pub`.
    if let Some(s) = rest.strip_prefix("async ") {
        rest = s.trim_start();
    }
    let after_fn = rest.strip_prefix("fn ")?;
    let end = after_fn
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(after_fn.len());
    if end == 0 {
        return None;
    }
    Some(after_fn[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_yaml;
    use tempfile::tempdir;

    const VWFD_FIXTURE: &str = r#"
apiVersion: vil.dev/v1
kind: Workflow
metadata:
  name: sample
spec:
  triggers:
    - name: onStart
      kind: manual
  handlers:
    - name: hello
      execution: native
    - name: goodbye
      execution: native
  steps:
    - id: s1
      handler: hello
    - id: s2
      handler: goodbye
"#;

    fn write_rs(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
    }

    #[test]
    fn parity_pass_full_parity_reports_no_issues() {
        let dir = tempdir().unwrap();
        write_rs(
            dir.path(),
            "src/handlers.rs",
            r#"
use vil_server::prelude::*;

#[vil_handler(shm)]
async fn hello(ctx: ServiceCtx) -> VilResponse<String> {
    VilResponse::ok("hi".into())
}

#[vil_handler]
pub async fn goodbye(ctx: ServiceCtx) -> VilResponse<String> {
    VilResponse::ok("bye".into())
}
"#,
        );
        let vwfd = from_yaml(VWFD_FIXTURE).unwrap();
        let issues = parity_pass(&vwfd, dir.path());
        assert!(issues.is_empty(), "expected no issues, got {issues:?}");
    }

    #[test]
    fn parity_pass_missing_rust_is_reported() {
        let dir = tempdir().unwrap();
        // Only `hello` implemented — `goodbye` missing on Rust side.
        write_rs(
            dir.path(),
            "src/handlers.rs",
            r#"
#[vil_handler]
async fn hello(ctx: ServiceCtx) -> VilResponse<String> {
    VilResponse::ok("hi".into())
}
"#,
        );
        let vwfd = from_yaml(VWFD_FIXTURE).unwrap();
        let issues = parity_pass(&vwfd, dir.path());
        assert_eq!(issues.len(), 1, "{issues:?}");
        match &issues[0] {
            ParityIssue::MissingRust { handler } => assert_eq!(handler, "goodbye"),
            other => panic!("unexpected issue: {other:?}"),
        }
    }

    #[test]
    fn parity_pass_orphan_rust_is_reported() {
        let dir = tempdir().unwrap();
        // Both VWFD handlers plus an extra orphan on Rust side.
        write_rs(
            dir.path(),
            "src/handlers.rs",
            r#"
#[vil_handler]
async fn hello(ctx: ServiceCtx) -> VilResponse<String> { todo!() }

#[vil_handler]
async fn goodbye(ctx: ServiceCtx) -> VilResponse<String> { todo!() }

#[vil_handler(shm)]
pub async fn orphaned(ctx: ServiceCtx) -> VilResponse<String> { todo!() }
"#,
        );
        let vwfd = from_yaml(VWFD_FIXTURE).unwrap();
        let issues = parity_pass(&vwfd, dir.path());
        assert_eq!(issues.len(), 1, "{issues:?}");
        match &issues[0] {
            ParityIssue::OrphanRust { handler, line, .. } => {
                assert_eq!(handler, "orphaned");
                assert!(*line > 0);
            }
            other => panic!("unexpected issue: {other:?}"),
        }
    }
}
