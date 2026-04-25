//! Step 3a — palette widget extracted from Stakpak.
//!
//! # Provenance
//!
//! Source: `vendor/stakpak/tui/src/services/helper_dropdown.rs`
//! at commit `2e75bd56970d114ab41653aed045300b9a3257a7` (Apache 2.0).
//! Render layout (visible-rows window, scroll arrows, position counter)
//! is preserved verbatim. The state surface is replaced: where the
//! donor reaches into `AppState.input_state.{filtered_helpers,
//! show_helper_dropdown, helper_scroll, helper_selected}`, this crate
//! exposes a self-contained [`PaletteViewState`] that the caller owns.
//! Theme colours are hard-coded sensible defaults — wiring to the VAC
//! theme service is a Step 4 concern.
//!
//! The extraction proves the donor pattern is liftable without
//! dragging the donor `AppState` along. No code in this crate depends
//! on `vac_tui_runtime`, on the donor `app::AppState`, or on
//! `commands_to_helper_commands()` legacy merge logic.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem},
};
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec};

/// Palette view state. Caller owns this; the renderer is pure.
///
/// The donor distributed these fields across `AppState.input_state`
/// (`filtered_helpers`, `show_helper_dropdown`, `helper_scroll`,
/// `helper_selected`). Pulling them into one struct is what lets the
/// widget travel without the donor's app.
///
/// Match policy is **injected**, not hard-coded. The default is a
/// strict slash-prefix match (easy to assert in the extraction proof
/// tests); product code is expected to swap in something fuzzier as
/// the donor originally tolerated. The renderer never inspects the
/// policy, so changes here never reach into UI code.
#[derive(Debug, Clone)]
pub struct PaletteViewState {
    /// Whether the palette overlay is open at all.
    pub visible: bool,
    /// Current filter input as the operator typed it (e.g. `"/mod"`).
    pub input: String,
    /// All known commands. Filtering runs against this list each
    /// render — kept simple deliberately (the donor caches a
    /// `filtered_helpers` view, but the source of truth here is the
    /// full registry, exposing the filter as a pure function).
    pub all: Vec<ShellCommandSpec>,
    /// Selected row in the *filtered* list (0-based).
    pub selected: usize,
    /// Top-of-window offset for scrolling (0-based).
    pub scroll: usize,
    /// Match policy. Stable function pointer keeps the type `Clone`
    /// and avoids dragging trait-object machinery into the widget.
    pub filter: FilterFn,
}

impl Default for PaletteViewState {
    fn default() -> Self {
        Self {
            visible: false,
            input: String::new(),
            all: Vec::new(),
            selected: 0,
            scroll: 0,
            filter: prefix_filter,
        }
    }
}

impl PaletteViewState {
    pub fn new(all: Vec<ShellCommandSpec>) -> Self {
        Self {
            all,
            ..Self::default()
        }
    }

    /// Replace the filter policy. Default is [`prefix_filter`]; pass a
    /// fuzzy/contains/scoring function here when product behaviour
    /// needs to widen.
    pub fn with_filter(mut self, filter: FilterFn) -> Self {
        self.filter = filter;
        self
    }

    /// Computed, filtered list of entries that should appear at the
    /// current input. Pure — does not mutate state.
    pub fn filtered(&self) -> Vec<ShellCommandSpec> {
        (self.filter)(&self.input, &self.all)
    }
}

/// Match policy signature. Receives the current input and the full
/// command list, returns the visible subset (already
/// `palette_visible`-checked or not — that's the policy's call).
pub type FilterFn = fn(&str, &[ShellCommandSpec]) -> Vec<ShellCommandSpec>;

/// Default policy — strict slash-prefix. Empty input or `"/"` lists
/// every `palette_visible` entry; a non-empty needle keeps entries
/// whose slash starts with it.
pub fn prefix_filter(input: &str, all: &[ShellCommandSpec]) -> Vec<ShellCommandSpec> {
    let needle = input.trim();
    if needle.is_empty() || needle == "/" {
        return all.iter().filter(|s| s.palette_visible).cloned().collect();
    }
    all.iter()
        .filter(|s| s.palette_visible && s.slash.starts_with(needle))
        .cloned()
        .collect()
}

/// Back-compat alias from the Step 3a proof. Prefer [`prefix_filter`]
/// in new code; this name was used in the proof's tests and is kept
/// stable so the test set continues to track the same behaviour.
pub fn filter_entries(input: &str, all: &[ShellCommandSpec]) -> Vec<ShellCommandSpec> {
    prefix_filter(input, all)
}

/// Logical key event consumed by the palette. The crate stays free of
/// `crossterm` so it can be driven from any input source (live TUI
/// events, deterministic tests, ACP messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKey {
    Up,
    Down,
    Enter,
    Escape,
    Backspace,
    Char(char),
}

/// Outcome of a key dispatch. The caller decides what to do with a
/// selection (typically: invoke the slash command via the VAC
/// command registry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteEvent {
    /// Operator chose this command. Carries the slash string so the
    /// caller can resolve it via `VacCommandRegistry::by_slash`.
    Selected(String),
    /// Operator dismissed without selecting.
    Dismissed,
    /// Key was handled but did not produce a terminal event.
    Consumed,
    /// Key was not handled — caller may forward elsewhere.
    Ignored,
}

