//! Inline diagnostics overlay (PR-T15).
//!
//! Converts a workspace-wide [`LspWorkspaceSnapshot`] (populated by
//! `vil_validate` / vil-lsp and surfaced via `InputEvent::LspDiagnostics`)
//! into per-line span ranges the renderer can underline in place.
//!
//! This module intentionally stays a *pure* helper:
//!   - `squiggly_spans_for_line` extracts character ranges for one line.
//!   - `render_line_with_diagnostics` produces a [`ratatui::text::Line`]
//!     with the corresponding styled spans.
//!   - [`DiagnosticsOverlayCache`] tracks which file is currently being
//!     rendered so callers can drop stale per-file state when the active
//!     file changes (typical for Review pane navigation).
//!
//! Consumers (Review / vil-workbench editor renderers) call these helpers
//! per visible line; integration is intentionally not wired yet to keep the
//! surface area of PR-T15 small.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use vac_core::lsp::types::{LspSeverity, LspWorkspaceSnapshot};

/// A styled run of characters within a single rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticSpan {
    /// 0-based character index within the line where the run starts.
    pub start: u32,
    /// Exclusive end character index within the line.
    pub end: u32,
    pub severity: LspSeverity,
}

/// Extract the per-line diagnostic spans for `file_path` at `line_index`
/// (0-based). Multi-line diagnostics are clipped to the requested line. The
/// returned spans are sorted by start column so the renderer can apply them
/// left-to-right without additional work.
pub fn squiggly_spans_for_line(
    snapshot: &LspWorkspaceSnapshot,
    file_path: &Path,
    line_index: u32,
    line_width: u32,
) -> Vec<DiagnosticSpan> {
    let mut out: Vec<DiagnosticSpan> = Vec::new();
    for diag in &snapshot.diagnostics {
        if diag.file_path != file_path {
            continue;
        }
        let r = &diag.range;
        if line_index < r.start_line || line_index > r.end_line {
            continue;
        }
        let start = if line_index == r.start_line {
            r.start_character
        } else {
            0
        };
        let end_raw = if line_index == r.end_line {
            r.end_character
        } else {
            line_width
        };
        // Clamp to the visible line width; ensure at least a 1-char squiggle
        // so zero-length LSP ranges still render something.
        let end = end_raw.min(line_width).max(start.saturating_add(1));
        if start >= line_width {
            continue;
        }
        out.push(DiagnosticSpan {
            start,
            end: end.min(line_width),
            severity: diag.severity.clone(),
        });
    }
    out.sort_by_key(|s| (s.start, s.end));
    out
}

/// Render `line` with the given diagnostic spans overlaid. Regions inside a
/// span get an underlined red/yellow style depending on severity; everything
/// else is rendered with the caller-provided `base` style. The returned line
/// has one [`Span`] per contiguous style run.
pub fn render_line_with_diagnostics<'a>(
    line: &'a str,
    spans: &[DiagnosticSpan],
    base: Style,
) -> Line<'a> {
    if spans.is_empty() {
        return Line::from(Span::styled(line, base));
    }
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let total = chars.len() as u32;
    let mut out: Vec<Span<'a>> = Vec::new();
    let mut cursor: u32 = 0;
    for span in spans {
        let start = span.start.min(total);
        let end = span.end.min(total).max(start);
        if cursor < start {
            let byte_lo = chars[cursor as usize].0;
            let byte_hi = chars[start as usize].0;
            out.push(Span::styled(&line[byte_lo..byte_hi], base));
        }
        if end > start {
            let byte_lo = chars[start as usize].0;
            let byte_hi = if end as usize >= chars.len() {
                line.len()
            } else {
                chars[end as usize].0
            };
            out.push(Span::styled(
                &line[byte_lo..byte_hi],
                style_for_severity(&span.severity),
            ));
        }
        cursor = cursor.max(end);
    }
    if cursor < total {
        let byte_lo = chars[cursor as usize].0;
        out.push(Span::styled(&line[byte_lo..], base));
    }
    Line::from(out)
}

/// Minimal fallback styling used when the caller cannot supply a theme
/// lookup. Dark-theme-ish colors; callers with access to `state.theme`
/// should prefer `theme.style(StyleKey::ValidationError)` etc.
fn style_for_severity(sev: &LspSeverity) -> Style {
    match sev {
        LspSeverity::Error => Style::default()
            .fg(ratatui::style::Color::Red)
            .add_modifier(Modifier::UNDERLINED),
        LspSeverity::Warning => Style::default()
            .fg(ratatui::style::Color::Yellow)
            .add_modifier(Modifier::UNDERLINED),
        LspSeverity::Information | LspSeverity::Hint => Style::default()
            .fg(ratatui::style::Color::Blue)
            .add_modifier(Modifier::UNDERLINED),
    }
}

/// Per-file overlay cache. Callers pass the currently-focused file each
/// render; when it changes, [`clear_if_file_changed`](Self::clear_if_file_changed)
/// drops any per-line caches so stale squiggles from the previous file
/// cannot leak into the new buffer.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticsOverlayCache {
    active_file: Option<PathBuf>,
    // Per-line span cache keyed by 0-based line index. Kept separate from
    // LspWorkspaceSnapshot so render passes can memoize without mutating
    // global state.
    per_line: HashMap<u32, Vec<DiagnosticSpan>>,
}

