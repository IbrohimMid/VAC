use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_shell_contracts::ShellStatusView;
use vac_shell_status_bar::render_status_bar;

fn render(view: &ShellStatusView) -> String {
    let backend = TestBackend::new(120, 1);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| render_status_bar(f, view, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for x in 0..buf.area.width {
        s.push_str(buf[(x, 0)].symbol());
    }
    s
}

#[test]
fn render_status_basic() {
    let v = ShellStatusView {
        surface: Some("chat".into()),
        model_label: Some("claude-sonnet-4.5".into()),
        cwd: "/home/emp/repo".into(),
        ..Default::default()
    };
    let s = render(&v);
    assert!(s.contains("CHAT"));
    assert!(s.contains("claude-sonnet-4.5"));
    assert!(s.contains("/home/emp/repo"));
}

#[test]
fn render_pending_approval_badge() {
    let v = ShellStatusView {
        surface: Some("chat".into()),
        cwd: "/repo".into(),
        pending_approvals: 3,
        ..Default::default()
    };
    let s = render(&v);
    assert!(s.contains("approvals"));
    assert!(s.contains("3"));
}

#[test]
fn render_error_state() {
    let v = ShellStatusView {
        surface: Some("chat".into()),
        cwd: "/repo".into(),
        last_error: Some("provider unreachable".into()),
        ..Default::default()
    };
    let s = render(&v);
    assert!(s.contains("error"));
    assert!(s.contains("provider unreachable"));
}

#[test]
fn render_no_model_shows_em_dash() {
    let v = ShellStatusView {
        surface: Some("runtime".into()),
        cwd: "/repo".into(),
        ..Default::default()
    };
    let s = render(&v);
    assert!(s.contains("RUNTIME"));
    assert!(s.contains("—"));
}

#[test]
fn render_with_git_branch() {
    let v = ShellStatusView {
        surface: Some("chat".into()),
        cwd: "/repo".into(),
        git_branch: Some("main".into()),
        ..Default::default()
    };
    let s = render(&v);
    assert!(s.contains("git"));
    assert!(s.contains("main"));
}
