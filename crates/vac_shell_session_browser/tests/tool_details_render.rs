//! D11 — session browser tool detail rendering tests.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_shell_contracts::{
    SessionEntry, SessionTileView, SessionToolSummary, SessionToolUseDetail, ToolUseUiStatus,
};
use vac_shell_session_browser::{SessionBrowserView, render_session_browser};

fn make_entry(id: &str) -> SessionEntry {
    SessionEntry {
        id: id.to_string(),
        label: format!("Session {id}"),
        last_active_unix: 1000,
    }
}

#[test]
fn render_shows_tool_details_in_right_panel() {
    let tiles = vec![SessionTileView {
        entry: make_entry("a"),
        tool_summary: Some(SessionToolSummary {
            total_calls: 4,
            ok_count: 1,
            error_count: 1,
            warning_count: 1,
            pending_count: 1,
            ..Default::default()
        }),
        tool_details: vec![
            SessionToolUseDetail {
                call_id: "c1".into(),
                tool_name: "glob".into(),
                status: ToolUseUiStatus::Ok,
                summary: "found 5 files".into(),
                duration_ms: 15,
            },
            SessionToolUseDetail {
                call_id: "c2".into(),
                tool_name: "shell_run".into(),
                status: ToolUseUiStatus::Error,
                summary: "exit code 1".into(),
                duration_ms: 200,
            },
            SessionToolUseDetail {
                call_id: "c3".into(),
                tool_name: "read_file".into(),
                status: ToolUseUiStatus::Warning,
                summary: "file too large".into(),
                duration_ms: 5,
            },
            SessionToolUseDetail {
                call_id: "c4".into(),
                tool_name: "write_file".into(),
                status: ToolUseUiStatus::Pending,
                summary: "waiting...".into(),
                duration_ms: 0,
            },
        ],
        recovery: None,
    }];

    let view = SessionBrowserView {
        visible: true,
        tiles,
        selected: 0,
        ..Default::default()
    };

    let backend = TestBackend::new(100, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_session_browser(f, &view, f.area()))
        .unwrap();

    let buf = terminal.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }

    assert!(s.contains("Tools"));
    assert!(s.contains("glob"));
    assert!(s.contains("found 5 files"));
    assert!(s.contains("15ms"));

    assert!(s.contains("shell_run"));
    assert!(s.contains("exit code 1"));
    assert!(s.contains("200ms"));

    assert!(s.contains("read_file"));
    assert!(s.contains("file too large"));
    assert!(s.contains("5ms"));

    assert!(s.contains("write_file"));
    assert!(s.contains("waiting..."));
    assert!(s.contains("pending"));
}

#[test]
fn render_shows_overflow_label() {
    let mut tool_details = Vec::new();
    for i in 0..10 {
        tool_details.push(SessionToolUseDetail {
            call_id: format!("c{i}"),
            tool_name: format!("tool_{i}"),
            status: ToolUseUiStatus::Ok,
            summary: "ok".into(),
            duration_ms: 1,
        });
    }

    let tiles = vec![SessionTileView {
        entry: make_entry("a"),
        tool_summary: Some(SessionToolSummary {
            total_calls: 10,
            ok_count: 10,
            ..Default::default()
        }),
        tool_details,
        recovery: None,
    }];

    let view = SessionBrowserView {
        visible: true,
        tiles,
        selected: 0,
        ..Default::default()
    };

    let backend = TestBackend::new(100, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_session_browser(f, &view, f.area()))
        .unwrap();

    let buf = terminal.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }

    assert!(s.contains("tool_0"));
    assert!(s.contains("tool_7"));
    assert!(!s.contains("tool_8")); // Only first 8 should be rendered
    assert!(s.contains("... and 2 more"));
}

#[test]
fn render_shows_fallback_when_details_missing_but_summary_present() {
    let tiles = vec![SessionTileView {
        entry: make_entry("a"),
        tool_summary: Some(SessionToolSummary {
            total_calls: 5,
            ok_count: 5,
            ..Default::default()
        }),
        tool_details: vec![],
        recovery: None,
    }];

    let view = SessionBrowserView {
        visible: true,
        tiles,
        selected: 0,
        ..Default::default()
    };

    let backend = TestBackend::new(100, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_session_browser(f, &view, f.area()))
        .unwrap();

    let buf = terminal.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }

    assert!(s.contains("Tools"));
    assert!(s.contains("tool details unavailable"));
}
