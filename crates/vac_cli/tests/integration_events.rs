//! Per-PR Bar gate file for `cargo test -p vac_cli --test integration_events`.
//!
//! Scope (narrow on purpose):
//!   * event emission / dispatch contract
//!   * argument → event mapping (ActionId → InputEvent)
//!   * overlay / workbench / runtime event triggers that are stable today
//!   * invariants relevant to plan acceptance (keybinding override, recorder
//!     round-trip, diagnostics-overlay cache, mouse dispatch contract)
//!
//! Not in scope: full UI flow — those live in `tui_flows.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use tempfile::TempDir;

use vac_tui_runtime::action_ids::ActionId;
use vac_tui_runtime::app::{InputEvent, WorkbenchTab, WorkspaceFocus};
use vac_tui_runtime::handlers::mouse::dispatch_click;
use vac_tui_runtime::services::diagnostics_overlay::DiagnosticsOverlayCache;
use vac_tui_runtime::services::keybindings_runtime::{
    ChordKeymap, action_id_to_input_event, key_event_to_chord,
};
use vac_tui_runtime::services::recorder::{
    RecordedInput, RecordedLine, Recorder, RecorderConfig, Replay,
};

use support::{drain_output, effective, fresh_state, key, rect};

// =============================================================================
// 1. Mouse dispatch contract (T16)
//
// Smoke the event-level invariant: a click inside a tracked tab region must
// (a) be consumed, (b) mutate the documented state fields, and (c) not emit
// a spurious OutputEvent. Full per-surface coverage lives in `tui_flows`.
// =============================================================================

#[test]
fn mouse_dispatch_tab_click_is_pure_state_transition() {
    let (mut state, tx, mut rx) = fresh_state();
    state.layout.focus = WorkspaceFocus::Input;
    state.layout.workbench_tab = WorkbenchTab::Sessions;
    state
        .layout
        .workbench_chrome
        .tab_regions
        .push((WorkbenchTab::Review, rect(0, 0, 10, 1)));

    let handled = dispatch_click(&mut state, &tx, 3, 0);
    assert!(handled, "tab-region click must be consumed");
    assert_eq!(state.layout.workbench_tab, WorkbenchTab::Review);
    assert_eq!(state.layout.focus, WorkspaceFocus::Workbench);

    let emitted = drain_output(&mut rx);
    assert!(
        emitted.is_empty(),
        "tab-switch click must be a pure state transition, no OutputEvents"
    );
}

#[test]
fn mouse_dispatch_ignores_click_outside_every_region() {
    let (mut state, tx, mut rx) = fresh_state();
    state.layout.focus = WorkspaceFocus::Input;
    // No regions seeded.
    let handled = dispatch_click(&mut state, &tx, 999, 999);
    assert!(!handled);
    assert_eq!(state.layout.focus, WorkspaceFocus::Input);
    assert!(drain_output(&mut rx).is_empty());
}

// =============================================================================
// 2. ActionId → InputEvent mapping (T19 contract surface)
//
// The keybinding override layer reaches the input pipeline only for ActionIds
// for which `action_id_to_input_event` returns `Some`. If this mapping shrinks
// silently, every user-configured chord for those IDs becomes a dead binding.
// =============================================================================

type ActionMatcher = fn(&InputEvent) -> bool;
type ReachableAction = (ActionId, ActionMatcher);

const REACHABLE_ACTIONS: &[ReachableAction] = &[
    (ActionId::Quit, is_attempt_quit),
    (ActionId::OpenCommandPalette, is_show_command_palette),
    (ActionId::OpenShortcuts, is_show_shortcuts),
    (ActionId::OpenFileSearch, is_show_file_search),
    (ActionId::CyclePane, is_tab),
    (ActionId::CycleWorkbenchTab, is_workbench_next_tab),
    (ActionId::ToggleSidePanel, is_toggle_side_panel),
    (ActionId::ToggleAutoApprove, is_toggle_auto_approve),
    (ActionId::CancelStream, is_handle_esc),
    (ActionId::ApproveAll, is_approve_all),
    (ActionId::RejectAll, is_reject_all),
    (ActionId::NewSession, is_new_session),
    (ActionId::ReviewOpen, is_review_open),
];

