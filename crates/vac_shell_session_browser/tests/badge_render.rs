//! D10 — session browser badge rendering contract tests.

use ratatui::{Terminal, backend::TestBackend};
use vac_shell_contracts::{SessionEntry, SessionTileView, SessionToolSummary};
use vac_shell_session_browser::{SessionBrowserView, render_session_browser};

fn make_entry(id: &str) -> SessionEntry {
    SessionEntry {
        id: id.to_string(),
        label: id.to_string(),
        last_active_unix: 0,
    }
}

#[test]
fn badge_text_contains_ok_and_err_counts() {
    let summary = SessionToolSummary {
        total_calls: 3,
        ok_count: 2,
        error_count: 1,
        ..Default::default()
    };
    assert_eq!(summary.badge_text(), "tools: 2 ok / 1 err");
}

#[test]
fn badge_text_no_tools_when_zero_calls() {
    let summary = SessionToolSummary::default();
    assert_eq!(summary.badge_text(), "no tools");
}

#[test]
fn render_shows_badge_for_tile_with_summary() {
    let mut view = SessionBrowserView {
        visible: true,
        tiles: vec![SessionTileView {
            entry: make_entry("sess-a"),
            tool_summary: Some(SessionToolSummary {
                total_calls: 3,
                ok_count: 2,
                error_count: 1,
                ..Default::default()
            }),
            tool_details: vec![],
            recovery: None,
        }],
        ..Default::default()
    };
    // Ensure it's visible.
    view.visible = true;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            render_session_browser(f, &view, f.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(
        content.contains("tools: 2 ok / 1 err"),
        "expected badge in output, got: {content:?}"
    );
}

#[test]
fn render_shows_no_tools_when_zero_calls() {
    let view = SessionBrowserView {
        visible: true,
        tiles: vec![SessionTileView {
            entry: make_entry("sess-b"),
            tool_summary: Some(SessionToolSummary::default()),
            tool_details: vec![],
            recovery: None,
        }],
        ..Default::default()
    };

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            render_session_browser(f, &view, f.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(
        content.contains("no tools"),
        "expected 'no tools' badge in output, got: {content:?}"
    );
}

#[test]
fn render_skips_badge_when_tool_summary_none() {
    let view = SessionBrowserView {
        visible: true,
        tiles: vec![SessionTileView {
            entry: make_entry("sess-c"),
            tool_summary: None,
            tool_details: vec![],
            recovery: None,
        }],
        ..Default::default()
    };

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            render_session_browser(f, &view, f.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(
        !content.contains("tools:"),
        "unexpected badge in output when summary is None: {content:?}"
    );
}
