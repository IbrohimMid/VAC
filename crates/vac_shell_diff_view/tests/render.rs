use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_contracts::{
    DiffFileView, DiffHunkView, DiffLineKind, DiffLineView, DiffReviewEvent,
};
use vac_shell_diff_view::{
    DiffReviewKey, DiffReviewKeyEvent, DiffReviewView, on_key, render_diff_review,
};

fn fixture() -> DiffReviewView {
    let f1 = DiffFileView {
        path: "src/lib.rs".into(),
        added: 3,
        removed: 1,
        hunks: vec![DiffHunkView {
            header: "@@ -1,3 +1,5 @@".into(),
            lines: vec![
                DiffLineView { kind: DiffLineKind::Context, text: "fn foo() {".into() },
                DiffLineView { kind: DiffLineKind::Removed, text: "    old();".into() },
                DiffLineView { kind: DiffLineKind::Added, text: "    new();".into() },
                DiffLineView { kind: DiffLineKind::Added, text: "    second();".into() },
                DiffLineView { kind: DiffLineKind::Added, text: "}".into() },
            ],
        }],
    };
    let f2 = DiffFileView {
        path: "README.md".into(),
        added: 1,
        removed: 0,
        hunks: vec![],
    };
    DiffReviewView {
        visible: true,
        files: vec![f1, f2],
        selected: 0,
        scroll: 0,
    }
}

fn render(view: &DiffReviewView) -> String {
    let backend = TestBackend::new(100, 16);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| render_diff_review(f, view, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width { s.push_str(buf[(x, y)].symbol()); }
        s.push('\n');
    }
    s
}

#[test]
fn empty_diff_shows_placeholder() {
    let v = DiffReviewView { visible: true, ..Default::default() };
    let s = render(&v);
    assert!(s.contains("no pending changes"));
}

#[test]
fn render_added_removed_lines_and_hunk_header() {
    let v = fixture();
    let s = render(&v);
    assert!(s.contains("src/lib.rs"));
    assert!(s.contains("+3"));
    assert!(s.contains("-1"));
    assert!(s.contains("@@ -1,3 +1,5 @@"));
    assert!(s.contains("+    new();"));
    assert!(s.contains("-    old();"));
}

#[test]
fn approve_emits_event_for_selected_file() {
    let mut v = fixture();
    let event = on_key(&mut v, DiffReviewKey::Approve);
    assert_eq!(
        event,
        DiffReviewKeyEvent::Event(DiffReviewEvent::ApproveFile("src/lib.rs".into()))
    );
}

#[test]
fn reject_emits_event_for_selected_file() {
    let mut v = fixture();
    on_key(&mut v, DiffReviewKey::Down);
    let event = on_key(&mut v, DiffReviewKey::Reject);
    assert_eq!(
        event,
        DiffReviewKeyEvent::Event(DiffReviewEvent::RejectFile("README.md".into()))
    );
}

#[test]
fn escape_dismisses_and_emits_event() {
    let mut v = fixture();
    let event = on_key(&mut v, DiffReviewKey::Escape);
    assert_eq!(event, DiffReviewKeyEvent::Event(DiffReviewEvent::Dismiss));
    assert!(!v.visible);
}

#[test]
fn page_down_advances_scroll() {
    let mut v = fixture();
    on_key(&mut v, DiffReviewKey::PageDown);
    assert_eq!(v.scroll, 8);
}