fn is_attempt_quit(e: &InputEvent) -> bool {
    matches!(e, InputEvent::AttemptQuit)
}
fn is_show_command_palette(e: &InputEvent) -> bool {
    matches!(e, InputEvent::ShowCommandPalette)
}
fn is_show_shortcuts(e: &InputEvent) -> bool {
    matches!(e, InputEvent::ShowShortcuts)
}
fn is_show_file_search(e: &InputEvent) -> bool {
    matches!(e, InputEvent::ShowFileSearch)
}
fn is_tab(e: &InputEvent) -> bool {
    matches!(e, InputEvent::Tab)
}
fn is_workbench_next_tab(e: &InputEvent) -> bool {
    matches!(e, InputEvent::WorkbenchNextTab)
}
fn is_toggle_side_panel(e: &InputEvent) -> bool {
    matches!(e, InputEvent::ToggleSidePanel)
}
fn is_toggle_auto_approve(e: &InputEvent) -> bool {
    matches!(e, InputEvent::ToggleAutoApprove)
}
fn is_handle_esc(e: &InputEvent) -> bool {
    matches!(e, InputEvent::HandleEsc)
}
fn is_approve_all(e: &InputEvent) -> bool {
    matches!(e, InputEvent::ApproveAll)
}
fn is_reject_all(e: &InputEvent) -> bool {
    matches!(e, InputEvent::RejectAll)
}
fn is_new_session(e: &InputEvent) -> bool {
    matches!(e, InputEvent::NewSession)
}
fn is_review_open(e: &InputEvent) -> bool {
    matches!(e, InputEvent::ReviewOpen)
}

#[test]
fn action_id_to_input_event_covers_every_reachable_action() {
    for (id, check) in REACHABLE_ACTIONS {
        let ev = action_id_to_input_event(*id)
            .unwrap_or_else(|| panic!("{id:?} lost its reachable mapping"));
        assert!(
            check(&ev),
            "{id:?} now maps to a different InputEvent variant: {ev:?}"
        );
    }
}

#[test]
fn action_id_to_input_event_leaves_slash_actions_unreachable() {
    for id in [
        ActionId::Clear,
        ActionId::Sessions,
        ActionId::Shell,
        ActionId::Export,
    ] {
        assert!(
            action_id_to_input_event(id).is_none(),
            "{id:?} must not be keyboard-reachable via the override layer"
        );
    }
}

// =============================================================================
// 3. Keybinding override chord resolution (T19 wiring)
//
// `ChordKeymap::from_effective` + `lookup` is what the runtime consults on
// every key press. A user config that rebinds `Ctrl+P → OpenShortcuts`
// must resolve to `InputEvent::ShowShortcuts`.
// =============================================================================

