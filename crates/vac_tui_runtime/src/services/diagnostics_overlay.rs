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

use crate::services::theme::{StyleKey, Theme};

// Note on hover precision: `hover_detail_at` is *line-level* only — it takes
// `(snapshot, file_path, line_index)` and selects the highest-severity
// diagnostic whose range covers that line. Column position is not
// considered, so hovering any column on a flagged line surfaces the same
// detail. A future enhancement to support LSP-style span-precise hover
// would extend the signature with a column argument and clip diagnostics
// to `[start_character, end_character)` on the matching line.

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
    theme: &Theme,
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
                style_for_severity(&span.severity, theme),
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

/// Resolve diagnostic severity to a themed style via [`Theme::style`].
fn style_for_severity(sev: &LspSeverity, theme: &Theme) -> Style {
    match sev {
        LspSeverity::Error => theme.style(StyleKey::DiagError),
        LspSeverity::Warning => theme.style(StyleKey::DiagWarning),
        LspSeverity::Information | LspSeverity::Hint => theme.style(StyleKey::DiagInfo),
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

// --- R6 / PR-T15 --- gutter severity marks -----------------------------------

/// A single-glyph gutter mark produced from the highest-severity diagnostic
/// intersecting a given line. Consumed by Review diff + VIL issue list
/// renderers to surface severity before the diagnostic text (R6 / T15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GutterMark {
    pub severity: LspSeverity,
    pub glyph: char,
}

impl GutterMark {
    pub fn new(severity: LspSeverity) -> Self {
        let glyph = glyph_for_severity(&severity);
        Self { severity, glyph }
    }
}

/// Pick the highest-severity diagnostic that covers `line_index` (0-based) in
/// `file_path` and project it into a [`GutterMark`]. Returns `None` when no
/// diagnostic intersects the line. Severity ordering is
/// `Error > Warning > Information > Hint`.
pub fn gutter_mark_for_line(
    snapshot: &LspWorkspaceSnapshot,
    file_path: &Path,
    line_index: u32,
) -> Option<GutterMark> {
    let mut best: Option<LspSeverity> = None;
    for diag in &snapshot.diagnostics {
        if diag.file_path != file_path {
            continue;
        }
        let r = &diag.range;
        if line_index < r.start_line || line_index > r.end_line {
            continue;
        }
        let sev = diag.severity.clone();
        best = Some(match best {
            None => sev,
            Some(cur) if severity_rank(&sev) > severity_rank(&cur) => sev,
            Some(cur) => cur,
        });
    }
    best.map(GutterMark::new)
}

/// Render the 2-column gutter cell for a line: `"<glyph> "` styled by severity
/// when a mark is present, or `"  "` styled with `base` otherwise. Callers
/// prepend this unconditionally so row prefix width stays constant (2 cols).
pub fn render_gutter_cell(mark: Option<&GutterMark>, base: Style, theme: &Theme) -> Span<'static> {
    match mark {
        Some(m) => Span::styled(
            format!("{} ", m.glyph),
            style_for_severity(&m.severity, theme).add_modifier(Modifier::BOLD),
        ),
        None => Span::styled("  ".to_string(), base),
    }
}

fn glyph_for_severity(sev: &LspSeverity) -> char {
    match sev {
        LspSeverity::Error => '●',
        LspSeverity::Warning => '▲',
        LspSeverity::Information | LspSeverity::Hint => 'ℹ',
    }
}

fn severity_rank(sev: &LspSeverity) -> u8 {
    match sev {
        LspSeverity::Error => 4,
        LspSeverity::Warning => 3,
        LspSeverity::Information => 2,
        LspSeverity::Hint => 1,
    }
}

// --- R7 / PR-T15 --- hover detail popup --------------------------------------

