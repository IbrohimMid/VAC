//! Runtime keymap override layer (PR-T19 wiring).
//!
//! Reads the `ActionId -> Vec<chord>` map produced by
//! [`crate::services::keybindings_loader::resolve_effective`] and exposes a
//! chord string -> [`InputEvent`] lookup that `event.rs` consults *before*
//! the hard-coded key match. User-configured bindings therefore take
//! precedence; unset actions fall through to compiled-in defaults.
//!
//! Only a subset of [`ActionId`] is keyboard-reachable (many are slash-only
//! or workbench-contextual). For those entries we return `None` from
//! [`action_id_to_input_event`] and simply ignore the override.
//!
//! ## Chord string format
//!
//! Matches the format used in `ACTION_SPECS`:
//!     - Single char:       "a", "?", "x"
//!     - Named key:         "Enter", "Esc", "Tab", "F1", "Up", "Down", "Left",
//!                            "Right", "Home", "End", "PageUp", "PageDown",
//!                            "Backspace", "Delete", "Space"
//!     - With modifiers:    "Ctrl+P", "Alt+T", "Ctrl+Shift+P"
//!     - Repeated:          "Ctrl+C×2" (the override layer does *not* handle
//!                            double-press detection; see `AttemptQuit` logic.
//!                            Entries containing '×' are intentionally skipped.)

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::action_registry::ActionId;
use crate::app::InputEvent;

/// Canonical encoding of a [`KeyEvent`] into the same chord string format
/// used by `ACTION_SPECS`. Returns `None` for release/repeat events or
/// non-text keys that have no stable string form.
pub fn key_event_to_chord(key: &KeyEvent) -> Option<String> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    let mut parts: Vec<&str> = Vec::with_capacity(4);
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt");
    }
    // SHIFT is only explicit for non-char keys; for Char variants crossterm
    // already delivers the upper-case char, and the chord string is that
    // upper-case char (matches ACTION_SPECS bindings like "A" = approve_all).
    let include_shift_explicit = matches!(
        key.code,
        KeyCode::Enter
            | KeyCode::Tab
            | KeyCode::Esc
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::F(_)
    );
    if include_shift_explicit && key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift");
    }

    let base: String = match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        _ => return None,
    };
    parts.push(&base);
    Some(parts.join("+"))
}

/// Map a keyboard-reachable [`ActionId`] to the [`InputEvent`] it should
/// dispatch. Returns `None` for slash-only or workbench-contextual actions
/// that are not intended to be rebindable via a single global chord.
pub fn action_id_to_input_event(id: ActionId) -> Option<InputEvent> {
    Some(match id {
        ActionId::Quit => InputEvent::AttemptQuit,
        ActionId::OpenCommandPalette => InputEvent::ShowCommandPalette,
        ActionId::OpenShortcuts => InputEvent::ShowShortcuts,
        ActionId::OpenFileSearch => InputEvent::ShowFileSearch,
        ActionId::SwitchModel => InputEvent::ShowModelSwitcher,
        ActionId::SwitchProfile => InputEvent::ShowProfileSwitcher,
        ActionId::SwitchIsolation => InputEvent::ShowIsolationSwitcher,
        ActionId::SwitchRulebook => InputEvent::ShowRulebookSwitcher,
        ActionId::CyclePane => InputEvent::Tab,
        ActionId::CycleWorkbenchTab => InputEvent::WorkbenchNextTab,
        ActionId::ToggleSidePanel => InputEvent::ToggleSidePanel,
        ActionId::ToggleAutoApprove => InputEvent::ToggleAutoApprove,
        ActionId::CancelStream => InputEvent::HandleEsc,
        ActionId::ApproveAll => InputEvent::ApproveAll,
        ActionId::RejectAll => InputEvent::RejectAll,
        ActionId::ApproveCurrent => InputEvent::AutoApproveCurrentTool,
        ActionId::RejectCurrent => InputEvent::RejectCurrentTool,
        ActionId::NewSession => InputEvent::NewSession,
        ActionId::ReviewOpen => InputEvent::ReviewOpen,
        // Slash-command actions + workbench-contextual actions are not
        // rebindable through this layer. Their dispatch still happens via
        // `input_commands::dispatch_action` or workbench input handlers.
        _ => return None,
    })
}