#[test]
fn chord_keymap_maps_user_chord_to_input_event() {
    let ctrl_p = key_event_to_chord(&key(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .expect("Ctrl+P must have a canonical chord encoding");
    assert_eq!(ctrl_p, "Ctrl+p", "chord encoding is the plan's contract");

    let keymap = ChordKeymap::from_effective(&effective([
        (ActionId::OpenShortcuts, vec!["Ctrl+p"]),
        (ActionId::Quit, vec!["Ctrl+q"]),
    ]));

    let ev = keymap
        .lookup(&key(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .expect("Ctrl+P override must resolve to an InputEvent");
    assert!(matches!(ev, InputEvent::ShowShortcuts));

    let ev = keymap
        .lookup(&key(KeyCode::Char('q'), KeyModifiers::CONTROL))
        .expect("Ctrl+Q override must resolve to an InputEvent");
    assert!(matches!(ev, InputEvent::AttemptQuit));

    // Non-overridden chord falls through (runtime then uses compiled defaults).
    assert!(
        keymap
            .lookup(&key(KeyCode::Char('z'), KeyModifiers::CONTROL))
            .is_none()
    );
}

#[test]
fn chord_keymap_records_skipped_non_reachable_actions() {
    let keymap = ChordKeymap::from_effective(&effective([
        // Slash-command action — not keyboard-reachable, must be skipped.
        (ActionId::Shell, vec!["Ctrl+s"]),
    ]));
    assert!(keymap.is_empty());
    assert_eq!(keymap.skipped_bindings().len(), 1);
    assert_eq!(keymap.accepted_bindings().len(), 0);
}

#[test]
fn chord_keymap_detects_chord_bound_to_multiple_actions() {
    // User bound `Ctrl+p` to both OpenShortcuts and OpenFileSearch — only
    // one can win at dispatch time, but the UI must be able to warn the
    // user that the other is being shadowed (PR-T19 R1).
    let keymap = ChordKeymap::from_effective(&effective([
        (ActionId::OpenShortcuts, vec!["Ctrl+p"]),
        (ActionId::OpenFileSearch, vec!["Ctrl+p"]),
    ]));
    let conflicts = keymap.conflicts();
    assert_eq!(
        conflicts.len(),
        1,
        "exactly one chord should be in conflict"
    );
    let (chord, ids) = &conflicts[0];
    assert_eq!(chord, "Ctrl+p");
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&ActionId::OpenShortcuts));
    assert!(ids.contains(&ActionId::OpenFileSearch));
    // Non-conflicting unique chord must not show up in `conflicts()`.
    let single =
        ChordKeymap::from_effective(&effective([(ActionId::OpenShortcuts, vec!["Ctrl+p"])]));
    assert!(single.conflicts().is_empty());
}

// =============================================================================
// 4. Diagnostics-overlay cache invariants (T15 contract)
//
// The renderer integration depends on `DiagnosticsCache` behaving as a
// per-file cache: setting lines, reading them back, and being cleared when
// the active file changes.
// =============================================================================

#[test]
fn diagnostics_cache_roundtrips_per_line_spans() {
    let mut cache = DiagnosticsOverlayCache::default();
    assert!(cache.is_empty());
    assert!(cache.active_file().is_none());

    let file_a = PathBuf::from("src/a.rs");
    let changed = cache.clear_if_file_changed(Some(&file_a));
    assert!(changed, "first file must count as a change");
    assert_eq!(cache.active_file(), Some(file_a.as_path()));

    cache.set_line(3, vec![]);
    cache.set_line(7, vec![]);
    assert_eq!(cache.len(), 2);
    assert!(cache.cached_line(3).is_some());
    assert!(cache.cached_line(99).is_none());

    // Same file → no clear.
    let changed = cache.clear_if_file_changed(Some(&file_a));
    assert!(!changed);
    assert_eq!(cache.len(), 2);

    // Different file → cache drops.
    let file_b = PathBuf::from("src/b.rs");
    let changed = cache.clear_if_file_changed(Some(&file_b));
    assert!(changed);
    assert!(cache.is_empty());
    assert_eq!(cache.active_file(), Some(file_b.as_path()));
}

// =============================================================================
// 5. Recorder → Replay round-trip (T18 contract)
//
// The CLI `--replay` path depends on the JSONL stream surviving a write/read
// cycle byte-for-byte, including timestamps and the `RecordedInput` tag.
// =============================================================================

#[test]
fn recorder_replay_roundtrip_preserves_event_stream() {
    let tmp = TempDir::new().unwrap();
    let cfg = RecorderConfig::new(tmp.path().join("recordings"));
    let mut rec = Recorder::open(cfg).unwrap();

    let events: Vec<(RecordedInput, u128)> = vec![
        (
            RecordedInput::Key {
                code: "Enter".into(),
                modifiers: 0,
            },
            10,
        ),
        (RecordedInput::MouseDragStart { col: 4, row: 9 }, 20),
        (
            RecordedInput::Paste {
                text: "hello".into(),
            },
            30,
        ),
    ];
    for (ev, ts) in &events {
        rec.record_at(ev.clone(), *ts).unwrap();
    }
    let path = rec.current_path().to_path_buf();
    rec.flush().unwrap();
    drop(rec);

    let replayed: Vec<RecordedLine> = Replay::open(&path).unwrap().collect();
    assert_eq!(replayed.len(), events.len());
    for (i, line) in replayed.iter().enumerate() {
        assert_eq!(line.ts_ms, events[i].1, "timestamp drift at {i}");
        assert_eq!(line.event, events[i].0, "event drift at {i}");
    }
}
