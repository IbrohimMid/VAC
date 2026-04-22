use super::*;
use crate::services::theme::Theme;
use ratatui::style::{Modifier, Style};
use std::path::{Path, PathBuf};
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
    let rendered =
        render_line_with_diagnostics("let x = broken_call;", &spans, Style::default(), &theme);
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
    assert_eq!(
        spans.len(),
        1,
        "bar.rs diagnostic must not leak into foo.rs"
    );
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
    let snap = snapshot_with(vec![diag_with("foo.rs", LspSeverity::Warning, 2, 3, 4, 1)]);
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
    let expected_err = theme
        .style(StyleKey::DiagError)
        .add_modifier(Modifier::BOLD);
    assert_eq!(err_span.style, expected_err);

    let warn_mark = GutterMark::new(LspSeverity::Warning);
    let warn_span = render_gutter_cell(Some(&warn_mark), Style::default(), &theme);
    assert_eq!(warn_span.content, "▲ ");
    let expected_warn = theme
        .style(StyleKey::DiagWarning)
        .add_modifier(Modifier::BOLD);
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
