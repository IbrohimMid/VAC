//! File Diff Rendering
//!
//! Minimal implementation for showing file diffs in TUI

use std::path::Path;

use crate::services::diagnostics_overlay::{
    render_line_with_diagnostics, squiggly_spans_for_line,
};
use crate::services::theme::{StyleKey, Theme};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use vac_core::lsp::types::LspWorkspaceSnapshot;

/// Render a simple diff between old and new content
pub fn render_diff(theme: &Theme, old_content: &str, new_content: &str, max_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![Span::styled(
        "--- Old",
        theme.style(StyleKey::DiffRemoved).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "+++ New",
        theme.style(StyleKey::DiffAdded).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Simple line-by-line comparison using `similar`
    let diff = similar::TextDiff::from_lines(old_content, new_content);

    for change in diff.iter_all_changes() {
        let line = change.value();
        let truncated = truncate_line(line.trim_end_matches('\n'), max_width.saturating_sub(2));

        match change.tag() {
            similar::ChangeTag::Delete => {
                lines.push(Line::from(vec![
                    Span::styled("- ", theme.style(StyleKey::DiffRemoved)),
                    Span::styled(truncated, theme.style(StyleKey::DiffRemoved)),
                ]));
            }
            similar::ChangeTag::Insert => {
                let mut span = Span::styled(truncated.clone(), theme.style(StyleKey::DiffAdded));
                // Highlight VIL macros (generated code hint)
                if truncated.trim().starts_with("#[vil_") {
                    span = Span::styled(
                        format!("{} (VIL-generated plumbing)", truncated),
                        theme.style(StyleKey::DiffAdded).add_modifier(Modifier::BOLD),
                    );
                }
                lines.push(Line::from(vec![
                    Span::styled("+ ", theme.style(StyleKey::DiffAdded)),
                    span,
                ]));
            }
            similar::ChangeTag::Equal => {
                lines.push(Line::from(vec![Span::raw("  "), Span::raw(truncated)]));
            }
        }
    }

    lines
}

/// PR-T15 P1 — same as [`render_diff`] but overlays inline LSP diagnostics on
/// Insert/Equal rows (the "new" side). Delete rows are left untouched because
/// they refer to lines that no longer exist in the new file. Line numbers are
/// tracked 0-based against the new file to match `LspRange::start_line`.
pub fn render_diff_with_diagnostics(
    theme: &Theme,
    old_content: &str,
    new_content: &str,
    max_width: usize,
    snapshot: Option<&LspWorkspaceSnapshot>,
    file_path: Option<&Path>,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(vec![Span::styled(
        "--- Old",
        theme.style(StyleKey::DiffRemoved).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "+++ New",
        theme.style(StyleKey::DiffAdded).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    let diff = similar::TextDiff::from_lines(old_content, new_content);
    // 0-based line counter for the new file; increments on Insert + Equal.
    let mut new_line_no: u32 = 0;

    for change in diff.iter_all_changes() {
        let raw = change.value();
        let truncated = truncate_line(raw.trim_end_matches('\n'), max_width.saturating_sub(2));

        match change.tag() {
            similar::ChangeTag::Delete => {
                lines.push(Line::from(vec![
                    Span::styled("- ", theme.style(StyleKey::DiffRemoved)),
                    Span::styled(truncated, theme.style(StyleKey::DiffRemoved)),
                ]));
            }
            similar::ChangeTag::Insert => {
                let base = theme.style(StyleKey::DiffAdded);
                let overlay = diagnostics_overlay_for(snapshot, file_path, new_line_no, &truncated);
                let mut row: Vec<Span<'static>> =
                    vec![Span::styled("+ ", theme.style(StyleKey::DiffAdded))];
                if overlay.is_empty() && !truncated.trim().starts_with("#[vil_") {
                    row.push(Span::styled(truncated.clone(), base));
                } else if truncated.trim().starts_with("#[vil_") {
                    // Preserve the VIL-generated plumbing marker.
                    row.push(Span::styled(
                        format!("{} (VIL-generated plumbing)", truncated),
                        base.add_modifier(Modifier::BOLD),
                    ));
                } else {
                    let overlayed = render_line_with_diagnostics(&truncated, &overlay, base);
                    for s in overlayed.spans.into_iter() {
                        row.push(Span::styled(s.content.into_owned(), s.style));
                    }
                }
                lines.push(Line::from(row));
                new_line_no = new_line_no.saturating_add(1);
            }
            similar::ChangeTag::Equal => {
                let overlay = diagnostics_overlay_for(snapshot, file_path, new_line_no, &truncated);
                if overlay.is_empty() {
                    lines.push(Line::from(vec![Span::raw("  "), Span::raw(truncated)]));
                } else {
                    let overlayed =
                        render_line_with_diagnostics(&truncated, &overlay, Style::default());
                    let mut row: Vec<Span<'static>> = vec![Span::raw("  ")];
                    for s in overlayed.spans.into_iter() {
                        row.push(Span::styled(s.content.into_owned(), s.style));
                    }
                    lines.push(Line::from(row));
                }
                new_line_no = new_line_no.saturating_add(1);
            }
        }
    }

    lines
}

fn diagnostics_overlay_for(
    snapshot: Option<&LspWorkspaceSnapshot>,
    file_path: Option<&Path>,
    line_index: u32,
    rendered: &str,
) -> Vec<crate::services::diagnostics_overlay::DiagnosticSpan> {
    match (snapshot, file_path) {
        (Some(snap), Some(path)) => {
            let width = rendered.chars().count() as u32;
            squiggly_spans_for_line(snap, path, line_index, width)
        }
        _ => Vec::new(),
    }
}

fn truncate_line(line: &str, max_width: usize) -> String {
    if line.len() <= max_width {
        line.to_string()
    } else {
        format!("{}...", &line[..max_width.saturating_sub(3)])
    }
}

/// Preview diff for a file operation
pub fn preview_file_diff(
    theme: &Theme,
    file_path: &str,
    old_content: &str,
    new_content: &str,
    max_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // File header - clone to make 'static
    lines.push(Line::from(vec![
        Span::styled("File: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(file_path.to_string(), theme.style(StyleKey::Accent)),
    ]));
    lines.push(Line::from(""));

    // Diff content
    lines.extend(render_diff(theme, old_content, new_content, max_width));

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_core::lsp::types::{LspDiagnostic, LspRange, LspSeverity, LspWorkspaceSnapshot};

    fn snapshot_with(diags: Vec<LspDiagnostic>) -> LspWorkspaceSnapshot {
        let mut snap = LspWorkspaceSnapshot {
            diagnostics: diags,
            total_errors: 0,
            total_warnings: 0,
        };
        snap.rebuild_counts();
        snap
    }

    fn err_at(path: &str, sl: u32, sc: u32, el: u32, ec: u32) -> LspDiagnostic {
        LspDiagnostic {
            file_path: std::path::PathBuf::from(path),
            severity: LspSeverity::Error,
            code: None,
            source: Some("vil_validate".to_string()),
            message: "boom".to_string(),
            range: LspRange {
                start_line: sl,
                start_character: sc,
                end_line: el,
                end_character: ec,
            },
        }
    }

    #[test]
    fn render_diff_with_diagnostics_overlays_insert_rows() {
        // One-line change: new file gets a single inserted line with an
        // error span at cols 4..8 on line 0.
        let theme = Theme::default();
        let old = "";
        let new = "let x = broken_call;\n";
        let snap = snapshot_with(vec![err_at("foo.rs", 0, 4, 0, 8)]);
        let lines = render_diff_with_diagnostics(
            &theme,
            old,
            new,
            80,
            Some(&snap),
            Some(Path::new("foo.rs")),
        );
        // Header rows (---, +++, blank) then one Insert row.
        assert!(lines.len() >= 4, "expected header + insert row");
        let insert_row = lines.last().expect("insert row");
        let has_underlined_run = insert_row.spans.iter().any(|s| {
            s.style
                .add_modifier
                .contains(ratatui::style::Modifier::UNDERLINED)
        });
        assert!(
            has_underlined_run,
            "new-side insert row must carry an underlined diagnostic span"
        );
    }

    #[test]
    fn render_diff_with_diagnostics_falls_back_without_snapshot() {
        let theme = Theme::default();
        let old = "a\nb\n";
        let new = "a\nc\n";
        // No snapshot — behavior should match plain render_diff shape-wise.
        let with_overlay = render_diff_with_diagnostics(&theme, old, new, 80, None, None);
        let plain = render_diff(&theme, old, new, 80);
        assert_eq!(
            with_overlay.len(),
            plain.len(),
            "line count must match plain render when snapshot is absent"
        );
    }

    #[test]
    fn render_diff_with_diagnostics_ignores_other_files() {
        let theme = Theme::default();
        let old = "";
        let new = "let x = broken_call;\n";
        let snap = snapshot_with(vec![err_at("other.rs", 0, 4, 0, 8)]);
        let lines = render_diff_with_diagnostics(
            &theme,
            old,
            new,
            80,
            Some(&snap),
            Some(Path::new("foo.rs")),
        );
        let insert_row = lines.last().expect("insert row");
        let has_underlined_run = insert_row.spans.iter().any(|s| {
            s.style
                .add_modifier
                .contains(ratatui::style::Modifier::UNDERLINED)
        });
        assert!(
            !has_underlined_run,
            "diagnostic from other.rs must not leak into foo.rs overlay"
        );
    }
}