/// Stateless key handler. Mutates the view state and returns a
/// terminal event when the operator selects or dismisses.
pub fn on_key(view: &mut PaletteViewState, key: PaletteKey) -> PaletteEvent {
    if !view.visible {
        return PaletteEvent::Ignored;
    }
    let visible_count = view.filtered().len();
    match key {
        PaletteKey::Up => {
            if visible_count > 0 {
                view.selected = view.selected.saturating_sub(1);
                if view.selected < view.scroll {
                    view.scroll = view.selected;
                }
            }
            PaletteEvent::Consumed
        }
        PaletteKey::Down => {
            if visible_count > 0 {
                view.selected = (view.selected + 1).min(visible_count - 1);
                if view.selected >= view.scroll + MAX_VISIBLE_ITEMS {
                    view.scroll = view.selected + 1 - MAX_VISIBLE_ITEMS;
                }
            }
            PaletteEvent::Consumed
        }
        PaletteKey::Enter => match view.filtered().get(view.selected) {
            Some(entry) => PaletteEvent::Selected(entry.slash.clone()),
            None => PaletteEvent::Consumed,
        },
        PaletteKey::Escape => {
            view.visible = false;
            view.input.clear();
            view.selected = 0;
            view.scroll = 0;
            PaletteEvent::Dismissed
        }
        PaletteKey::Char(c) => {
            view.input.push(c);
            view.selected = 0;
            view.scroll = 0;
            PaletteEvent::Consumed
        }
        PaletteKey::Backspace => {
            view.input.pop();
            view.selected = 0;
            view.scroll = 0;
            PaletteEvent::Consumed
        }
    }
}

const MAX_VISIBLE_ITEMS: usize = 5;

/// Render the palette dropdown. Mirrors the donor layout (visible
/// window, top/bottom arrows, position counter on overflow) without
/// reaching into a global `AppState`.
pub fn render_palette(f: &mut Frame, view: &PaletteViewState, area: Rect) {
    if !view.visible {
        return;
    }
    let entries = view.filtered();
    if entries.is_empty() {
        return;
    }
    let total = entries.len();
    let visible_height = MAX_VISIBLE_ITEMS.min(total);
    let max_scroll = total.saturating_sub(visible_height);
    let scroll = view.scroll.min(max_scroll);

    let has_above = scroll > 0;
    let has_below = scroll < max_scroll;
    let arrow_lines = (has_above as usize) + (has_below as usize);
    let counter_line = if has_above || has_below { 1 } else { 0 };
    let height = (visible_height + arrow_lines + counter_line) as u16;
    let compact_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height,
    };

    // Theme — donor reads ThemeColors::dropdown_*; we hold sensible
    // defaults here so this crate stays free of the VAC theme service.
    let bg = Color::Reset;
    let muted = Color::DarkGray;
    let text = Color::Gray;
    let highlight_fg = Color::Black;
    let highlight_bg = Color::Cyan;
    let cmd_fg = Color::Cyan;

    let max_name = entries.iter().map(|s| s.slash.len()).max().unwrap_or(0);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if has_above {
        lines.push(Line::from(Span::styled(
            " ▲",
            Style::default().fg(muted).bg(bg),
        )));
    }
    for i in 0..visible_height {
        let idx = scroll + i;
        let entry = &entries[idx];
        let is_selected = idx == view.selected;
        let pad = " ".repeat(max_name.saturating_sub(entry.slash.len()));

        let cmd_style = if is_selected {
            Style::default().fg(highlight_fg).bg(highlight_bg)
        } else {
            Style::default().fg(cmd_fg).bg(bg)
        };
        let desc_style = if is_selected {
            Style::default().fg(highlight_fg).bg(highlight_bg)
        } else {
            Style::default().fg(text).bg(bg)
        };
        let pad_style = if is_selected {
            Style::default().fg(highlight_fg).bg(highlight_bg)
        } else {
            Style::default().fg(muted).bg(bg)
        };
        let prefix = match entry.kind {
            ShellCommandKind::PromptTemplate => " – [template] ",
            _ => " – ",
        };
        let desc = format!("{}{}", prefix, entry.description);
        lines.push(Line::from(vec![
            Span::styled(format!("  {}  ", entry.slash), cmd_style),
            Span::styled(pad, pad_style),
            Span::styled(desc, desc_style),
        ]));
    }
    if has_below {
        lines.push(Line::from(Span::styled(
            " ▼",
            Style::default().fg(muted).bg(bg),
        )));
    }
    if has_above || has_below {
        lines.push(Line::from(Span::styled(
            format!(" ({}/{})", view.selected + 1, total),
            Style::default().fg(muted).bg(bg),
        )));
    }

    let items: Vec<ListItem> = lines.into_iter().map(ListItem::new).collect();
    let list = List::new(items)
        .block(Block::default())
        .style(Style::default().bg(bg).fg(text));
    f.render_widget(list, compact_area);
}
