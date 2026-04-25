use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_contracts::{PlanMetadata, PlanStatus};
use vac_shell_plan_view::render_plan_view;

fn render(plan: Option<&PlanMetadata>) -> String {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render_plan_view(f, plan, f.area())).unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    all
}

#[test]
fn render_empty_plan_state() {
    let s = render(None);
    assert!(s.contains("no plan loaded"), "{s}");
    assert!(s.contains(".vac/session/plan.md"), "{s}");
}

#[test]
fn render_plan_status_active_with_steps() {
    let plan = PlanMetadata {
        status: PlanStatus::Active,
        objective: Some("Land the cockpit".into()),
        steps: vec!["extract".into(), "wire".into()],
        blocked_on: vec![],
    };
    let s = render(Some(&plan));
    assert!(s.contains("active"));
    assert!(s.contains("Land the cockpit"));
    assert!(s.contains("1. extract"));
    assert!(s.contains("2. wire"));
}

#[test]
fn render_plan_blocked_section() {
    let plan = PlanMetadata {
        status: PlanStatus::Blocked,
        objective: None,
        steps: vec![],
        blocked_on: vec!["review approval".into()],
    };
    let s = render(Some(&plan));
    assert!(s.contains("blocked"));
    assert!(s.contains("review approval"));
}
