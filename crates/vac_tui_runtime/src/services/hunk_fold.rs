//! F7.8 — Diff hunk-fold polish.
//!
//! When an inline diff contains long runs of unchanged context, the
//! viewer collapses the middle into a single "… N lines hidden …"
//! marker. Keeps `keep_head` lines before and `keep_tail` after a
//! change block so the surrounding context is still readable.
//!
//! Deliberately string-first — the viewer is the caller's concern,
//! this module is pure logic + unit tests.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

impl DiffLine {
    pub fn is_context(&self) -> bool {
        matches!(self, Self::Context(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoldedLine {
    /// Passthrough.
    Line(DiffLine),
    /// Collapsed context range summary.
    Elision { hidden_lines: usize },
}

/// Fold parameters.
#[derive(Debug, Clone, Copy)]
pub struct FoldConfig {
    /// Minimum consecutive unchanged context lines before we consider
    /// folding. A run shorter than `threshold` is always kept.
    pub threshold: usize,
    /// Lines to keep at the start of a folded run.
    pub keep_head: usize,
    /// Lines to keep at the end of a folded run.
    pub keep_tail: usize,
}

impl Default for FoldConfig {
    fn default() -> Self {
        Self {
            threshold: 6,
            keep_head: 2,
            keep_tail: 2,
        }
    }
}

/// Fold long context runs in `lines`. Returns a new vector preserving
/// changed lines verbatim, with long context runs replaced by
/// head + elision + tail.
pub fn fold_context(lines: &[DiffLine], cfg: FoldConfig) -> Vec<FoldedLine> {
    let mut out: Vec<FoldedLine> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].is_context() {
            out.push(FoldedLine::Line(lines[i].clone()));
            i += 1;
            continue;
        }
        // Measure this context run.
        let mut j = i;
        while j < lines.len() && lines[j].is_context() {
            j += 1;
        }
        let run_len = j - i;
        let max_kept = cfg.keep_head + cfg.keep_tail;
        if run_len < cfg.threshold || run_len <= max_kept {
            for k in i..j {
                out.push(FoldedLine::Line(lines[k].clone()));
            }
        } else {
            for k in i..i + cfg.keep_head {
                out.push(FoldedLine::Line(lines[k].clone()));
            }
            let hidden = run_len - cfg.keep_head - cfg.keep_tail;
            out.push(FoldedLine::Elision {
                hidden_lines: hidden,
            });
            for k in j - cfg.keep_tail..j {
                out.push(FoldedLine::Line(lines[k].clone()));
            }
        }
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(n: usize) -> Vec<DiffLine> {
        (0..n).map(|i| DiffLine::Context(format!("c{i}"))).collect()
    }

    #[test]
    fn short_context_run_is_not_folded() {
        let lines = ctx(4);
        let out = fold_context(&lines, FoldConfig::default());
        assert_eq!(out.len(), 4);
        assert!(out.iter().all(|l| matches!(l, FoldedLine::Line(_))));
    }

    #[test]
    fn long_run_collapses_with_head_and_tail() {
        let mut lines = Vec::new();
        lines.push(DiffLine::Added("+ plus".into()));
        lines.extend(ctx(20));
        lines.push(DiffLine::Removed("- minus".into()));
        let out = fold_context(
            &lines,
            FoldConfig {
                threshold: 6,
                keep_head: 2,
                keep_tail: 2,
            },
        );
        // Added + 2 head + elision + 2 tail + Removed = 7.
        assert_eq!(out.len(), 7);
        // The elision reports 20 - 2 - 2 = 16 hidden lines.
        let elided = out.iter().find_map(|l| match l {
            FoldedLine::Elision { hidden_lines } => Some(*hidden_lines),
            _ => None,
        });
        assert_eq!(elided, Some(16));
    }

    #[test]
    fn run_equal_to_threshold_is_preserved_when_head_plus_tail_covers_it() {
        let cfg = FoldConfig {
            threshold: 4,
            keep_head: 2,
            keep_tail: 2,
        };
        let out = fold_context(&ctx(4), cfg);
        // run_len (4) <= keep_head + keep_tail (4) → preserved.
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn interleaved_changes_only_fold_between_them() {
        let mut lines = Vec::new();
        lines.push(DiffLine::Added("+1".into()));
        lines.extend(ctx(10));
        lines.push(DiffLine::Added("+2".into()));
        lines.extend(ctx(3));
        lines.push(DiffLine::Removed("-3".into()));
        let out = fold_context(&lines, FoldConfig::default());
        // Only the 10-run folds (it exceeds threshold=6 and head+tail=4).
        let elisions: usize = out
            .iter()
            .filter(|l| matches!(l, FoldedLine::Elision { .. }))
            .count();
        assert_eq!(elisions, 1);
    }
}
