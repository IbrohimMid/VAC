pub mod diff_compute;
pub use diff_compute::*;

use ratatui::text::Line;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DiffData {
    pub path: String,
    pub old_content: String,
    pub new_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<String>,
    pub staged: bool,
}

impl DiffHunk {
    pub fn removed_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|line| line.strip_prefix('-'))
            .collect()
    }

    pub fn added_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|line| line.strip_prefix('+'))
            .collect()
    }

    pub fn context_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|line| line.strip_prefix(' '))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct PatchModel {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}

impl PatchModel {
    /// Build from in-process diff (no git needed)
    pub fn from_diff(path: &str, old_content: &str, new_content: &str) -> Self {
        Self {
            path: path.to_string(),
            hunks: parse_hunks_from_contents(old_content, new_content),
        }
    }

    pub fn stage_hunk(&mut self, index: usize) {
        if let Some(hunk) = self.hunks.get_mut(index) {
            hunk.staged = true;
        }
    }

    pub fn unstage_hunk(&mut self, index: usize) {
        if let Some(hunk) = self.hunks.get_mut(index) {
            hunk.staged = false;
        }
    }

    /// Returns Err jika hunk tidak bisa dilokasikan di content
    pub fn revert_hunk(&self, index: usize, current_content: &str) -> Result<String, String> {
        let hunk = self
            .hunks
            .get(index)
            .ok_or_else(|| format!("Hunk index out of range: {index}"))?;
        diff_compute::reverse_apply_hunk(current_content, hunk)
    }

    pub fn revert_hunk_to_disk(&self, index: usize, file_path: &Path) -> Result<(), String> {
        let current_content = std::fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {e}", file_path.display()))?;
        let reverted = self.revert_hunk(index, &current_content)?;
        std::fs::write(file_path, reverted)
            .map_err(|e| format!("Failed to write {}: {e}", file_path.display()))
    }
}

pub fn snapshot_path(project_root: &Path, session_id: Uuid, file_path: &str) -> PathBuf {
    let safe_name = format!("{}.bak", file_path.replace(['/', '\\'], "__"));
    project_root
        .join(".vac/backups")
        .join(session_id.to_string())
        .join(safe_name)
}

pub fn load_diff(
    project_root: &Path,
    session_id: Uuid,
    file_path: &str,
) -> Result<DiffData, String> {
    // R8d: image files have a dedicated preview pane in the review tab
    // (see workbench/review.rs + services/review_preview.rs). Attempting to
    // `read_to_string` a binary image would always fail with an "invalid
    // UTF-8" error that the UI would surface as a spurious diff failure.
    // Return an empty DiffData so the image-preview branch can take over
    // without any stale last_error leaking into ReviewDiffState.
    if crate::services::review_preview::is_image_path(file_path) {
        return Ok(DiffData {
            path: file_path.to_string(),
            old_content: String::new(),
            new_content: String::new(),
        });
    }

    let abs_path = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        project_root.join(file_path)
    };

    let snap_path = snapshot_path(project_root, session_id, file_path);

    let old_content = if snap_path.exists() {
        std::fs::read_to_string(&snap_path)
            .map_err(|e| format!("Failed to read snapshot {}: {e}", snap_path.display()))?
    } else {
        String::new()
    };

    let new_content = if abs_path.exists() {
        std::fs::read_to_string(&abs_path)
            .map_err(|e| format!("Failed to read file {}: {e}", abs_path.display()))?
    } else {
        String::new()
    };

    Ok(DiffData {
        path: file_path.to_string(),
        old_content,
        new_content,
    })
}

pub fn detect_editor(preferred: Option<String>) -> Option<String> {
    if let Some(editor) = preferred
        && is_editor_available(&editor)
    {
        return Some(editor);
    }

    for editor in ["nvim", "vim", "nano", "vi"] {
        if is_editor_available(editor) {
            return Some(editor.to_string());
        }
    }

    None
}

