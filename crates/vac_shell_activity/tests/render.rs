use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_shell_activity::{
    ActivityLogBrowserView, ActivityView, LogsBrowserKey, on_logs_browser_key, render_activity,
    render_logs_browser,
};
use vac_shell_contracts::{Severity, ShellActivityEntry, ShellActivityFilter, ShellActivityKind};

fn entry(kind: ShellActivityKind, severity: Severity, title: &str) -> ShellActivityEntry {
    ShellActivityEntry {
        id: "x".into(),
        ts_unix: 0,
        kind,
        title: title.into(),
        detail: None,
        severity,
    }
}

fn render(view: &ActivityView) -> String {
    let backend = TestBackend::new(80, 12);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| render_activity(f, view, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

#[test]
fn empty_activity_renders_placeholder() {
    let v = ActivityView::default();
    let s = render(&v);
    assert!(s.contains("no activity yet"));
}

#[test]
fn render_activity_entries_with_kind_and_title() {
    let v = ActivityView {
        entries: vec![
            entry(ShellActivityKind::UserInput, Severity::Info, "operator: hi"),
            entry(ShellActivityKind::ToolCall, Severity::Info, "shell: ls -la"),
            entry(ShellActivityKind::Error, Severity::Error, "boom"),
        ],
        scroll: 0,
    };
    let s = render(&v);
    assert!(s.contains("user"));
    assert!(s.contains("operator: hi"));
    assert!(s.contains("tool"));
    assert!(s.contains("ls -la"));
    assert!(s.contains("error"));
}

#[test]
fn scroll_advances_window() {
    let mut v = ActivityView::default();
    for i in 0..30 {
        v.entries.push(entry(
            ShellActivityKind::UserInput,
            Severity::Info,
            &format!("line {i}"),
        ));
    }
    v.scroll = 25;
    let s = render(&v);
    assert!(s.contains("line 25"));
    assert!(!s.contains("line 0"));
}

#[test]
fn renders_diagnostic_and_status_labels() {
    let v = ActivityView {
        entries: vec![
            entry(
                ShellActivityKind::Diagnostic,
                Severity::Ok,
                "doctor: model config — Ok",
            ),
            entry(
                ShellActivityKind::Status,
                Severity::Ok,
                "status: cockpit ok",
            ),
            entry(ShellActivityKind::ToolResult, Severity::Ok, "glob ok"),
        ],
        scroll: 0,
    };
    let s = render(&v);

    assert!(
        s.contains("diag"),
        "Diagnostic rows should render as 'diag'"
    );
    assert!(
        s.contains("status"),
        "Status rows should render as 'status'"
    );
    assert!(
        s.contains("tool·ok"),
        "ToolResult rows should render as 'tool·ok'"
    );

    assert!(s.contains("doctor: model config"));
    assert!(s.contains("status: cockpit ok"));
    assert!(s.contains("glob ok"));
}

// =====================================================================
// Logs Browser Tests
// =====================================================================

fn logs_entry(kind: ShellActivityKind, severity: Severity, title: &str) -> ShellActivityEntry {
    ShellActivityEntry {
        id: "x".into(),
        ts_unix: 0,
        kind,
        title: title.into(),
        detail: None,
        severity,
    }
}

fn render_logs(view: &ActivityLogBrowserView) -> String {
    let backend = TestBackend::new(80, 24);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| render_logs_browser(f, view, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

#[test]
fn logs_empty_entries_renders_no_log_entries() {
    let v = ActivityLogBrowserView::default();
    let s = render_logs(&v);
    assert!(s.contains("no log entries"));
    assert!(s.contains("Logs Browser"));
}

#[test]
fn logs_filter_all_returns_all() {
    let mut v = ActivityLogBrowserView {
        entries: vec![
            logs_entry(ShellActivityKind::Diagnostic, Severity::Info, "d1"),
            logs_entry(ShellActivityKind::Error, Severity::Error, "e1"),
            logs_entry(ShellActivityKind::Status, Severity::Info, "s1"),
        ],
        filter: ShellActivityFilter::All,
        search_query: String::new(),
        search_mode: false,
        scroll: 0,
        ..Default::default()
    };
    on_logs_browser_key(&mut v, LogsBrowserKey::FilterAll);
    assert_eq!(v.filter, ShellActivityFilter::All);
}

#[test]
fn logs_filter_errors_returns_error_severity() {
    let mut v = ActivityLogBrowserView {
        entries: vec![
            logs_entry(ShellActivityKind::Diagnostic, Severity::Info, "d1"),
            logs_entry(ShellActivityKind::Error, Severity::Error, "e1"),
            logs_entry(ShellActivityKind::Status, Severity::Info, "s1"),
        ],
        filter: ShellActivityFilter::All,
        search_query: String::new(),
        search_mode: false,
        scroll: 0,
        ..Default::default()
    };
    on_logs_browser_key(&mut v, LogsBrowserKey::FilterErrors);
    assert_eq!(v.filter, ShellActivityFilter::Errors);
}

#[test]
fn logs_filter_warnings_returns_warn() {
    let mut v = ActivityLogBrowserView {
        entries: vec![
            logs_entry(ShellActivityKind::Diagnostic, Severity::Info, "d1"),
            logs_entry(ShellActivityKind::Status, Severity::Warn, "w1"),
        ],
        filter: ShellActivityFilter::All,
        search_query: String::new(),
        search_mode: false,
        scroll: 0,
        ..Default::default()
    };
    on_logs_browser_key(&mut v, LogsBrowserKey::FilterWarnings);
    assert_eq!(v.filter, ShellActivityFilter::Warnings);
}

#[test]
fn logs_key_filter_resets_scroll() {
    let mut v = ActivityLogBrowserView {
        entries: vec![logs_entry(
            ShellActivityKind::Diagnostic,
            Severity::Info,
            "d1",
        )],
        filter: ShellActivityFilter::All,
        search_query: String::new(),
        search_mode: false,
        scroll: 10,
        ..Default::default()
    };
    on_logs_browser_key(&mut v, LogsBrowserKey::FilterErrors);
    assert_eq!(v.scroll, 0);
}

#[test]
fn logs_key_search_mode_appends_chars() {
    let mut v = ActivityLogBrowserView::default();
    on_logs_browser_key(&mut v, LogsBrowserKey::Search);
    assert!(v.search_mode);
    on_logs_browser_key(&mut v, LogsBrowserKey::Char('f'));
    on_logs_browser_key(&mut v, LogsBrowserKey::Char('o'));
    on_logs_browser_key(&mut v, LogsBrowserKey::Char('o'));
    assert_eq!(v.search_query, "foo");
}

#[test]
fn logs_key_backspace_edits_search() {
    let mut v = ActivityLogBrowserView {
        search_mode: true,
        search_query: "foo".into(),
        ..Default::default()
    };
    on_logs_browser_key(&mut v, LogsBrowserKey::Backspace);
    assert_eq!(v.search_query, "fo");
}

#[test]
fn logs_render_populated_includes_logs_browser_title() {
    let v = ActivityLogBrowserView {
        entries: vec![logs_entry(
            ShellActivityKind::Status,
            Severity::Ok,
            "test row",
        )],
        filter: ShellActivityFilter::All,
        search_query: String::new(),
        search_mode: false,
        scroll: 0,
        ..Default::default()
    };
    let s = render_logs(&v);
    assert!(s.contains("Logs Browser"));
    assert!(s.contains("status"));
    assert!(s.contains("test row"));
}

#[test]
fn logs_search_matches_kind_label() {
    let v = ActivityLogBrowserView {
        entries: vec![logs_entry(
            ShellActivityKind::Diagnostic,
            Severity::Info,
            "some entry",
        )],
        filter: ShellActivityFilter::All,
        search_query: "diag".into(),
        search_mode: false,
        scroll: 0,
        ..Default::default()
    };
    let s = render_logs(&v);
    assert!(s.contains("diag"));
}

#[test]
fn logs_search_matches_severity_label() {
    let v = ActivityLogBrowserView {
        entries: vec![logs_entry(
            ShellActivityKind::Error,
            Severity::Error,
            "some entry",
        )],
        filter: ShellActivityFilter::All,
        search_query: "error".into(),
        search_mode: false,
        scroll: 0,
        ..Default::default()
    };
    let s = render_logs(&v);
    assert!(s.contains("error"));
}

#[test]
fn logs_empty_filtered_renders_no_matching_logs() {
    let v = ActivityLogBrowserView {
        entries: vec![logs_entry(ShellActivityKind::Status, Severity::Info, "s1")],
        filter: ShellActivityFilter::Errors,
        search_query: String::new(),
        search_mode: false,
        scroll: 0,
        ..Default::default()
    };
    let s = render_logs(&v);
    assert!(s.contains("no matching logs"));
}

#[test]
fn logs_render_includes_footer_controls() {
    let v = ActivityLogBrowserView {
        entries: vec![logs_entry(
            ShellActivityKind::Status,
            Severity::Info,
            "test",
        )],
        filter: ShellActivityFilter::All,
        search_query: String::new(),
        search_mode: false,
        scroll: 0,
        ..Default::default()
    };
    let s = render_logs(&v);
    assert!(s.contains("filter"));
    assert!(s.contains("scroll"));
}

#[test]
fn logs_search_mode_shows_search_status() {
    let v = ActivityLogBrowserView {
        entries: vec![logs_entry(
            ShellActivityKind::Status,
            Severity::Info,
            "test",
        )],
        filter: ShellActivityFilter::All,
        search_query: "test query".into(),
        search_mode: true,
        scroll: 0,
        ..Default::default()
    };
    let s = render_logs(&v);
    assert!(s.contains("search: test query"));
}
