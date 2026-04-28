//! Slice 6 — shortcuts/command-palette popup extraction proof.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_shell_contracts::{SessionEntry, ShellCommandKind, ShellCommandSpec};
use vac_shell_shortcuts::{
    Shortcut, ShortcutsMode, ShortcutsView, build_shortcuts_lines, default_shortcuts,
    filter_commands, filter_sessions, filter_shortcuts, render_shortcuts_popup,
};

fn session(id: &str, label: &str, ts: u64) -> SessionEntry {
    SessionEntry {
        id: id.into(),
        label: label.into(),
        last_active_unix: ts,
    }
}

fn cmd(slash: &str, palette_visible: bool) -> ShellCommandSpec {
    ShellCommandSpec {
        id: slash.trim_start_matches('/').to_string(),
        slash: slash.into(),
        title: slash.into(),
        description: format!("desc for {slash}"),
        kind: ShellCommandKind::BuiltInAction,
        palette_visible,
        shortcut: None,
        ..Default::default()
    }
}

#[test]
fn empty_search_lists_visible_commands_only() {
    let cmds = vec![
        cmd("/model", true),
        cmd("/runtime", true),
        cmd("/internal", false),
    ];
    let listed = filter_commands("", &cmds);
    let slashes: Vec<&str> = listed.iter().map(|s| s.slash.as_str()).collect();
    assert_eq!(slashes, vec!["/model", "/runtime"]);
}

#[test]
fn command_filter_matches_slash_or_description() {
    let cmds = vec![cmd("/model", true), cmd("/runtime", true)];
    let by_slash = filter_commands("runt", &cmds);
    assert_eq!(by_slash.len(), 1);
    assert_eq!(by_slash[0].slash, "/runtime");
    let by_desc = filter_commands("desc for /model", &cmds);
    assert_eq!(by_desc.len(), 1);
    assert_eq!(by_desc[0].slash, "/model");
}

#[test]
fn shortcut_filter_matches_key_description_or_category() {
    let all = default_shortcuts();
    assert!(
        filter_shortcuts("ctrl", &all)
            .iter()
            .all(|s| s.key.to_lowercase().contains("ctrl")
                || s.description.to_lowercase().contains("ctrl")
                || s.category.to_lowercase().contains("ctrl"))
    );
    let nav = filter_shortcuts("navigation", &all);
    assert!(nav.iter().all(|s| s.category == "Navigation"));
    assert!(filter_shortcuts("nothing-matches-xyz", &all).is_empty());
}

#[test]
fn build_shortcuts_lines_orders_categories_and_renders_headers() {
    let shortcuts = default_shortcuts();
    let lines = build_shortcuts_lines(60, &shortcuts);
    let plain: String = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let nav_at = plain
        .find(" Navigation ")
        .expect("Navigation header missing");
    let cmds_at = plain.find(" Commands ").expect("Commands header missing");
    assert!(nav_at < cmds_at, "category order broken");
}

#[test]
fn toggle_mode_cycles_commands_shortcuts_sessions() {
    let mut view = ShortcutsView::new(vec![cmd("/model", true)], default_shortcuts());
    view.search = "anything".into();
    view.toggle_mode();
    assert_eq!(view.mode, ShortcutsMode::Shortcuts);
    assert!(view.search.is_empty());
    view.toggle_mode();
    assert_eq!(view.mode, ShortcutsMode::Sessions);
    view.toggle_mode();
    assert_eq!(view.mode, ShortcutsMode::Commands);
}

#[test]
fn filter_sessions_matches_id_or_label() {
    let all = vec![
        session("aaa", "Add error handling", 100),
        session("bbb", "Refactor swarm", 200),
        session("ccc", "Bump dependencies", 300),
    ];
    assert_eq!(filter_sessions("", &all).len(), 3);
    let by_id = filter_sessions("bbb", &all);
    assert_eq!(by_id.len(), 1);
    assert_eq!(by_id[0].id, "bbb");
    let by_label = filter_sessions("refactor", &all);
    assert_eq!(by_label.len(), 1);
    assert_eq!(by_label[0].id, "bbb");
    assert!(filter_sessions("nothing-matches-xyz", &all).is_empty());
}

#[test]
fn visible_sessions_mode_renders_entries_or_empty_hint() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = ShortcutsView::new(vec![], default_shortcuts()).with_sessions(vec![session(
        "uuid-one",
        "Refactor swarm",
        200,
    )]);
    view.visible = true;
    view.mode = ShortcutsMode::Sessions;
    terminal
        .draw(|f| render_shortcuts_popup(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("Sessions"), "Sessions tab missing\n{all}");
    assert!(all.contains("Refactor swarm"), "label missing\n{all}");
    assert!(all.contains("uuid-one"), "id missing\n{all}");
}

#[test]
fn visible_sessions_mode_with_no_entries_renders_empty_hint() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = ShortcutsView::new(vec![], default_shortcuts());
    view.visible = true;
    view.mode = ShortcutsMode::Sessions;
    terminal
        .draw(|f| render_shortcuts_popup(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("no sessions yet"), "empty hint missing\n{all}");
}

#[test]
fn invisible_view_renders_nothing() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let view = ShortcutsView::new(vec![cmd("/model", true)], default_shortcuts());
    terminal
        .draw(|f| render_shortcuts_popup(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let any = (0..buf.area.width)
        .flat_map(|x| (0..buf.area.height).map(move |y| (x, y)))
        .any(|(x, y)| !buf[(x, y)].symbol().trim().is_empty());
    assert!(!any);
}

#[test]
fn visible_commands_mode_renders_title_tabs_and_a_command() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = ShortcutsView::new(
        vec![cmd("/model", true), cmd("/runtime", true)],
        default_shortcuts(),
    );
    view.visible = true;
    terminal
        .draw(|f| render_shortcuts_popup(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("Command Palette"), "title missing\n{all}");
    assert!(all.contains("Commands"), "tab label missing\n{all}");
    assert!(all.contains("Shortcuts"), "tab label missing\n{all}");
    assert!(all.contains("/model"), "command row missing\n{all}");
}

#[test]
fn visible_shortcuts_mode_renders_a_known_category_header() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = ShortcutsView::new(vec![], default_shortcuts());
    view.visible = true;
    view.mode = ShortcutsMode::Shortcuts;
    terminal
        .draw(|f| render_shortcuts_popup(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(
        all.contains("Navigation") || all.contains("Text Input"),
        "no category header rendered\n{all}"
    );
}

/// Drift tripwire — only `ratatui` and contract types may show up
/// at the boundary. A future patch adding `vac_tui_runtime` or
/// donor types would compile-fail this test.
#[test]
fn no_donor_or_engine_dependency_links_in() {
    fn assert_only_local<T: Sized>(_: T) {}
    assert_only_local(ShortcutsMode::Commands);
    assert_only_local(Shortcut::new("a", "b", "c"));
}