fn is_editor_available(editor: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    Command::new(finder)
        .arg(editor)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn render_diff_viewport(
    old_content: &str,
    new_content: &str,
    max_width: usize,
    scroll: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let theme = crate::services::theme::Theme::default();
    let lines = crate::services::file_diff::render_diff(&theme, old_content, new_content, max_width);
    if height == 0 {
        return vec![];
    }
    lines.into_iter().skip(scroll).take(height).collect()
}

/// PR-T15 P1 — viewport wrapper that overlays inline LSP diagnostics on the
/// new-side rows. Falls back to plain diff rendering when either the snapshot
/// or file path is `None`, preserving behavior for call sites without LSP.
pub fn render_diff_viewport_with_diagnostics(
    old_content: &str,
    new_content: &str,
    max_width: usize,
    scroll: usize,
    height: usize,
    snapshot: Option<&vac_core::lsp::types::LspWorkspaceSnapshot>,
    file_path: Option<&std::path::Path>,
) -> Vec<Line<'static>> {
    let theme = crate::services::theme::Theme::default();
    let lines = crate::services::file_diff::render_diff_with_diagnostics(
        &theme,
        old_content,
        new_content,
        max_width,
        snapshot,
        file_path,
    );
    if height == 0 {
        return vec![];
    }
    lines.into_iter().skip(scroll).take(height).collect()
}

pub fn git_diff_file(project_root: &Path, file_path: &str) -> Option<String> {
    let output = Command::new("git")
        .current_dir(project_root)
        .args(["diff", "--", file_path])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (!stdout.trim().is_empty()).then_some(stdout)
}

pub fn git_stage_hunk(project_root: &Path, file_path: &str, hunk: &DiffHunk) -> Result<(), String> {
    let patch = diff_compute::hunk_to_patch(file_path, hunk);
    diff_compute::apply_patch_to_git(project_root, &patch, true)
}

