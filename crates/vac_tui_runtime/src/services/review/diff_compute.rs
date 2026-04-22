use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use super::DiffHunk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffOp<'a> {
    Equal(usize, usize, &'a str),
    Insert(usize, usize, &'a str),
    Remove(usize, usize, &'a str),
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

pub(super) fn reverse_apply_hunk(content: &str, hunk: &DiffHunk) -> Result<String, String> {
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

pub(super) fn hunk_to_patch(file_path: &str, hunk: &DiffHunk) -> String {
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

pub(super) fn apply_patch_to_git(
    project_root: &Path,
    patch: &str,
    cached: bool,
) -> Result<(), String> {
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

pub(super) fn split_lines(content: &str) -> (Vec<&str>, bool) {
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<&str> = content.split('\n').collect();
    if had_trailing_newline {
        let _ = lines.pop();
    }
    (lines, had_trailing_newline)
}

pub(super) fn join_lines(lines: &[String], had_trailing_newline: bool) -> String {
    let mut joined = lines.join("\n");
    if had_trailing_newline && !lines.is_empty() {
        joined.push('\n');
    }
    joined
}