/// Read-only chord→ActionId table built from the user's effective
/// keybindings. [`InputEvent`] is *not* `Clone` (variants carry channels,
/// tool calls, etc.), so we store the [`ActionId`] and re-materialize the
/// event on every lookup via [`action_id_to_input_event`].
#[derive(Debug, Default, Clone)]
pub struct ChordKeymap {
    chord_to_action: HashMap<String, ActionId>,
    /// Bindings that we accepted and will honour. Useful for tests and for
    /// a future `:shortcuts` overlay that wants to show the resolved map.
    accepted: Vec<(String, ActionId)>,
    /// Bindings we silently dropped because the ActionId is not
    /// keyboard-reachable. Surfaced for diagnostics, not errors.
    skipped_non_reachable: Vec<(String, ActionId)>,
    /// Chord strings bound to more than one ActionId in the user's
    /// effective map. Last-writer wins for dispatch, but we keep the full
    /// list so the UI can warn the user that one of their bindings is
    /// shadowing another (PR-T19 R1).
    conflicts: Vec<(String, Vec<ActionId>)>,
}

impl ChordKeymap {
    /// Build a keymap from `resolve_effective`'s output. Unreachable
    /// actions are recorded for diagnostics but never inserted into the
    /// lookup table.
    pub fn from_effective(effective: &HashMap<ActionId, Vec<String>>) -> Self {
        let mut map = ChordKeymap::default();
        // Collect all (chord, id) pairs first so we can detect duplicate
        // chord bindings before the last-writer-wins `HashMap::insert`
        // hides them. We sort by ActionId enum order for deterministic
        // conflict-reporting output (important for test snapshots).
        let mut staged: Vec<(String, ActionId)> = Vec::new();
        for (&id, chords) in effective {
            if action_id_to_input_event(id).is_none() {
                for c in chords {
                    map.skipped_non_reachable.push((c.clone(), id));
                }
                continue;
            }
            for chord in chords {
                // Double-press chords stay on the hard-coded path.
                if chord.contains('\u{00d7}') {
                    continue;
                }
                staged.push((chord.clone(), id));
            }
        }
        staged.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| (a.1 as u32).cmp(&(b.1 as u32))));
        let mut by_chord: HashMap<String, Vec<ActionId>> = HashMap::new();
        for (chord, id) in &staged {
            by_chord.entry(chord.clone()).or_default().push(*id);
        }
        for (chord, ids) in &by_chord {
            if ids.len() > 1 {
                map.conflicts.push((chord.clone(), ids.clone()));
            }
        }
        map.conflicts.sort_by(|a, b| a.0.cmp(&b.0));
        for (chord, id) in staged {
            map.chord_to_action.insert(chord.clone(), id);
            map.accepted.push((chord, id));
        }
        map
    }

    /// Look up the chord produced by a live [`KeyEvent`]. Returns `None`
    /// when the user has no override for this chord (the caller should
    /// then fall through to the compiled-in defaults).
    pub fn lookup(&self, key: &KeyEvent) -> Option<InputEvent> {
        let chord = key_event_to_chord(key)?;
        let id = *self.chord_to_action.get(&chord)?;
        action_id_to_input_event(id)
    }

    pub fn accepted_bindings(&self) -> &[(String, ActionId)] {
        &self.accepted
    }

    pub fn skipped_bindings(&self) -> &[(String, ActionId)] {
        &self.skipped_non_reachable
    }

    /// Chord strings that the user bound to more than one ActionId. The UI
    /// surfaces these so the user knows exactly which other action is
    /// shadowing theirs (PR-T19 R1). Each entry lists every ActionId that
    /// claimed the chord; `chord_to_action` retains whichever arrived last
    /// after the stable sort in `from_effective`.
    pub fn conflicts(&self) -> &[(String, Vec<ActionId>)] {
        &self.conflicts
    }

    pub fn is_empty(&self) -> bool {
        self.chord_to_action.is_empty()
    }
}