impl DiagnosticsOverlayCache {
    pub fn active_file(&self) -> Option<&Path> {
        self.active_file.as_deref()
    }

    pub fn cached_line(&self, line: u32) -> Option<&Vec<DiagnosticSpan>> {
        self.per_line.get(&line)
    }

    pub fn set_line(&mut self, line: u32, spans: Vec<DiagnosticSpan>) {
        self.per_line.insert(line, spans);
    }

    /// Returns `true` when `new_file` differs from the previously-tracked
    /// file. Side effect: drops the per-line cache and records the new
    /// path. Passing `None` clears the cache and returns `true` iff a file
    /// was previously tracked.
    pub fn clear_if_file_changed(&mut self, new_file: Option<&Path>) -> bool {
        let same = match (&self.active_file, new_file) {
            (Some(old), Some(new)) => old.as_path() == new,
            (None, None) => true,
            _ => false,
        };
        if same {
            return false;
        }
        self.per_line.clear();
        self.active_file = new_file.map(PathBuf::from);
        true
    }

    pub fn len(&self) -> usize {
        self.per_line.len()
    }

    pub fn is_empty(&self) -> bool {
        self.per_line.is_empty()
    }
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
            file_path: PathBuf::from(path),
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
    fn diagnostics_underline_renders_squiggly() {
        // Snapshot with a single error on line 0, cols 4..8 of `foo.rs`.
        let snap = snapshot_with(vec![err_at("foo.rs", 0, 4, 0, 8)]);
        let spans = squiggly_spans_for_line(&snap, Path::new("foo.rs"), 0, 40);
        assert_eq!(
            spans,
            vec![DiagnosticSpan {
                start: 4,
                end: 8,
                severity: LspSeverity::Error
            }]
        );

        let rendered = render_line_with_diagnostics("let x = broken_call;", &spans, Style::default());
        // Expect three spans: prefix (base), underlined run, suffix (base).
        assert_eq!(rendered.spans.len(), 3);
        assert_eq!(rendered.spans[0].content, "let ");
        assert_eq!(rendered.spans[1].content, "x = ");
        assert!(
            rendered.spans[1]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED),
            "underlined modifier must be set on the error range"
        );
        assert_eq!(
            rendered.spans[1].style.fg,
            Some(ratatui::style::Color::Red),
            "error severity must render red"
        );
        assert_eq!(rendered.spans[2].content, "broken_call;");
    }

    #[test]
    fn diagnostics_clear_on_file_change() {
        let mut cache = DiagnosticsOverlayCache::default();
        assert!(cache.clear_if_file_changed(Some(Path::new("a.rs"))));
        cache.set_line(
            0,
            vec![DiagnosticSpan {
                start: 0,
                end: 3,
                severity: LspSeverity::Error,
            }],
        );
        assert_eq!(cache.len(), 1);
        // Same file: no reset.
        assert!(!cache.clear_if_file_changed(Some(Path::new("a.rs"))));
        assert_eq!(cache.len(), 1);
        // Different file: cache drops.
        assert!(cache.clear_if_file_changed(Some(Path::new("b.rs"))));
        assert!(cache.is_empty());
        assert_eq!(cache.active_file(), Some(Path::new("b.rs")));
        // Returning to None clears and reports change.
        assert!(cache.clear_if_file_changed(None));
        assert!(cache.active_file().is_none());
    }

    #[test]
    fn diagnostics_filter_other_files() {
        let snap = snapshot_with(vec![
            err_at("foo.rs", 0, 4, 0, 8),
            err_at("bar.rs", 0, 0, 0, 3),
        ]);
        let spans = squiggly_spans_for_line(&snap, Path::new("foo.rs"), 0, 40);
        assert_eq!(spans.len(), 1, "bar.rs diagnostic must not leak into foo.rs");
    }

    #[test]
    fn diagnostics_multi_line_clipped_to_requested_line() {
        // Error spans lines 2..=4, cols 5..3.
        let snap = snapshot_with(vec![err_at("foo.rs", 2, 5, 4, 3)]);
        // Middle line: entire width should underline up to line_width.
        let mid = squiggly_spans_for_line(&snap, Path::new("foo.rs"), 3, 20);
        assert_eq!(mid.len(), 1);
        assert_eq!(mid[0].start, 0);
        assert_eq!(mid[0].end, 20);
        // Last line: stops at end_character.
        let last = squiggly_spans_for_line(&snap, Path::new("foo.rs"), 4, 20);
        assert_eq!(last[0].start, 0);
        assert_eq!(last[0].end, 3);
        // Outside the range entirely.
        let out = squiggly_spans_for_line(&snap, Path::new("foo.rs"), 5, 20);
        assert!(out.is_empty());
    }

    #[test]
    fn render_with_no_spans_returns_base_only() {
        let rendered = render_line_with_diagnostics("plain", &[], Style::default());
        assert_eq!(rendered.spans.len(), 1);
        assert_eq!(rendered.spans[0].content, "plain");
    }
}
