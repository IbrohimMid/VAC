//! L3 — Signal workbench tab.
//!
//! Left column: list of active signal streams (id, kind, lines, dropped).
//! Right column: selected stream tail (50 lines) and distilled key-lines.
//!
//! Data source: `AppState::signal_registry()` which snapshots the
//! per-subsystem `SignalBuffer`s (vil_dev, shell, mcp, runtime). Render
//! is read-only; interaction beyond tab navigation lands in follow-up.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::WorkbenchTabView;
use crate::app::AppState;
use crate::system_pulse::FacetSeverity;

pub struct SignalTab;

impl WorkbenchTabView for SignalTab {
    fn tab_label(state: &AppState) -> String {
        let reg = state.signal_registry();
        let active = reg.len();
        if active == 0 {
            "Signal".to_string()
        } else {
            format!("Signal ({active})")
        }
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        let reg = state.signal_registry();
        let summaries = reg.summary();

        // Left: stream list
        let items: Vec<ListItem> = if summaries.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "no active signal streams",
                Style::default().add_modifier(Modifier::DIM),
            )))]
        } else {
            summaries
                .iter()
                .map(|s| {
                    let kind = format!("{:?}", s.kind);
                    let suffix = if s.dropped > 0 {
                        format!(" ({} lines, {} dropped)", s.lines, s.dropped)
                    } else {
                        format!(" ({} lines)", s.lines)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("[{kind}] "),
                            Style::default().add_modifier(Modifier::DIM),
                        ),
                        Span::raw(s.id.clone()),
                        Span::styled(suffix, Style::default().add_modifier(Modifier::DIM)),
                    ]))
                })
                .collect()
        };
        // E3 — align panel header with SystemPulse grammar: the
        // glyph encodes stream health. Dropped lines escalate to
        // Warn; empty registry stays Ok; any active stream is Info.
        let total_dropped: u64 = summaries.iter().map(|s| s.dropped).sum();
        let severity = if total_dropped > 0 {
            FacetSeverity::Warn
        } else if summaries.is_empty() {
            FacetSeverity::Ok
        } else {
            FacetSeverity::Info
        };
        let title = format!(" {} Streams ", severity.glyph());
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(Style::default().add_modifier(Modifier::BOLD)),
        );
        f.render_widget(list, split[0]);

        // Right: tail + key-lines of first stream (selection UI lands later)
        let detail_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(split[1]);

        let (tail_body, key_body) = match summaries.first() {
            Some(s) => match reg.get(&s.id) {
                Some(buf) => {
                    let tail = buf
                        .tail(20)
                        .into_iter()
                        .map(|l| Line::from(Span::raw(l.to_string())))
                        .collect::<Vec<_>>();
                    let view = buf.distilled_default(20);
                    let key = view
                        .key_lines
                        .iter()
                        .map(|l| {
                            Line::from(vec![
                                Span::styled("! ", Style::default().add_modifier(Modifier::BOLD)),
                                Span::raw(l.clone()),
                            ])
                        })
                        .collect::<Vec<_>>();
                    (tail, key)
                }
                None => (Vec::new(), Vec::new()),
            },
            None => (Vec::new(), Vec::new()),
        };

        let tail_title = match summaries.first() {
            Some(s) => format!(" Tail ({}) ", s.id),
            None => " Tail ".to_string(),
        };
        let tail = Paragraph::new(tail_body)
            .block(Block::default().borders(Borders::ALL).title(tail_title))
            .wrap(Wrap { trim: false });
        f.render_widget(tail, detail_split[0]);

        let key_title = format!(" Key lines ({}) ", key_body.len());
        let key = Paragraph::new(key_body)
            .block(Block::default().borders(Borders::ALL).title(key_title))
            .wrap(Wrap { trim: false });
        f.render_widget(key, detail_split[1]);
    }
}
