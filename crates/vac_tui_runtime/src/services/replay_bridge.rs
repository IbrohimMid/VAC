//! Bridge between [`crossterm::event::Event`] and [`RecordedInput`]
//! (PR-T18 wiring).
//!
//! The recorder module already defines the wire shape. This module is the
//! glue that lives between the live crossterm poll thread and the JSONL
//! recorder/replay files:
//!
//! - **Record tap**: `crossterm_event_to_recorded_input` extracts the
//!   user-visible subset of a crossterm event into a [`RecordedInput`].
//!   Backend-produced events (like a channel push) are filtered at the
//!   call site — the tap only runs on events fished out of
//!   `crossterm::event::read()`.
//! - **Replay driver**: `recorded_input_to_crossterm_event` re-synthesizes
//!   a [`crossterm::event::Event`] so replay can be fed through the exact
//!   same `map_crossterm_event_to_input_event` path that live input
//!   takes. This keeps keybindings overrides (PR-T19) and any other
//!   mapping logic on a single code path.
//!
//! Key-code encoding uses the short strings already used by
//!   `ACTION_SPECS`: `"Enter"`, `"F1"`, `"a"`, `"?"`, `"Up"`, etc. Single
//!   unicode chars round-trip through `KeyCode::Char(c)`.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

use crate::services::recorder::RecordedInput;

/// Extract the replayable subset of a crossterm [`Event`]. Returns `None`
/// for events we intentionally don't record (focus gain/loss, key
/// release/repeat, mouse moves without drag, etc.).
pub fn crossterm_event_to_recorded_input(event: &Event) -> Option<RecordedInput> {
    match event {
        Event::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return None;
            }
            let code = key_code_to_string(&key.code)?;
            Some(RecordedInput::Key {
                code,
                modifiers: key.modifiers.bits(),
            })
        }
        Event::Mouse(MouseEvent {
            kind, column, row, ..
        }) => match kind {
            MouseEventKind::Down(MouseButton::Left) => Some(RecordedInput::MouseDragStart {
                col: *column,
                row: *row,
            }),
            MouseEventKind::Drag(MouseButton::Left) => Some(RecordedInput::MouseDrag {
                col: *column,
                row: *row,
            }),
            MouseEventKind::Up(MouseButton::Left) => Some(RecordedInput::MouseDragEnd {
                col: *column,
                row: *row,
            }),
            _ => None,
        },
        Event::Resize(cols, rows) => Some(RecordedInput::Resize {
            cols: *cols,
            rows: *rows,
        }),
        Event::Paste(text) => Some(RecordedInput::Paste { text: text.clone() }),
        _ => None,
    }
}

/// Re-synthesize a crossterm [`Event`] from a recorded entry so it can be
/// replayed through the live input pipeline. Returns `None` if the
/// key-code string is not recognized.
pub fn recorded_input_to_crossterm_event(input: &RecordedInput) -> Option<Event> {
    match input {
        RecordedInput::Key { code, modifiers } => {
            let key_code = string_to_key_code(code)?;
            let key = KeyEvent::new(key_code, KeyModifiers::from_bits_truncate(*modifiers));
            Some(Event::Key(key))
        }
        RecordedInput::MouseDragStart { col, row } => Some(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: *col,
            row: *row,
            modifiers: KeyModifiers::NONE,
        })),
        RecordedInput::MouseDrag { col, row } => Some(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: *col,
            row: *row,
            modifiers: KeyModifiers::NONE,
        })),
        RecordedInput::MouseDragEnd { col, row } => Some(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: *col,
            row: *row,
            modifiers: KeyModifiers::NONE,
        })),
        RecordedInput::Resize { cols, rows } => Some(Event::Resize(*cols, *rows)),
        RecordedInput::Paste { text } => Some(Event::Paste(text.clone())),
    }
}

fn key_code_to_string(code: &KeyCode) -> Option<String> {
    Some(match code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::BackTab => "BackTab".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Insert => "Insert".into(),
        KeyCode::F(n) => format!("F{n}"),
        KeyCode::Null => "Null".into(),
        _ => return None,
    })
}

