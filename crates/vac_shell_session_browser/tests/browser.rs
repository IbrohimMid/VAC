use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_shell_contracts::{SessionAction, SessionEntry, SessionPreview, SessionTileView};
use vac_shell_session_browser::{
    SessionBrowserEvent, SessionBrowserKey, SessionBrowserView, on_key, render_session_browser,
};

fn tiles() -> Vec<SessionTileView> {
    vec![
        SessionTileView {
            entry: SessionEntry {
                id: "abc".into(),
                label: "Refactor swarm".into(),
                last_active_unix: 200,
            },
            tool_summary: None,
            tool_details: vec![],
        },
        SessionTileView {
            entry: SessionEntry {
                id: "def".into(),
                label: "Bump deps".into(),
                last_active_unix: 100,
            },
            tool_summary: None,
            tool_details: vec![],
        },
    ]
}

fn open_view() -> SessionBrowserView {
    let mut v = SessionBrowserView::default();
    v.visible = true;
    v.tiles = tiles();
    v
}

#[test]
fn session_browser_renders_entries_newest_first() {
    let mut v = open_view();
    v.preview = Some(SessionPreview {
        id: "abc".into(),
        title: Some("Refactor swarm".into()),
        lines: vec!["operator: extract palette".into()],
    });
    let backend = TestBackend::new(120, 14);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| render_session_browser(f, &v, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    assert!(s.contains("Refactor swarm"));
    assert!(s.contains("Bump deps"));
    assert!(s.contains("operator: extract palette"));
}

#[test]
fn session_search_filters_by_label() {
    let mut v = open_view();
    on_key(&mut v, SessionBrowserKey::Char('B'));
    on_key(&mut v, SessionBrowserKey::Char('u'));
    on_key(&mut v, SessionBrowserKey::Char('m'));
    let event = on_key(&mut v, SessionBrowserKey::Enter);
    match event {
        SessionBrowserEvent::Action(SessionAction::Open { id }) => assert_eq!(id, "def"),
        other => panic!("expected Open(def), got {other:?}"),
    }
}

#[test]
fn resume_emits_shell_action() {
    let mut v = open_view();
    let event = on_key(&mut v, SessionBrowserKey::Resume);
    assert_eq!(
        event,
        SessionBrowserEvent::Action(SessionAction::Resume { id: "abc".into() })
    );
}

#[test]
fn delete_requires_two_keypresses() {
    let mut v = open_view();
    let first = on_key(&mut v, SessionBrowserKey::Delete);
    assert_eq!(first, SessionBrowserEvent::Consumed);
    assert!(v.delete_pending);
    let second = on_key(&mut v, SessionBrowserKey::Delete);
    assert_eq!(
        second,
        SessionBrowserEvent::Action(SessionAction::Delete { id: "abc".into() })
    );
    assert!(!v.delete_pending);
}

#[test]
fn escape_cancels_pending_delete_first_then_dismisses() {
    let mut v = open_view();
    on_key(&mut v, SessionBrowserKey::Delete);
    assert!(v.delete_pending);
    let cancel = on_key(&mut v, SessionBrowserKey::Escape);
    assert_eq!(cancel, SessionBrowserEvent::Consumed);
    assert!(!v.delete_pending);
    assert!(v.visible);
    let dismiss = on_key(&mut v, SessionBrowserKey::Escape);
    assert_eq!(dismiss, SessionBrowserEvent::Dismissed);
    assert!(!v.visible);
}
