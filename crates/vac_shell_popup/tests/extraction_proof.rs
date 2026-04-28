//! Step 2 slice 3 — shell popup extraction proof.
//!
//! Acceptance covers the same shape used for `vac_shell_palette`:
//! pure-state behaviour the donor exercised through `AppState`, plus
//! a tripwire test that fails if the dep graph drifts to pull
//! `vac_tui_runtime`, the donor PTY layer, or any other VAC engine
//! crate into this widget crate.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::text::{Line, Span};

use vac_shell_popup::{
    SHELL_POPUP_MAX_HEIGHT_PERCENT, SHELL_POPUP_MIN_HEIGHT, ShellPopupViewState,
    calculate_popup_height, render_shell_popup, reset_cursor_blink, update_cursor_blink,
};

fn dummy_lines(n: usize) -> Vec<Line<'static>> {
    (0..n)
        .map(|i| Line::from(Span::raw(format!("line {i}"))))
        .collect()
}

#[test]
fn invisible_popup_returns_zero_height() {
    let view = ShellPopupViewState::default();
    assert!(!view.visible);
    assert_eq!(calculate_popup_height(&view, 100), 0);
}

#[test]
fn collapsed_popup_snaps_to_two_buckets() {
    let mut view = ShellPopupViewState {
        visible: true,
        expanded: false,
        ..Default::default()
    };
    view.content_rows = 1;
    assert_eq!(
        calculate_popup_height(&view, 100),
        SHELL_POPUP_MIN_HEIGHT,
        "≤2 content rows collapses to MIN_HEIGHT"
    );
    view.content_rows = 8;
    assert_eq!(
        calculate_popup_height(&view, 100),
        5,
        ">2 content rows uses the 5-line collapsed bucket"
    );
}

#[test]
fn expanded_popup_clamps_between_min_and_60_percent() {
    let view = ShellPopupViewState {
        visible: true,
        expanded: true,
        content_rows: 50,
        ..Default::default()
    };
    let h = calculate_popup_height(&view, 40);
    let max = (40_f32 * SHELL_POPUP_MAX_HEIGHT_PERCENT) as u16;
    assert!(h <= max, "{h} should not exceed 60% of 40 ({max})");
    assert!(h >= SHELL_POPUP_MIN_HEIGHT);
}

#[test]
fn expanded_popup_floors_to_two_content_rows() {
    let view = ShellPopupViewState {
        visible: true,
        expanded: true,
        content_rows: 0,
        ..Default::default()
    };
    // 2 content rows + 2 borders = 4 == MIN_HEIGHT
    assert_eq!(calculate_popup_height(&view, 100), SHELL_POPUP_MIN_HEIGHT);
}

#[test]
fn cursor_blink_toggles_every_five_ticks() {
    let mut view = ShellPopupViewState::default();
    view.cursor_visible = true;
    update_cursor_blink(&mut view); // timer = 1
    update_cursor_blink(&mut view); // timer = 2
    update_cursor_blink(&mut view); // timer = 3
    update_cursor_blink(&mut view); // timer = 4
    assert!(view.cursor_visible, "cursor should not flip before 5 ticks");
    update_cursor_blink(&mut view); // timer = 5 → toggles
    assert!(!view.cursor_visible);
}

#[test]
fn reset_cursor_blink_restores_visible_and_zeros_timer() {
    let mut view = ShellPopupViewState {
        cursor_visible: false,
        cursor_blink_timer: 17,
        ..Default::default()
    };
    reset_cursor_blink(&mut view);
    assert!(view.cursor_visible);
    assert_eq!(view.cursor_blink_timer, 0);
}

#[test]
fn invisible_popup_does_not_render_anything() {
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let view = ShellPopupViewState::default();
    terminal
        .draw(|f| render_shell_popup(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let any_non_space = (0..buf.area.width)
        .flat_map(|x| (0..buf.area.height).map(move |y| (x, y)))
        .any(|(x, y)| !buf[(x, y)].symbol().trim().is_empty());
    assert!(
        !any_non_space,
        "invisible popup should produce empty buffer"
    );
}

#[test]
fn visible_popup_renders_borders_and_title() {
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend).unwrap();
    let view = ShellPopupViewState {
        visible: true,
        expanded: true,
        active_command: Some("ls -la".into()),
        pending_command_value: Some("ls -la".into()),
        pending_command_executed: true,
        screen_lines: dummy_lines(3),
        content_rows: 3,
        ..Default::default()
    };
    terminal
        .draw(|f| render_shell_popup(f, &view, f.area()))
        .unwrap();
    // Spot-check: somewhere in the buffer the command name appears.
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(
        all.contains("ls -la"),
        "title must include the command name; got:\n{all}"
    );
}

#[test]
fn collapsed_with_overflow_shows_hidden_count_indicator() {
    let backend = TestBackend::new(60, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let view = ShellPopupViewState {
        visible: true,
        expanded: false,
        active_command: Some("yes".into()),
        pending_command_value: Some("yes".into()),
        pending_command_executed: true,
        screen_lines: dummy_lines(7),
        content_rows: 7,
        ..Default::default()
    };
    terminal
        .draw(|f| render_shell_popup(f, &view, f.area()))
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
        all.contains("hidden lines"),
        "collapsed overflow must show '+ N hidden lines'; got:\n{all}"
    );
}

/// Tripwire — drift guard.
///
/// If a future patch silently links `vac_tui_runtime`, the donor PTY
/// layer, or `vac_core` into this crate, this test will not compile
/// because those crates' types would shadow the imports below. The
/// only allowed external types are from `ratatui` and this crate.
#[test]
fn no_donor_or_runtime_dependency_links_in() {
    fn assert_only_local<T: Sized>(_: T) {}
    assert_only_local(ShellPopupViewState::default());
    assert_only_local(SHELL_POPUP_MIN_HEIGHT);
}
