//! Slice 13 — session browser widget.
//!
//! Pure UI: caller hands `Vec<SessionEntry>` (host-built via
//! `enumerate_sessions`) plus an optional `SessionPreview` for the
//! current selection. Emits `SessionBrowserEvent` intents; the host
//! turns them into `SessionAction` and drives transcript loading,
//! resume, archive, and delete.
//!
//! Delete is two-step (Esc/Enter on `delete_pending = true`) so an
//! accidental keypress cannot wipe a session.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use vac_shell_contracts::{SessionAction, SessionPreview, SessionTileView};

#[derive(Debug, Clone, Default)]
pub struct SessionBrowserView {
    pub visible: bool,
    pub search: String,
    pub selected: usize,
    /// D10 — replaces `entries: Vec<SessionEntry>`. Each tile carries
    /// the entry plus an optional tool-use badge summary.
    pub tiles: Vec<SessionTileView>,
    pub preview: Option<SessionPreview>,
    pub delete_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionBrowserKey {
    Up,
    Down,
    Enter,
    Resume,
    Archive,
    Delete,
    Escape,
    Char(char),
    Backspace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionBrowserEvent {
    Action(SessionAction),
    Dismissed,
    Consumed,
    Ignored,
}

pub fn filter_sessions<'a>(needle: &str, all: &'a [SessionTileView]) -> Vec<&'a SessionTileView> {
    let n = needle.trim().to_lowercase();
    if n.is_empty() {
        return all.iter().collect();
    }
    all.iter()
        .filter(|t| {
            t.entry.id.to_lowercase().contains(&n) || t.entry.label.to_lowercase().contains(&n)
        })
        .collect()
}

pub fn on_key(view: &mut SessionBrowserView, key: SessionBrowserKey) -> SessionBrowserEvent {
    if !view.visible {
        return SessionBrowserEvent::Ignored;
    }
    let filtered = filter_sessions(&view.search, &view.tiles);
    let len = filtered.len();
    match key {
        SessionBrowserKey::Up => {
            view.delete_pending = false;
            if len > 0 {
                view.selected = view.selected.saturating_sub(1);
            }
            SessionBrowserEvent::Consumed
        }
        SessionBrowserKey::Down => {
            view.delete_pending = false;
            if len > 0 {
                view.selected = (view.selected + 1).min(len - 1);
            }
            SessionBrowserEvent::Consumed
        }
        SessionBrowserKey::Enter => match filtered.get(view.selected) {
            Some(t) => SessionBrowserEvent::Action(SessionAction::Open { id: t.entry.id.clone() }),
            None => SessionBrowserEvent::Consumed,
        },
        SessionBrowserKey::Resume => match filtered.get(view.selected) {
            Some(t) => {
                SessionBrowserEvent::Action(SessionAction::Resume { id: t.entry.id.clone() })
            }
            None => SessionBrowserEvent::Consumed,
        },
        SessionBrowserKey::Archive => match filtered.get(view.selected) {
            Some(t) => {
                SessionBrowserEvent::Action(SessionAction::Archive { id: t.entry.id.clone() })
            }
            None => SessionBrowserEvent::Consumed,
        },
        SessionBrowserKey::Delete => {
            if !view.delete_pending {
                view.delete_pending = true;
                SessionBrowserEvent::Consumed
            } else {
                view.delete_pending = false;
                match filtered.get(view.selected) {
                    Some(t) => SessionBrowserEvent::Action(SessionAction::Delete {
                        id: t.entry.id.clone(),
                    }),
                    None => SessionBrowserEvent::Consumed,
                }
            }
        }
        SessionBrowserKey::Escape => {
            if view.delete_pending {
                view.delete_pending = false;
                SessionBrowserEvent::Consumed
            } else {
                view.visible = false;
                view.search.clear();
                view.selected = 0;
                SessionBrowserEvent::Dismissed
            }
        }
        SessionBrowserKey::Char(c) => {
            view.search.push(c);
            view.selected = 0;
            SessionBrowserEvent::Consumed
        }
        SessionBrowserKey::Backspace => {
            view.search.pop();
            view.selected = 0;
            SessionBrowserEvent::Consumed
        }
    }
}

pub fn render_session_browser(f: &mut Frame, view: &SessionBrowserView, area: Rect) {
    if !view.visible {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Sessions ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(inner);

    let filtered = filter_sessions(&view.search, &view.tiles);
    let mut left: Vec<Line<'static>> = Vec::new();
    left.push(Line::from(vec![
        Span::styled(" search ", Style::default().fg(Color::DarkGray)),
        Span::styled(view.search.clone(), Style::default().fg(Color::Gray)),
        Span::styled("|", Style::default().fg(Color::Cyan)),
    ]));
    if filtered.is_empty() {
        left.push(Line::from(Span::styled(
            "  no sessions",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, tile) in filtered.iter().enumerate() {
            let is_sel = i == view.selected;
            let label_style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let label = if tile.entry.label.is_empty() {
                tile.entry.id.as_str()
            } else {
                tile.entry.label.as_str()
            };
            let badge = tile
                .tool_summary
                .as_ref()
                .map(|s| s.badge_text())
                .unwrap_or_default();
            let badge_color = tile
                .tool_summary
                .as_ref()
                .map(|s| {
                    if s.error_count > 0 {
                        Color::Red
                    } else if s.total_calls > 0 {
                        Color::Green
                    } else {
                        Color::DarkGray
                    }
                })
                .unwrap_or(Color::DarkGray);
            left.push(Line::from(vec![
                Span::styled(format!("  {label}"), label_style),
                Span::styled(
                    format!("   [{}]", tile.entry.id),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            if !badge.is_empty() {
                left.push(Line::from(Span::styled(
                    format!("    {badge}"),
                    Style::default().fg(badge_color),
                )));
            }
        }
    }
    f.render_widget(
        Paragraph::new(left).wrap(Wrap { trim: false }),
        chunks[0],
    );

    let mut right: Vec<Line<'static>> = Vec::new();
    if let Some(p) = &view.preview {
        if let Some(t) = &p.title {
            right.push(Line::from(Span::styled(
                format!(" {t}"),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
            right.push(Line::raw(""));
        }
        for line in &p.lines {
            right.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(Color::Gray),
            )));
        }
    } else {
        right.push(Line::from(Span::styled(
            " select a session to preview",
            Style::default().fg(Color::DarkGray),
        )));
    }
    if view.delete_pending {
        right.push(Line::raw(""));
        right.push(Line::from(Span::styled(
            " press Delete again to confirm — Esc to cancel",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    f.render_widget(
        Paragraph::new(right).wrap(Wrap { trim: false }),
        chunks[1],
    );
}