pub fn git_unstage_hunk(
    project_root: &Path,
    file_path: &str,
    hunk: &DiffHunk,
) -> Result<(), String> {
    let inverted = DiffHunk {
        old_start: hunk.new_start,
        old_lines: hunk.new_lines,
        new_start: hunk.old_start,
        new_lines: hunk.old_lines,
        lines: hunk
            .lines
            .iter()
            .map(|line| {
                if let Some(rest) = line.strip_prefix('+') {
                    format!("-{rest}")
                } else if let Some(rest) = line.strip_prefix('-') {
                    format!("+{rest}")
                } else {
                    line.clone()
                }
            })
            .collect(),
        staged: false,
    };
    let patch = diff_compute::hunk_to_patch(file_path, &inverted);
    diff_compute::apply_patch_to_git(project_root, &patch, true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;
    use tempfile::tempdir;

    fn git(dir: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "VAC Test")
            .env("GIT_AUTHOR_EMAIL", "vac-test@example.com")
            .env("GIT_COMMITTER_NAME", "VAC Test")
            .env("GIT_COMMITTER_EMAIL", "vac-test@example.com")
            .args(args)
            .output()
            .unwrap()
    }

    #[test]
    fn load_diff_falls_back_to_empty_when_snapshot_missing() {
        let dir = tempfile::tempdir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        std::fs::write(dir.path().join("a.txt"), "new").unwrap();
        let diff = load_diff(dir.path(), session_id, "a.txt").unwrap();
        assert_eq!(diff.old_content, "");
        assert_eq!(diff.new_content, "new");
    }

    #[test]
    fn load_diff_short_circuits_for_image_paths_without_reading_disk() {
        // R8d: is_image_path short-circuits before any read_to_string.
        // Point the path at a file that does NOT exist on disk — if the
        // short-circuit regresses, read_to_string would return Err and
        // this test would fail with a different panic.
        let dir = tempfile::tempdir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let diff = load_diff(dir.path(), session_id, "assets/logo.png")
            .expect("image path must short-circuit to empty DiffData");
        assert_eq!(diff.path, "assets/logo.png");
        assert_eq!(diff.old_content, "");
        assert_eq!(diff.new_content, "");
        // Also covers uppercase + URL-style suffix variants because
        // is_image_path already has its own exhaustive unit tests; this
        // test only pins the integration point in load_diff.
    }

    #[test]
    fn load_diff_reads_snapshot_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(dir.path().join(".vac/backups").join(session_id.to_string()))
            .unwrap();
        std::fs::write(snapshot_path(dir.path(), session_id, "a.txt"), "old").unwrap();
        std::fs::write(dir.path().join("a.txt"), "new").unwrap();
        let diff = load_diff(dir.path(), session_id, "a.txt").unwrap();
        assert_eq!(diff.old_content, "old");
        assert_eq!(diff.new_content, "new");
    }

    #[test]
    fn render_diff_viewport_applies_scroll() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nX\nd\ne\n";
        let lines0 = render_diff_viewport(old, new, 120, 0, 3);
        let lines1 = render_diff_viewport(old, new, 120, 1, 3);
        assert_ne!(format!("{:?}", lines0), format!("{:?}", lines1));
    }

    #[test]
    fn parse_unified_diff_extracts_hunk_header() {
        let diff = "--- a/example.rs\n+++ b/example.rs\n@@ -2,3 +2,4 @@\n line1\n-line2\n+line2_changed\n+line2_5\n line3\n";
        let hunks = parse_unified_diff(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 2);
        assert_eq!(hunks[0].old_lines, 3);
        assert_eq!(hunks[0].new_start, 2);
        assert_eq!(hunks[0].new_lines, 4);
    }

    #[test]
    fn reverse_apply_hunk_reverts_single_line_change() {
        let old = "line1\nline2\nold_line\nline4\n";
        let new = "line1\nline2\nnew_line\nline4\n";
        let patch = PatchModel::from_diff("sample.txt", old, new);
        let reverted = patch.revert_hunk(0, new).unwrap();
        assert_eq!(reverted, old);
    }

    #[test]
    fn reverse_apply_hunk_on_added_lines() {
        let old = "a\nb\nc\n";
        let new = "a\nb\nINSERTED\nc\n";
        let patch = PatchModel::from_diff("sample.txt", old, new);
        let reverted = patch.revert_hunk(0, new).unwrap();
        assert_eq!(reverted, old);
    }

    #[test]
    fn patch_model_from_diff_roundtrip() {
        let old = "fn foo() {}\nfn bar() {}\n";
        let new = "fn foo() {}\nfn bar() { /* changed */ }\n";
        let patch = PatchModel::from_diff("sample.rs", old, new);
        assert_eq!(patch.hunks.len(), 1);
        let reverted = patch.revert_hunk(0, new).unwrap();
        assert_eq!(reverted, old);
    }

    #[test]
    fn git_stage_and_unstage_hunk_against_real_repo() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        assert!(git(root, &["init"]).status.success());

        let file_path = root.join("sample.txt");
        let old = "a\nb\nc\n";
        let new = "a\nb\nCHANGED\nc\n";
        std::fs::write(&file_path, old).unwrap();

        assert!(git(root, &["add", "."]).status.success());
        assert!(git(root, &["commit", "-m", "initial"]).status.success());

        std::fs::write(&file_path, new).unwrap();

        let hunks = parse_hunks_from_contents(old, new);
        assert_eq!(hunks.len(), 1);

        git_stage_hunk(root, "sample.txt", &hunks[0]).unwrap();
        let staged = git(root, &["diff", "--cached", "--", "sample.txt"]);
        assert!(staged.status.success());
        let staged_text = String::from_utf8_lossy(&staged.stdout);
        assert!(staged_text.contains("+CHANGED"));

        git_unstage_hunk(root, "sample.txt", &hunks[0]).unwrap();
        let unstaged = git(root, &["diff", "--cached", "--", "sample.txt"]);
        assert!(unstaged.status.success());
        let unstaged_text = String::from_utf8_lossy(&unstaged.stdout);
        assert!(unstaged_text.trim().is_empty());
    }
}
