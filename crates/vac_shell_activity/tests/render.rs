use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_activity::{ActivityView, render_activity};
use vac_shell_contracts::{Severity, ShellActivityEntry, ShellActivityKind};

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
        for x in 0..buf.area.width { s.push_str(buf[(x, y)].symbol()); }
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
