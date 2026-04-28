//! Step 2 slice 6 — shortcuts / command-palette popup widget.
//!
//! # Provenance
//!
//! Source: `vendor/stakpak/tui/src/services/shortcuts_popup.rs` at
//! commit `2e75bd56970d114ab41653aed045300b9a3257a7` (Apache 2.0).
//!
//! # Scope
//!
//! Reviewer-gated extraction: **Commands + Shortcuts sections only.**
//! Sessions section is deliberately omitted — it requires a `VacPaths`
//! adapter that the shell stack has not landed yet, and the donor
//! map flagged it as a separate slice. The `ShortcutsMode` enum
//! reflects the supported set; adding `Sessions` later means adding
//! a variant + its render path.
//!
//! # Decoupling moves
//!
//! * Donor reads from `state.shortcuts_panel_state.mode`,
//!   `state.command_palette_state.{search, scroll, is_selected}`,
//!   and a global `OnceLock` cache of the formatted shortcuts. We
//!   pull those into a self-contained [`ShortcutsView`] and rebuild
//!   the formatted lines per render — caching is a host concern.
//! * Donor's `filter_commands` reaches into `services::commands` to
//!   pull the registered set. We accept a `Vec<ShellCommandSpec>` in
//!   the view; the host hands a snapshot from any
//!   `VacCommandRegistry`.
//! * Donor renders the section dispatcher inline against `AppState`.
//!   Here `render_shortcuts_popup` is a pure function over the view.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
};
use std::collections::HashMap;
use vac_shell_contracts::{SessionEntry, ShellCommandSpec};

const SCROLL_BUFFER_LINES: usize = 1;

/// Which tab the popup is currently showing. The Sessions tab lit up
/// in slice 7 once `VacPaths` was implemented and a host-side
/// session-directory enumerator landed (`vac_shell_host_paths`).
/// Resume / delete / transcript-open flows are still deliberately
/// out of scope; this tab is read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShortcutsMode {
    #[default]
    Commands,
    Shortcuts,
    Sessions,
}

/// One row in the Shortcuts tab. The donor exposes the same shape;
/// kept stable so the formatting loop ports verbatim.
#[derive(Debug, Clone)]
pub struct Shortcut {
    pub key: String,
    pub description: String,
    pub category: String,
}

impl Shortcut {
    pub fn new(
        key: impl Into<String>,
        description: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            category: category.into(),
        }
    }
}

/// View state. Caller owns it and feeds `commands` from a
/// `VacCommandRegistry`; `shortcuts` is typically the catalogue
/// returned by [`default_shortcuts`].
#[derive(Debug, Clone, Default)]
pub struct ShortcutsView {
    pub mode: ShortcutsMode,
    pub visible: bool,
    pub search: String,
    pub command_selected: usize,
    pub command_scroll: usize,
    pub shortcuts_scroll: usize,
    pub commands: Vec<ShellCommandSpec>,
    pub shortcuts: Vec<Shortcut>,
    /// Sessions snapshot. The host fills this from
    /// `vac_shell_host_paths::enumerate_sessions(...)` (or any other
    /// `VacPaths`-backed scanner) — the widget never touches disk.
    pub sessions: Vec<SessionEntry>,
    /// Selected row in the sessions list.
    pub sessions_scroll: usize,
}

impl ShortcutsView {
    pub fn new(commands: Vec<ShellCommandSpec>, shortcuts: Vec<Shortcut>) -> Self {
        Self {
            commands,
            shortcuts,
            ..Self::default()
        }
    }

    pub fn with_sessions(mut self, sessions: Vec<SessionEntry>) -> Self {
        self.sessions = sessions;
        self
    }

    /// Cycle Commands → Shortcuts → Sessions → Commands.
    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            ShortcutsMode::Commands => ShortcutsMode::Shortcuts,
            ShortcutsMode::Shortcuts => ShortcutsMode::Sessions,
            ShortcutsMode::Sessions => ShortcutsMode::Commands,
        };
        self.search.clear();
    }
}

