use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_approval_detail::{
    ApprovalDetailViewState, DetailEvent, DetailKey, on_key, render_approval_detail,
};
use vac_shell_contracts::{ApprovalDetailView, RiskLevel};

fn fixture() -> ApprovalDetailViewState {
    ApprovalDetailViewState {
        visible: true,
        detail: Some(ApprovalDetailView {
            id: "tool-9".into(),
            tool_name: "shell".into(),
            risk_level: RiskLevel::High,
            reason: "running rm -rf inside repo".into(),
            command_preview: Some("rm -rf target/".into()),
            file_preview: None,
            policy_source: Some("vil.core / shell.delete".into()),
        }),
    }
}

fn render(view: &ApprovalDetailViewState) -> String {
    let backend = TestBackend::new(80, 16);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| render_approval_detail(f, view, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width { s.push_str(buf[(x, y)].symbol()); }
        s.push('\n');
    }
    s
}

#[test]
fn render_high_risk_approval() {
    let s = render(&fixture());
    assert!(s.contains("shell"));
    assert!(s.contains("high"));
    assert!(s.contains("rm -rf target/"));
    assert!(s.contains("vil.core / shell.delete"));
}

#[test]
fn approve_emits_event_with_id() {
    let mut v = fixture();
    let event = on_key(&mut v, DetailKey::Approve);
    assert_eq!(event, DetailEvent::Approve("tool-9".into()));
}

#[test]
fn reject_emits_event_with_id() {
    let mut v = fixture();
    let event = on_key(&mut v, DetailKey::Reject);
    assert_eq!(event, DetailEvent::Reject("tool-9".into()));
}

#[test]
fn escape_dismisses() {
    let mut v = fixture();
    let event = on_key(&mut v, DetailKey::Escape);
    assert_eq!(event, DetailEvent::Dismissed);
    assert!(!v.visible);
}

#[test]
fn no_detail_renders_placeholder() {
    let v = ApprovalDetailViewState { visible: true, detail: None };
    let s = render(&v);
    assert!(s.contains("no approval selected"));
}
