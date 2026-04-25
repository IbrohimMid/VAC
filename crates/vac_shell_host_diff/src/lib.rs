//! Slice 15 — host-side diff projection.
//!
//! Tiny line-level differ — not a full Myers/LCS implementation,
//! but produces a sensible single-hunk `DiffFileView` good enough
//! for the review widget. Hosts that want a real diff should swap
//! this out for `similar`/`difference`/etc. — the `DiffFileView`
//! shape is the boundary.

use vac_shell_contracts::{DiffFileView, DiffHunkView, DiffLineKind, DiffLineView};

/// Build a `DiffFileView` from `before` and `after` snapshots of a
/// file. Lines that match by index land as `Context`; the trailing
/// extras land as `Added` or `Removed` accordingly.
pub fn project_file_diff(path: impl Into<String>, before: &str, after: &str) -> DiffFileView {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    let mut lines: Vec<DiffLineView> = Vec::new();
    let common = before_lines.len().min(after_lines.len());
    let mut added = 0usize;
    let mut removed = 0usize;

    for i in 0..common {
        if before_lines[i] == after_lines[i] {
            lines.push(DiffLineView {
                kind: DiffLineKind::Context,
                text: before_lines[i].to_string(),
            });
        } else {
            lines.push(DiffLineView {
                kind: DiffLineKind::Removed,
                text: before_lines[i].to_string(),
            });
            removed += 1;
            lines.push(DiffLineView {
                kind: DiffLineKind::Added,
                text: after_lines[i].to_string(),
            });
            added += 1;
        }
    }
    for line in before_lines.iter().skip(common) {
        lines.push(DiffLineView {
            kind: DiffLineKind::Removed,
            text: line.to_string(),
        });
        removed += 1;
    }
    for line in after_lines.iter().skip(common) {
        lines.push(DiffLineView {
            kind: DiffLineKind::Added,
            text: line.to_string(),
        });
        added += 1;
    }

    let header = format!(
        "@@ -1,{} +1,{} @@",
        before_lines.len().max(1),
        after_lines.len().max(1)
    );

    DiffFileView {
        path: path.into(),
        added,
        removed,
        hunks: vec![DiffHunkView { header, lines }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_no_change_yields_zero_added_removed() {
        let f = project_file_diff("a.rs", "fn foo() {}\n", "fn foo() {}\n");
        assert_eq!(f.added, 0);
        assert_eq!(f.removed, 0);
    }

    #[test]
    fn project_pure_addition_counts_as_added_only() {
        let f = project_file_diff("a.rs", "a\n", "a\nb\nc\n");
        assert_eq!(f.added, 2);
        assert_eq!(f.removed, 0);
        let kinds: Vec<DiffLineKind> = f.hunks[0].lines.iter().map(|l| l.kind).collect();
        assert!(kinds.contains(&DiffLineKind::Context));
        assert!(kinds.contains(&DiffLineKind::Added));
    }

    #[test]
    fn project_replacement_counts_both_sides() {
        let f = project_file_diff("a.rs", "old\n", "new\n");
        assert_eq!(f.added, 1);
        assert_eq!(f.removed, 1);
    }
}