/// Default catalogue ported from the donor. Hosts may extend or
/// replace this by passing a different `Vec<Shortcut>` into the view.
pub fn default_shortcuts() -> Vec<Shortcut> {
    vec![
        Shortcut::new("↑/↓", "Navigate messages", "Navigation"),
        Shortcut::new("Page Up/Down", "Page through messages", "Navigation"),
        Shortcut::new("Tab", "Complete command or select file", "Navigation"),
        Shortcut::new("Esc", "Close dialogs/popups", "Navigation"),
        Shortcut::new("Enter", "Submit input", "Text Input"),
        Shortcut::new("Backspace", "Delete previous character", "Text Input"),
        Shortcut::new("Ctrl+J", "Insert newline", "Text Input"),
        Shortcut::new("Ctrl+W", "Delete previous word", "Text Input"),
        Shortcut::new("Ctrl+U", "Delete to start of line", "Text Input"),
        Shortcut::new("Ctrl+P", "Open command palette", "UI Controls"),
        Shortcut::new("Ctrl+S", "Show shortcuts (this popup)", "UI Controls"),
        Shortcut::new("Ctrl+C", "Quit (double press)", "UI Controls"),
        Shortcut::new("/help", "Show help information", "Commands"),
        Shortcut::new("/model", "Switch model", "Commands"),
        Shortcut::new("/runtime", "Open runtime surface", "Commands"),
        Shortcut::new("/chat", "Return to conversation surface", "Commands"),
    ]
}

/// Filter the shortcut catalogue against an optional substring needle
/// (case-insensitive). Empty needle returns the full set.
pub fn filter_shortcuts<'a>(needle: &str, all: &'a [Shortcut]) -> Vec<&'a Shortcut> {
    let n = needle.trim().to_lowercase();
    if n.is_empty() {
        return all.iter().collect();
    }
    all.iter()
        .filter(|s| {
            s.key.to_lowercase().contains(&n)
                || s.description.to_lowercase().contains(&n)
                || s.category.to_lowercase().contains(&n)
        })
        .collect()
}

/// Filter sessions against an optional substring needle
/// (case-insensitive over `id` + `label`). Empty needle returns all.
pub fn filter_sessions<'a>(needle: &str, all: &'a [SessionEntry]) -> Vec<&'a SessionEntry> {
    let n = needle.trim().to_lowercase();
    if n.is_empty() {
        return all.iter().collect();
    }
    all.iter()
        .filter(|s| s.id.to_lowercase().contains(&n) || s.label.to_lowercase().contains(&n))
        .collect()
}

/// Filter palette-visible commands by slash-prefix or by description
/// substring (donor permitted both — adopting the broader form so
/// the popup can find `/model` by typing "model" too). Reuses the
/// `palette_visible` flag the registry already carries.
pub fn filter_commands<'a>(needle: &str, all: &'a [ShellCommandSpec]) -> Vec<&'a ShellCommandSpec> {
    let n = needle.trim().to_lowercase();
    let visible = all.iter().filter(|s| s.palette_visible);
    if n.is_empty() {
        return visible.collect();
    }
    visible
        .filter(|s| {
            s.slash.to_lowercase().contains(&n) || s.description.to_lowercase().contains(&n)
        })
        .collect()
}

/// Build the formatted shortcuts content lines. Mirrors the donor
/// `get_cached_shortcuts_content` layout (category headers, fixed
/// 25/40 column alignment) without the global cache.
pub fn build_shortcuts_lines(width: usize, shortcuts: &[Shortcut]) -> Vec<Line<'static>> {
    let mut categories: HashMap<&str, Vec<&Shortcut>> = HashMap::new();
    for s in shortcuts {
        categories.entry(&s.category).or_default().push(s);
    }
    let category_order = [
        "Navigation",
        "Text Input",
        "Tool Management",
        "UI Controls",
        "Commands",
        "File Search",
        "Mouse",
    ];

    let cyan = Color::Cyan;
    let muted = Color::DarkGray;
    let green = Color::Green;
    let text = Color::Gray;

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(""));
    for category_name in category_order {
        if let Some(rows) = categories.get(category_name) {
            let dash_w = width.saturating_sub(category_name.len() + 5);
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", category_name),
                    Style::default().fg(cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled("─".repeat(dash_w), Style::default().fg(muted)),
            ]));
            for s in rows {
                let key_col = format!(" {:<25}", s.key);
                let desc_col = format!("{:<40} ", s.description);
                lines.push(Line::from(vec![
                    Span::styled(
                        key_col,
                        Style::default().fg(green).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(desc_col, Style::default().fg(text)),
                ]));
            }
            lines.push(Line::from(""));
        }
    }
    lines
}