// ---- Global single-instance wiring -----------------------------------------
//
// PR-T19 R3: the global keymap is now stored behind an `RwLock<Option<Arc<..>>>`
// so it can be swapped at runtime when `.vac/keybindings.toml` changes on disk.
// `install_global_keymap` preserves its first-write-wins semantics for
// backward compatibility; `reload_global_keymap` is the reload entry point
// and always overwrites the slot.

fn global_keymap_slot() -> &'static RwLock<Option<Arc<ChordKeymap>>> {
    static SLOT: OnceLock<RwLock<Option<Arc<ChordKeymap>>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Install the process-wide keymap. Only the first call wins; subsequent
/// calls are ignored (returns `false`). Intended to be called exactly once
/// from TUI startup. Later updates should use [`reload_global_keymap`].
pub fn install_global_keymap(keymap: ChordKeymap) -> bool {
    match global_keymap_slot().write() {
        Ok(mut guard) => {
            if guard.is_some() {
                false
            } else {
                *guard = Some(Arc::new(keymap));
                true
            }
        }
        Err(_) => false,
    }
}

/// Atomically replace the process-wide keymap. Always overwrites the slot,
/// even if one was previously installed. Used by the `.vac/keybindings.toml`
/// hot-reload path (PR-T19 R3).
pub fn reload_global_keymap(keymap: ChordKeymap) {
    if let Ok(mut guard) = global_keymap_slot().write() {
        *guard = Some(Arc::new(keymap));
    }
}

/// Snapshot the currently installed global keymap, if any. Returned as an
/// `Arc` so callers can drop the read lock immediately.
pub fn current_global_keymap() -> Option<Arc<ChordKeymap>> {
    global_keymap_slot().read().ok()?.clone()
}