/// Rich diagnostic detail surfaced by the hover popup (R7 / T15). Picks the
/// highest-severity diagnostic intersecting a given 0-based `line_index` in
/// `file_path` and snapshots the user-visible fields needed to render a
/// popup: severity, message, optional `code`, optional `source`.
///
/// Decoupled from [`GutterMark`] so the gutter remains cheap to compute
/// per-line while the popup only pays the string-clone cost on click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverDetail {
    pub severity: LspSeverity,
    pub message: String,
    pub code: Option<String>,
    pub source: Option<String>,
    /// The 0-based line index the hover was anchored to. Used by the
    /// renderer to reposition the popup when the underlying row scrolls.
    pub line_index: u32,
}

/// Pick the highest-severity diagnostic that covers `line_index` (0-based)
/// in `file_path` and project it into a [`HoverDetail`]. Returns `None` when
/// no diagnostic intersects the line. Severity ordering matches
/// [`gutter_mark_for_line`] (`Error > Warning > Information > Hint`).
///
/// When multiple diagnostics share the top severity, the first one wins
/// (matches LSP client convention of preserving server order).
pub fn hover_detail_at(
    snapshot: &LspWorkspaceSnapshot,
    file_path: &Path,
    line_index: u32,
) -> Option<HoverDetail> {
    let mut best: Option<&vac_core::lsp::types::LspDiagnostic> = None;
    for diag in &snapshot.diagnostics {
        if diag.file_path != file_path {
            continue;
        }
        let r = &diag.range;
        if line_index < r.start_line || line_index > r.end_line {
            continue;
        }
        best = Some(match best {
            None => diag,
            Some(cur) if severity_rank(&diag.severity) > severity_rank(&cur.severity) => diag,
            Some(cur) => cur,
        });
    }
    best.map(|d| HoverDetail {
        severity: d.severity.clone(),
        message: d.message.clone(),
        code: d.code.clone(),
        source: d.source.clone(),
        line_index,
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use ratatui::style::{Modifier, Style};
    use crate::services::theme::Theme;
    use vac_core::lsp::types::{LspDiagnostic, LspRange, LspSeverity, LspWorkspaceSnapshot};

    fn test_theme() -> Theme {
        Theme::default()
    }

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

        let theme = test_theme();
        let rendered = render_line_with_diagnostics("let x = broken_call;", &spans, Style::default(), &theme);
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
            rendered.spans[1].style,
            theme.style(StyleKey::DiagError),
            "error severity must use DiagError theme style"
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
        let theme = test_theme();
        let rendered = render_line_with_diagnostics("plain", &[], Style::default(), &theme);
        assert_eq!(rendered.spans.len(), 1);
        assert_eq!(rendered.spans[0].content, "plain");
    }

    // --- R6 / PR-T15 gutter marks ------------------------------------------

    fn diag_with(path: &str, sev: LspSeverity, sl: u32, sc: u32, el: u32, ec: u32) -> LspDiagnostic {
        LspDiagnostic {
            file_path: PathBuf::from(path),
            severity: sev,
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
    fn gutter_mark_prefers_highest_severity() {
        // Two diagnostics on the same line: a Warning and an Error. The
        // Error must win because it has higher severity rank.
        let snap = snapshot_with(vec![
            diag_with("foo.rs", LspSeverity::Warning, 1, 0, 1, 5),
            diag_with("foo.rs", LspSeverity::Error, 1, 3, 1, 8),
            diag_with("foo.rs", LspSeverity::Hint, 1, 0, 1, 2),
        ]);
        let mark = gutter_mark_for_line(&snap, Path::new("foo.rs"), 1).expect("mark");
        assert_eq!(mark.severity, LspSeverity::Error);
        assert_eq!(mark.glyph, '●');
    }

    #[test]
    fn gutter_mark_none_when_no_diagnostics() {
        let snap = snapshot_with(vec![diag_with("foo.rs", LspSeverity::Error, 0, 0, 0, 5)]);
        // Different line in same file.
        assert!(gutter_mark_for_line(&snap, Path::new("foo.rs"), 4).is_none());
        // Empty snapshot.
        let empty = snapshot_with(vec![]);
        assert!(gutter_mark_for_line(&empty, Path::new("foo.rs"), 0).is_none());
    }

    #[test]
    fn gutter_mark_covers_multiline_range() {
        // Warning spans lines 2..=4. Every line in the range should yield a
        // gutter mark, including the middle line 3.
        let snap = snapshot_with(vec![diag_with(
            "foo.rs",
            LspSeverity::Warning,
            2,
            3,
            4,
            1,
        )]);
        for line in 2..=4 {
            let mark = gutter_mark_for_line(&snap, Path::new("foo.rs"), line)
                .unwrap_or_else(|| panic!("expected mark on line {line}"));
            assert_eq!(mark.severity, LspSeverity::Warning);
            assert_eq!(mark.glyph, '▲');
        }
        // Outside the range: no mark.
        assert!(gutter_mark_for_line(&snap, Path::new("foo.rs"), 1).is_none());
        assert!(gutter_mark_for_line(&snap, Path::new("foo.rs"), 5).is_none());
    }

    #[test]
    fn gutter_mark_ignores_other_files() {
        let snap = snapshot_with(vec![
            diag_with("foo.rs", LspSeverity::Error, 0, 0, 0, 5),
            diag_with("bar.rs", LspSeverity::Error, 0, 0, 0, 5),
        ]);
        // Only foo.rs matches, but we are asking about bar.rs line 0 — that
        // should still produce a mark from the bar.rs diag only. The check
        // we care about is that querying `baz.rs` returns None.
        assert!(gutter_mark_for_line(&snap, Path::new("foo.rs"), 0).is_some());
        assert!(gutter_mark_for_line(&snap, Path::new("bar.rs"), 0).is_some());
        assert!(gutter_mark_for_line(&snap, Path::new("baz.rs"), 0).is_none());
    }

    #[test]
    fn render_gutter_cell_styled_by_severity() {
        let theme = test_theme();

        let err_mark = GutterMark::new(LspSeverity::Error);
        let err_span = render_gutter_cell(Some(&err_mark), Style::default(), &theme);
        assert_eq!(err_span.content, "● ");
        // DiagError style + BOLD applied by render_gutter_cell.
        let expected_err = theme.style(StyleKey::DiagError).add_modifier(Modifier::BOLD);
        assert_eq!(err_span.style, expected_err);

        let warn_mark = GutterMark::new(LspSeverity::Warning);
        let warn_span = render_gutter_cell(Some(&warn_mark), Style::default(), &theme);
        assert_eq!(warn_span.content, "▲ ");
        let expected_warn = theme.style(StyleKey::DiagWarning).add_modifier(Modifier::BOLD);
        assert_eq!(warn_span.style, expected_warn);

        let info_mark = GutterMark::new(LspSeverity::Information);
        let info_span = render_gutter_cell(Some(&info_mark), Style::default(), &theme);
        assert_eq!(info_span.content, "ℹ ");
        let expected_info = theme.style(StyleKey::DiagInfo).add_modifier(Modifier::BOLD);
        assert_eq!(info_span.style, expected_info);

        let hint_mark = GutterMark::new(LspSeverity::Hint);
        let hint_span = render_gutter_cell(Some(&hint_mark), Style::default(), &theme);
        // Hint shares the info glyph by design (single-char gutter budget).
        assert_eq!(hint_span.content, "ℹ ");
        assert_eq!(hint_span.style, expected_info);

        // None — returns a 2-space placeholder so row width stays constant.
        let empty_span = render_gutter_cell(None, Style::default(), &theme);
        assert_eq!(empty_span.content, "  ");
        assert_eq!(empty_span.style.fg, None);
    }

    // --- R7 / PR-T15 hover detail popup ------------------------------------

    fn diag_full(
        path: &str,
        sev: LspSeverity,
        code: Option<&str>,
        source: Option<&str>,
        msg: &str,
        sl: u32,
        sc: u32,
        el: u32,
        ec: u32,
    ) -> LspDiagnostic {
        LspDiagnostic {
            file_path: PathBuf::from(path),
            severity: sev,
            code: code.map(str::to_string),
            source: source.map(str::to_string),
            message: msg.to_string(),
            range: LspRange {
                start_line: sl,
                start_character: sc,
                end_line: el,
                end_character: ec,
            },
        }
    }

    #[test]
    fn hover_detail_inside_span_returns_diag() {
        let snap = snapshot_with(vec![diag_full(
            "foo.rs",
            LspSeverity::Warning,
            Some("W0042"),
            Some("vil_validate"),
            "unused variable `x`",
            3,
            2,
            3,
            7,
        )]);
        let detail = hover_detail_at(&snap, Path::new("foo.rs"), 3).expect("hover");
        assert_eq!(detail.severity, LspSeverity::Warning);
        assert_eq!(detail.message, "unused variable `x`");
        assert_eq!(detail.code.as_deref(), Some("W0042"));
        assert_eq!(detail.source.as_deref(), Some("vil_validate"));
        assert_eq!(detail.line_index, 3);
    }

    #[test]
    fn hover_detail_outside_span_returns_none() {
        let snap = snapshot_with(vec![diag_full(
            "foo.rs",
            LspSeverity::Error,
            None,
            None,
            "boom",
            5,
            0,
            5,
            3,
        )]);
        // Different line in same file.
        assert!(hover_detail_at(&snap, Path::new("foo.rs"), 6).is_none());
        // Right line, different file.
        assert!(hover_detail_at(&snap, Path::new("bar.rs"), 5).is_none());
        // Empty snapshot.
        let empty = snapshot_with(vec![]);
        assert!(hover_detail_at(&empty, Path::new("foo.rs"), 5).is_none());
    }

    #[test]
    fn hover_detail_picks_highest_severity_when_overlapping() {
        // Three diagnostics on the same line. The Error must win regardless
        // of insertion order; the returned detail must be the Error-level
        // payload, not the Warning/Hint ones.
        let snap = snapshot_with(vec![
            diag_full(
                "foo.rs",
                LspSeverity::Warning,
                Some("W001"),
                None,
                "warn msg",
                1,
                0,
                1,
                10,
            ),
            diag_full(
                "foo.rs",
                LspSeverity::Error,
                Some("E500"),
                Some("vil_validate"),
                "err msg",
                1,
                3,
                1,
                8,
            ),
            diag_full(
                "foo.rs",
                LspSeverity::Hint,
                None,
                None,
                "hint msg",
                1,
                0,
                1,
                2,
            ),
        ]);
        let detail = hover_detail_at(&snap, Path::new("foo.rs"), 1).expect("hover");
        assert_eq!(detail.severity, LspSeverity::Error);
        assert_eq!(detail.code.as_deref(), Some("E500"));
        assert_eq!(detail.message, "err msg");
        assert_eq!(detail.source.as_deref(), Some("vil_validate"));
    }

    #[test]
    fn hover_detail_covers_multiline_range() {
        // Warning spans lines 2..=4 with message "multi". Hover on the
        // middle line must still return the detail, not None.
        let snap = snapshot_with(vec![diag_full(
            "foo.rs",
            LspSeverity::Warning,
            None,
            None,
            "multi",
            2,
            3,
            4,
            1,
        )]);
        let mid = hover_detail_at(&snap, Path::new("foo.rs"), 3).expect("mid line");
        assert_eq!(mid.severity, LspSeverity::Warning);
        assert_eq!(mid.message, "multi");
        // Outside the multi-line range: no detail.
        assert!(hover_detail_at(&snap, Path::new("foo.rs"), 1).is_none());
        assert!(hover_detail_at(&snap, Path::new("foo.rs"), 5).is_none());
    }
}
