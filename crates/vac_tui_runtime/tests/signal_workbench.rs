//! L3 — Signal workbench tab render smoke tests.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_tui_runtime::app::{AppState, WorkbenchTab};
use vac_tui_runtime::workbench::{SignalTab, WorkbenchTabView, tab_from_index, active_tab_index};

#[test]
fn contract_tab_index_roundtrips() {
    assert_eq!(active_tab_index(&WorkbenchTab::Signal), 8);
    assert_eq!(tab_from_index(8), WorkbenchTab::Signal);
}

#[test]
fn contract_next_tab_cycles_through_signal() {
    // Signal is inserted after Vwfd in the cycle order.
    assert_eq!(WorkbenchTab::Vwfd.next(), WorkbenchTab::Signal);
    assert_eq!(WorkbenchTab::Signal.next(), WorkbenchTab::Approvals);
}

#[test]
fn contract_label_reflects_stream_count() {
    let mut state = AppState::default();
    // Default state registers vil_dev (+ 0 shell, 0 mcp, 0 runtime) = 1.
    assert_eq!(SignalTab::tab_label(&state), "Signal (1)");

    state.shell.session_store.push_new("shell-a".to_string());
    // Registry now has vil_dev + 1 shell = 2.
    assert_eq!(SignalTab::tab_label(&state), "Signal (2)");
}

#[test]
fn contract_render_does_not_panic_on_empty_state() {
    let mut state = AppState::default();
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let area = f.area();
            SignalTab::render(f, &mut state, area);
        })
        .unwrap();
}

#[test]
fn contract_render_with_mixed_streams() {
    let mut state = AppState::default();
    state.vil_dev.output.push_line("WARN: slow");
    state.vil_dev.output.push_line("Error: boom");
    state.shell.session_store.push_new("shell-1".to_string());
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let area = f.area();
            SignalTab::render(f, &mut state, area);
        })
        .unwrap();
}