/// Look up an override for the given key event against the installed
/// global keymap. Returns `None` if no keymap was installed or there is
/// no override for this chord.
pub fn lookup_override_global(key: &KeyEvent) -> Option<InputEvent> {
    let snapshot = current_global_keymap()?;
    snapshot.lookup(key)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn encodes_plain_chars() {
        assert_eq!(
            key_event_to_chord(&press(KeyCode::Char('a'), KeyModifiers::NONE)).unwrap(),
            "a"
        );
        assert_eq!(
            key_event_to_chord(&press(KeyCode::Char('?'), KeyModifiers::NONE)).unwrap(),
            "?"
        );
        assert_eq!(
            key_event_to_chord(&press(KeyCode::Char('A'), KeyModifiers::NONE)).unwrap(),
            "A",
            "upper-case char should not add explicit Shift"
        );
    }

    #[test]
    fn encodes_modifiers_in_stable_order() {
        assert_eq!(
            key_event_to_chord(&press(KeyCode::Char('p'), KeyModifiers::CONTROL)).unwrap(),
            "Ctrl+p"
        );
        assert_eq!(
            key_event_to_chord(&press(KeyCode::Char('t'), KeyModifiers::ALT)).unwrap(),
            "Alt+t"
        );
        assert_eq!(
            key_event_to_chord(&press(
                KeyCode::Char('p'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            ))
            .unwrap(),
            "Ctrl+Alt+p"
        );
    }

    #[test]
    fn encodes_named_keys() {
        assert_eq!(
            key_event_to_chord(&press(KeyCode::Enter, KeyModifiers::NONE)).unwrap(),
            "Enter"
        );
        assert_eq!(
            key_event_to_chord(&press(KeyCode::F(1), KeyModifiers::NONE)).unwrap(),
            "F1"
        );
        assert_eq!(
            key_event_to_chord(&press(KeyCode::PageUp, KeyModifiers::NONE)).unwrap(),
            "PageUp"
        );
    }

    #[test]
    fn shift_on_named_key_is_explicit() {
        assert_eq!(
            key_event_to_chord(&press(KeyCode::Tab, KeyModifiers::SHIFT)).unwrap(),
            "Shift+Tab"
        );
    }

    #[test]
    fn release_events_are_rejected() {
        let mut ev = press(KeyCode::Char('a'), KeyModifiers::NONE);
        ev.kind = KeyEventKind::Release;
        assert!(key_event_to_chord(&ev).is_none());
    }

    #[test]
    fn subset_mapping_covers_expected_action_ids() {
        // Spot-check: the actions we *expect* to be rebindable must round-trip.
        assert!(action_id_to_input_event(ActionId::OpenShortcuts).is_some());
        assert!(action_id_to_input_event(ActionId::OpenFileSearch).is_some());
        assert!(action_id_to_input_event(ActionId::Quit).is_some());
        assert!(action_id_to_input_event(ActionId::ToggleAutoApprove).is_some());
        // Slash-only action is not in the subset.
        assert!(action_id_to_input_event(ActionId::Shell).is_none());
    }

    #[test]
    fn keymap_override_wins_for_reachable_action() {
        let mut effective: HashMap<ActionId, Vec<String>> = HashMap::new();
        effective.insert(ActionId::OpenShortcuts, vec!["F1".to_string()]);
        let map = ChordKeymap::from_effective(&effective);
        let ev = map
            .lookup(&press(KeyCode::F(1), KeyModifiers::NONE))
            .expect("F1 must resolve to ShowShortcuts");
        assert!(matches!(ev, InputEvent::ShowShortcuts));
        assert_eq!(map.accepted_bindings().len(), 1);
    }

    #[test]
    fn keymap_ignores_unreachable_action() {
        let mut effective: HashMap<ActionId, Vec<String>> = HashMap::new();
        effective.insert(ActionId::Shell, vec!["Ctrl+Shift+S".to_string()]);
        let map = ChordKeymap::from_effective(&effective);
        assert!(
            map.is_empty(),
            "unreachable actions must not populate the chord table"
        );
        assert_eq!(map.skipped_bindings().len(), 1);
    }

    #[test]
    fn keymap_skips_double_press_chords() {
        let mut effective: HashMap<ActionId, Vec<String>> = HashMap::new();
        effective.insert(ActionId::Quit, vec!["Ctrl+C\u{00d7}2".to_string()]);
        let map = ChordKeymap::from_effective(&effective);
        assert!(
            map.lookup(&press(KeyCode::Char('c'), KeyModifiers::CONTROL))
                .is_none(),
            "double-press chord must stay on the hard-coded path"
        );
    }

    #[test]
    fn keymap_respects_empty_override_vec() {
        // Empty vec means "user cleared the binding"; nothing should be
        // inserted into the override table for that action.
        let mut effective: HashMap<ActionId, Vec<String>> = HashMap::new();
        effective.insert(ActionId::OpenShortcuts, Vec::new());
        let map = ChordKeymap::from_effective(&effective);
        assert!(map.is_empty());
    }

    #[test]
    fn keymap_multiple_chords_for_same_action() {
        let mut effective: HashMap<ActionId, Vec<String>> = HashMap::new();
        effective.insert(
            ActionId::OpenFileSearch,
            vec!["Ctrl+p".to_string(), "Ctrl+t".to_string()],
        );
        let map = ChordKeymap::from_effective(&effective);
        assert!(
            map.lookup(&press(KeyCode::Char('p'), KeyModifiers::CONTROL))
                .is_some()
        );
        assert!(
            map.lookup(&press(KeyCode::Char('t'), KeyModifiers::CONTROL))
                .is_some()
        );
    }
}