fn string_to_key_code(s: &str) -> Option<KeyCode> {
    // Single-char case: any unicode scalar becomes KeyCode::Char.
    let mut chars = s.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(KeyCode::Char(c));
    }
    Some(match s {
        "Enter" => KeyCode::Enter,
        "Tab" => KeyCode::Tab,
        "BackTab" => KeyCode::BackTab,
        "Esc" => KeyCode::Esc,
        "Up" => KeyCode::Up,
        "Down" => KeyCode::Down,
        "Left" => KeyCode::Left,
        "Right" => KeyCode::Right,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Insert" => KeyCode::Insert,
        "Null" => KeyCode::Null,
        s if s.starts_with('F') => {
            let n: u8 = s[1..].parse().ok()?;
            KeyCode::F(n)
        }
        _ => return None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn key_round_trips_through_bridge() {
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let recorded = crossterm_event_to_recorded_input(&Event::Key(key)).unwrap();
        let ev = recorded_input_to_crossterm_event(&recorded).unwrap();
        match ev {
            Event::Key(k) => {
                assert_eq!(k.code, KeyCode::Char('p'));
                assert!(k.modifiers.contains(KeyModifiers::CONTROL));
            }
            _ => panic!("expected Event::Key"),
        }
    }

    #[test]
    fn named_keys_round_trip() {
        for code in [
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Tab,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::PageUp,
            KeyCode::F(5),
        ] {
            let recorded = crossterm_event_to_recorded_input(&Event::Key(KeyEvent::new(
                code,
                KeyModifiers::NONE,
            )))
            .unwrap();
            let ev = recorded_input_to_crossterm_event(&recorded).unwrap();
            match ev {
                Event::Key(k) => assert_eq!(k.code, code, "round-trip lost {:?}", code),
                _ => panic!("expected Event::Key"),
            }
        }
    }

    #[test]
    fn key_release_is_dropped_on_record() {
        let mut key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert!(crossterm_event_to_recorded_input(&Event::Key(key)).is_none());
    }

    #[test]
    fn resize_and_paste_round_trip() {
        let r = crossterm_event_to_recorded_input(&Event::Resize(80, 24)).unwrap();
        assert_eq!(
            r,
            RecordedInput::Resize {
                cols: 80,
                rows: 24
            }
        );
        assert!(matches!(
            recorded_input_to_crossterm_event(&r).unwrap(),
            Event::Resize(80, 24)
        ));

        let p =
            crossterm_event_to_recorded_input(&Event::Paste("hi".to_string())).unwrap();
        assert_eq!(
            p,
            RecordedInput::Paste {
                text: "hi".to_string()
            }
        );
        match recorded_input_to_crossterm_event(&p).unwrap() {
            Event::Paste(t) => assert_eq!(t, "hi"),
            _ => panic!("expected paste"),
        }
    }

    #[test]
    fn mouse_drag_sequence_round_trips() {
        for (kind, expected) in [
            (
                MouseEventKind::Down(MouseButton::Left),
                RecordedInput::MouseDragStart { col: 3, row: 4 },
            ),
            (
                MouseEventKind::Drag(MouseButton::Left),
                RecordedInput::MouseDrag { col: 3, row: 4 },
            ),
            (
                MouseEventKind::Up(MouseButton::Left),
                RecordedInput::MouseDragEnd { col: 3, row: 4 },
            ),
        ] {
            let got = crossterm_event_to_recorded_input(&Event::Mouse(MouseEvent {
                kind,
                column: 3,
                row: 4,
                modifiers: KeyModifiers::NONE,
            }))
            .unwrap();
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn unknown_code_string_returns_none() {
        assert!(string_to_key_code("").is_none());
        assert!(string_to_key_code("NotAKey").is_none());
        // Single-char "F" is legitimately KeyCode::Char('F') — that's not
        // ambiguous with F-keys because F-keys always carry at least one
        // digit ("F1".., "F12"..).
        assert!(matches!(string_to_key_code("F"), Some(KeyCode::Char('F'))));
        // F-number parses as u8; overflow is rejected (we don't silently clamp).
        assert!(string_to_key_code("F999").is_none());
        assert!(matches!(string_to_key_code("F12"), Some(KeyCode::F(12))));
    }
}
