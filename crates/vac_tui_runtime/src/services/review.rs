use ratatui::text::Line;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
        reverse_apply_hunk(current_content, hunk)
    }

    pub fn revert_hunk_to_disk(&self, index: usize, file_path: &Path) -> Result<(), String> {
        let current_content = std::fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {e}", file_path.display()))?;
        let reverted = self.revert_hunk(index, &current_content)?;
        std::fs::write(file_path, reverted)
            .map_err(|e| format!("Failed to write {}: {e}", file_path.display()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffOp<'a> {
    Equal(usize, usize, &'a str),
    Insert(usize, usize, &'a str),
    Remove(usize, usize, &'a str),
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

pub fn parse_unified_diff(diff_text: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current: Option<DiffHunk> = None;

    for line in diff_text.lines() {
        if line.starts_with("@@") {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }

            let rest = line
                .trim_start_matches("@@")
                .trim()
                .trim_end_matches("@@")
                .trim();
            let Some((old_start, old_lines, new_start, new_lines)) = parse_hunk_header(rest) else {
                continue;
            };
            current = Some(DiffHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
                staged: false,
            });
            continue;
        }

        if let Some(hunk) = current.as_mut() {
            if line == r"\ No newline at end of file" {
                continue;
            }
            if line.starts_with("--- ")
                || line.starts_with("+++ ")
                || line.starts_with("diff --git")
            {
                continue;
            }
            if matches!(line.chars().next(), Some(' ' | '+' | '-')) {
                hunk.lines.push(line.to_string());
            }
        }
    }

    if let Some(hunk) = current {
        hunks.push(hunk);
    }

    hunks
}

pub fn parse_hunks_from_contents(old: &str, new: &str) -> Vec<DiffHunk> {
    let (old_lines, _) = split_lines(old);
    let (new_lines, _) = split_lines(new);
    let ops = lcs_diff(&old_lines, &new_lines);

    if ops.iter().all(|op| matches!(op, DiffOp::Equal(..))) {
        return Vec::new();
    }

    const CONTEXT: usize = 3;

    let change_indices: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter_map(|(idx, op)| (!matches!(op, DiffOp::Equal(..))).then_some(idx))
        .collect();

    if change_indices.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut group_start = change_indices[0];
    let mut group_end = change_indices[0];

    for &idx in change_indices.iter().skip(1) {
        let equal_gap = idx.saturating_sub(group_end + 1);
        if equal_gap <= CONTEXT * 2 {
            group_end = idx;
        } else {
            ranges.push((group_start, group_end));
            group_start = idx;
            group_end = idx;
        }
    }
    ranges.push((group_start, group_end));

    ranges
        .into_iter()
        .map(|(change_start, change_end)| {
            let start = change_start.saturating_sub(CONTEXT);
            let end = (change_end + CONTEXT + 1).min(ops.len());
            let slice = &ops[start..end];

            let (old_start, new_start) = match slice.first().copied() {
                Some(DiffOp::Equal(old, new, _))
                | Some(DiffOp::Insert(old, new, _))
                | Some(DiffOp::Remove(old, new, _)) => (old, new),
                None => (1, 1),
            };

            let mut old_count = 0usize;
            let mut new_count = 0usize;
            let mut lines = Vec::new();

            for op in slice {
                match op {
                    DiffOp::Equal(_, _, line) => {
                        old_count += 1;
                        new_count += 1;
                        lines.push(format!(" {line}"));
                    }
                    DiffOp::Insert(_, _, line) => {
                        new_count += 1;
                        lines.push(format!("+{line}"));
                    }
                    DiffOp::Remove(_, _, line) => {
                        old_count += 1;
                        lines.push(format!("-{line}"));
                    }
                }
            }

            DiffHunk {
                old_start,
                old_lines: old_count,
                new_start,
                new_lines: new_count,
                lines,
                staged: false,
            }
        })
        .collect()
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
    let patch = hunk_to_patch(file_path, hunk);
    apply_patch_to_git(project_root, &patch, true)
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
    let patch = hunk_to_patch(file_path, &inverted);
    apply_patch_to_git(project_root, &patch, true)
}

fn reverse_apply_hunk(content: &str, hunk: &DiffHunk) -> Result<String, String> {
    let (content_lines, had_trailing_newline) = split_lines(content);
    let mut working: Vec<String> = content_lines.into_iter().map(str::to_string).collect();

    let needle: Vec<&str> = hunk
        .lines
        .iter()
        .filter_map(|line| line.strip_prefix(' ').or_else(|| line.strip_prefix('+')))
        .collect();
    let replacement: Vec<String> = hunk
        .lines
        .iter()
        .filter_map(|line| {
            line.strip_prefix(' ')
                .or_else(|| line.strip_prefix('-'))
                .map(ToOwned::to_owned)
        })
        .collect();

    if needle.is_empty() {
        let insert_at = hunk.new_start.saturating_sub(1).min(working.len());
        working.splice(insert_at..insert_at, replacement);
        return Ok(join_lines(&working, had_trailing_newline));
    }

    let haystack: Vec<&str> = working.iter().map(String::as_str).collect();
    let hint = hunk.new_start.saturating_sub(1);
    let Some(start) = find_window(&haystack, &needle, hint) else {
        return Err(format!(
            "Could not locate hunk near line {} while reverting",
            hunk.new_start
        ));
    };

    let end = start + needle.len();
    working.splice(start..end, replacement);
    Ok(join_lines(&working, had_trailing_newline))
}

fn find_window(haystack: &[&str], needle: &[&str], hint: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let max_start = haystack.len() - needle.len();
    let hint = hint.min(max_start);
    for offset in 0..=max_start {
        for &start in &[hint.saturating_sub(offset), hint + offset] {
            if start > max_start {
                continue;
            }
            if haystack[start..start + needle.len()] == needle[..] {
                return Some(start);
            }
        }
    }
    None
}

fn lcs_diff<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<DiffOp<'a>> {
    let n = old.len();
    let m = new.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if old[i] == new[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            ops.push(DiffOp::Equal(i + 1, j + 1, old[i]));
            i += 1;
            j += 1;
        } else if j < m && (i >= n || dp[i][j + 1] >= dp[i + 1][j]) {
            ops.push(DiffOp::Insert(i + 1, j + 1, new[j]));
            j += 1;
        } else {
            ops.push(DiffOp::Remove(i + 1, j + 1, old[i]));
            i += 1;
        }
    }
    ops
}

fn hunk_to_patch(file_path: &str, hunk: &DiffHunk) -> String {
    let mut patch = String::new();
    patch.push_str(&format!("diff --git a/{0} b/{0}\n", file_path));
    patch.push_str(&format!("--- a/{file_path}\n"));
    patch.push_str(&format!("+++ b/{file_path}\n"));
    patch.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
    ));
    for line in &hunk.lines {
        patch.push_str(line);
        patch.push('\n');
    }
    patch
}

