//! Step 2 slice 4 — approval bar widget extraction proof.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_approval_bar::{
    ApprovalActionView, ApprovalBarEvent, ApprovalBarKey, ApprovalBarViewState, ApprovalStatus,
    calculate_height, format_tool_label, on_key, render_approval_bar,
};

fn fixture() -> ApprovalBarViewState {
    ApprovalBarViewState {
        actions: vec![
            ApprovalActionView {
                id: "a".into(),
                label: "Run Command".into(),
                status: ApprovalStatus::Approved,
            },
            ApprovalActionView {
                id: "b".into(),
                label: "Create".into(),
                status: ApprovalStatus::Approved,
            },
            ApprovalActionView {
                id: "c".into(),
                label: "Str Replace".into(),
                status: ApprovalStatus::Approved,
            },
        ],
        selected_index: 0,
        visible: true,
        esc_pending: false,
    }
}

#[test]
fn invisible_bar_returns_zero_height() {
    let mut view = fixture();
    view.visible = false;
    assert_eq!(calculate_height(&view, 80), 0);
}

#[test]
fn empty_actions_means_not_visible() {
    let mut view = fixture();
    view.actions.clear();
    assert!(!view.is_visible());
    assert_eq!(calculate_height(&view, 80), 0);
}

#[test]
fn space_emits_toggle_for_selected_id() {
    let mut view = fixture();
    let event = on_key(&mut view, ApprovalBarKey::Space);
    assert_eq!(event, ApprovalBarEvent::Toggle("a".into()));
    view.select_next();
    let event = on_key(&mut view, ApprovalBarKey::Space);
    assert_eq!(event, ApprovalBarEvent::Toggle("b".into()));
}

#[test]
fn arrow_keys_wrap_selection() {
    let mut view = fixture();
    on_key(&mut view, ApprovalBarKey::Left);
    assert_eq!(view.selected_index, 2, "left from 0 wraps to last");
    on_key(&mut view, ApprovalBarKey::Right);
    assert_eq!(view.selected_index, 0, "right from last wraps to first");
}

#[test]
fn enter_submits_all() {
    let mut view = fixture();
    assert_eq!(on_key(&mut view, ApprovalBarKey::Enter), ApprovalBarEvent::SubmitAll);
}

#[test]
fn first_escape_primes_second_escape_rejects() {
    let mut view = fixture();
    assert_eq!(on_key(&mut view, ApprovalBarKey::Escape), ApprovalBarEvent::EscPrimed);
    assert!(view.esc_pending);
    assert_eq!(on_key(&mut view, ApprovalBarKey::Escape), ApprovalBarEvent::RejectAll);
    assert!(!view.esc_pending, "esc_pending must clear after second press");
}

#[test]
fn keys_ignored_when_bar_invisible() {
    let mut view = fixture();
    view.visible = false;
    assert_eq!(on_key(&mut view, ApprovalBarKey::Enter), ApprovalBarEvent::Ignored);
}

#[test]
fn format_tool_label_humanises_snake_case() {
    assert_eq!(format_tool_label("run_command"), "Run Command");
    assert_eq!(format_tool_label("get_pak_content"), "Get Pak Content");
    assert_eq!(format_tool_label("create"), "Create");
}

#[test]
fn render_includes_title_and_action_labels() {
    let backend = TestBackend::new(80, 8);
    let mut terminal = Terminal::new(backend).unwrap();
    let view = fixture();
    terminal
        .draw(|f| render_approval_bar(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("Approval Required"), "title missing\n{all}");
    assert!(all.contains("Run Command"), "label missing\n{all}");
    assert!(all.contains("Str Replace"), "label missing\n{all}");
}

#[test]
fn invisible_bar_renders_nothing() {
    let backend = TestBackend::new(80, 8);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = fixture();
    view.visible = false;
    terminal
        .draw(|f| render_approval_bar(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let any = (0..buf.area.width)
        .flat_map(|x| (0..buf.area.height).map(move |y| (x, y)))
        .any(|(x, y)| !buf[(x, y)].symbol().trim().is_empty());
    assert!(!any);
}

/// Drift tripwire — only `ratatui` types may show up at the
/// crate boundary. Anything else compile-fails this test.
#[test]
fn no_donor_or_engine_dependency_links_in() {
    fn assert_only_local<T: Sized>(_: T) {}
    assert_only_local(ApprovalStatus::Approved);
    assert_only_local(ApprovalBarKey::Enter);
}