/// Render the popup. No-op when `!view.visible`.
pub fn render_shortcuts_popup(f: &mut Frame, view: &ShortcutsView, area: Rect) {
    if !view.visible {
        return;
    }
    f.render_widget(ratatui::widgets::Clear, area);

    let cyan = Color::Cyan;
    let title_color = Color::Cyan;
    let muted = Color::DarkGray;
    let accent = Color::Cyan;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(cyan));
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let title = Line::from(Span::styled(
        " Command Palette",
        Style::default()
            .fg(title_color)
            .add_modifier(Modifier::BOLD),
    ));
    let title_widget = Paragraph::new(title);

    let tab_titles = vec![" Commands ", " Shortcuts ", " Sessions "];
    let selected_tab = match view.mode {
        ShortcutsMode::Commands => 0,
        ShortcutsMode::Shortcuts => 1,
        ShortcutsMode::Sessions => 2,
    };
    let tabs = Tabs::new(tab_titles)
        .select(selected_tab)
        .style(Style::default().fg(muted))
        .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .divider(" | ");

    match view.mode {
        ShortcutsMode::Commands => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(3),
                    Constraint::Min(3),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(inner);
            f.render_widget(title_widget, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            render_commands_section(f, view, chunks[2], chunks[3], chunks[4], chunks[5], area);
        }
        ShortcutsMode::Shortcuts => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(inner);
            f.render_widget(title_widget, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            render_shortcuts_section(f, view, chunks[3], chunks[4], chunks[5], chunks[6], area);
        }
        ShortcutsMode::Sessions => {
            // Donor layout: title, tabs, spacer, search, spacer, content,
            // scroll, help. Mirrored verbatim modulo `AppState` reads.
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(inner);
            f.render_widget(title_widget, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            render_sessions_section(f, view, chunks[3], chunks[5], chunks[6], chunks[7]);
        }
    }

    f.render_widget(block, area);
}

fn render_sessions_section(
    f: &mut Frame,
    view: &ShortcutsView,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
) {
    let muted = Color::DarkGray;
    let accent = Color::Cyan;
    let text = Color::Gray;

    f.render_widget(
        Paragraph::new(render_search_line(view, "Type to filter sessions"))
            .block(Block::default().border_style(Style::default().fg(muted))),
        search_area,
    );

    let filtered = filter_sessions(&view.search, &view.sessions);
    let total = filtered.len();
    let height = content_area.height as usize;
    let max_scroll = total.saturating_sub(height);
    let scroll = view.sessions_scroll.min(max_scroll);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if total == 0 {
        lines.push(Line::from(Span::styled(
            "  no sessions yet — start typing to begin",
            Style::default().fg(muted),
        )));
    } else {
        for i in 0..height {
            let idx = scroll + i;
            if idx < total {
                let s = filtered[idx];
                let label = if s.label.is_empty() {
                    s.id.as_str()
                } else {
                    s.label.as_str()
                };
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(label.to_string(), Style::default().fg(text)),
                    Span::styled("   ", Style::default()),
                    Span::styled(format!("[{}]", s.id), Style::default().fg(muted)),
                ]));
            } else {
                lines.push(Line::from(""));
            }
        }
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        content_area,
    );

    let has_above = scroll > 0;
    let has_below = scroll < max_scroll;
    if has_above || has_below {
        let cumulative = (scroll + height).min(total);
        let mut indicator: Vec<Span<'static>> = Vec::new();
        if has_above {
            indicator.push(Span::styled(" ▲ ", Style::default().fg(muted)));
        }
        indicator.push(Span::styled(
            format!("({}/{})", cumulative, total),
            Style::default().fg(muted),
        ));
        if has_below {
            indicator.push(Span::styled(" ▼", Style::default().fg(muted)));
        }
        f.render_widget(Paragraph::new(Line::from(indicator)), scroll_area);
    } else {
        f.render_widget(Paragraph::new(""), scroll_area);
    }

    f.render_widget(help_line(accent, muted), help_area);
}

fn render_search_line(view: &ShortcutsView, placeholder: &str) -> Line<'static> {
    let magenta = Color::Magenta;
    let cyan = Color::Cyan;
    let muted = Color::DarkGray;
    let text = Color::Gray;
    if view.search.is_empty() {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(">", Style::default().fg(magenta)),
            Span::raw(" "),
            Span::styled("|", Style::default().fg(cyan)),
            Span::styled(placeholder.to_string(), Style::default().fg(muted)),
            Span::raw(" "),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(">", Style::default().fg(magenta)),
            Span::raw(" "),
            Span::styled(
                view.search.clone(),
                Style::default().fg(text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("|", Style::default().fg(cyan)),
            Span::raw(" "),
        ])
    }
}