fn apply_patch_to_git(project_root: &Path, patch: &str, cached: bool) -> Result<(), String> {
    let mut command = Command::new("git");
    command
        .current_dir(project_root)
        .arg("apply")
        .arg("--unidiff-zero")
        .arg("--whitespace=nowarn");
    if cached {
        command.arg("--cached");
    }
    command.arg("-");
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn git apply: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(patch.as_bytes())
            .map_err(|e| format!("Failed to write patch to git apply: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for git apply: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "git apply failed".to_string()
        })
    }
}

fn parse_hunk_header(rest: &str) -> Option<(usize, usize, usize, usize)> {
    let mut parts = rest.split_whitespace();
    let old = parts.next()?;
    let new = parts.next()?;
    let (old_start, old_lines) = parse_range(old)?;
    let (new_start, new_lines) = parse_range(new)?;
    Some((old_start, old_lines, new_start, new_lines))
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    let s = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    if let Some((start, len)) = s.split_once(',') {
        Some((start.parse().ok()?, len.parse().ok()?))
    } else {
        Some((s.parse().ok()?, 1))
    }
}

fn split_lines(content: &str) -> (Vec<&str>, bool) {
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<&str> = content.split('\n').collect();
    if had_trailing_newline {
        let _ = lines.pop();
    }
    (lines, had_trailing_newline)
}

fn join_lines(lines: &[String], had_trailing_newline: bool) -> String {
    let mut joined = lines.join("\n");
    if had_trailing_newline && !lines.is_empty() {
        joined.push('\n');
    }
    joined
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
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