fn render_commands_section(
    f: &mut Frame,
    view: &ShortcutsView,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
    area: Rect,
) {
    let muted = Color::DarkGray;
    let accent = Color::Cyan;
    let highlight_fg = Color::Black;
    let highlight_bg = Color::Cyan;
    let text = Color::Gray;

    let search_text = ratatui::text::Text::from(vec![
        Line::from(""),
        render_search_line(view, "Type to filter"),
        Line::from(""),
    ]);
    f.render_widget(Paragraph::new(search_text), search_area);

    let filtered = filter_commands(&view.search, &view.commands);
    let total = filtered.len();
    let height = content_area.height as usize;
    let max_scroll = total.saturating_sub(height.saturating_sub(SCROLL_BUFFER_LINES));
    let scroll = view.command_scroll.min(max_scroll);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if scroll > 0 {
        lines.push(Line::from(Span::styled(" ▲", Style::default().fg(muted))));
    }
    for i in 0..height {
        let idx = scroll + i;
        if idx < total {
            let cmd = filtered[idx];
            let avail = (area.width as usize).saturating_sub(2);
            let is_selected = idx == view.command_selected;
            let bg = if is_selected {
                highlight_bg
            } else {
                Color::Reset
            };
            let fg = if is_selected { highlight_fg } else { text };
            let shortcut_str = cmd.shortcut.clone().unwrap_or_default();
            let name_w = avail.saturating_sub(shortcut_str.len() + 2);
            let name_col = format!(" {:<width$}", cmd.slash, width = name_w);
            let sc_col = format!("{} ", shortcut_str);
            lines.push(Line::from(vec![
                Span::styled(name_col, Style::default().fg(fg).bg(bg)),
                Span::styled(
                    sc_col,
                    Style::default()
                        .fg(if is_selected { highlight_fg } else { muted })
                        .bg(bg),
                ),
            ]));
        } else {
            lines.push(Line::from(""));
        }
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Color::Reset).fg(text)),
        content_area,
    );

    let has_below = scroll < max_scroll;
    if scroll > 0 || has_below {
        let cumulative = (scroll + height).min(total);
        let mut indicator = vec![Span::styled(
            format!(" ({}/{})", cumulative, total),
            Style::default().fg(muted),
        )];
        if has_below {
            indicator.push(Span::styled(" ▼", Style::default().fg(muted)));
        }
        f.render_widget(Paragraph::new(Line::from(indicator)), scroll_area);
    } else {
        f.render_widget(Paragraph::new(""), scroll_area);
    }

    f.render_widget(help_line(accent, muted), help_area);
}

fn render_shortcuts_section(
    f: &mut Frame,
    view: &ShortcutsView,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
    area: Rect,
) {
    let muted = Color::DarkGray;
    let accent = Color::Cyan;

    f.render_widget(
        Paragraph::new(render_search_line(view, "Type to filter (e.g. 'ctrl+')"))
            .block(Block::default().border_style(Style::default().fg(muted))),
        search_area,
    );

    let filtered: Vec<Shortcut> = filter_shortcuts(&view.search, &view.shortcuts)
        .into_iter()
        .cloned()
        .collect();
    let lines = build_shortcuts_lines(area.width as usize, &filtered);
    let total = lines.len();
    let height = content_area.height as usize;
    let max_scroll = total.saturating_sub(height);
    let scroll = view.shortcuts_scroll.min(max_scroll);

    let visible: Vec<Line<'static>> = lines.iter().skip(scroll).take(height).cloned().collect();
    f.render_widget(
        Paragraph::new(visible).wrap(Wrap { trim: false }),
        content_area,
    );

    let has_above = scroll > 0;
    let has_below = scroll < max_scroll;
    if has_above || has_below {
        let mut indicator: Vec<Span<'static>> = Vec::new();
        if has_above {
            indicator.push(Span::styled(" ▲", Style::default().fg(muted)));
            indicator.push(Span::raw(" "));
        }
        let cumulative = (scroll + height).min(total);
        indicator.push(Span::styled(
            format!("({}/{})", cumulative, total),
            Style::default().fg(muted),
        ));
        if has_below {
            indicator.push(Span::raw(" "));
            indicator.push(Span::styled("▼", Style::default().fg(muted)));
        }
        f.render_widget(Paragraph::new(Line::from(indicator)), scroll_area);
    } else {
        f.render_widget(Paragraph::new(""), scroll_area);
    }

    f.render_widget(help_line(accent, muted), help_area);
}

fn help_line(accent: Color, muted: Color) -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        Span::styled(" ↑/↓", Style::default().fg(muted)),
        Span::styled(" navigate", Style::default().fg(accent)),
        Span::raw("  "),
        Span::styled("enter", Style::default().fg(muted)),
        Span::styled(" select", Style::default().fg(accent)),
        Span::raw("  "),
        Span::styled("tab", Style::default().fg(muted)),
        Span::styled(" switch", Style::default().fg(accent)),
        Span::raw("  "),
        Span::styled("esc", Style::default().fg(muted)),
        Span::styled(" close", Style::default().fg(accent)),
    ]))
}
