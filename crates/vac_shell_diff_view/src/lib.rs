//! Slice 15 — diff review widget.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use vac_shell_contracts::{DiffFileView, DiffLineKind, DiffReviewEvent};

#[derive(Debug, Clone, Default)]
pub struct DiffReviewView {
    pub visible: bool,
    pub files: Vec<DiffFileView>,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffReviewKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Approve,
    Reject,
    Open,
    Escape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffReviewKeyEvent {
    Event(DiffReviewEvent),
    Consumed,
    Ignored,
}

pub fn on_key(view: &mut DiffReviewView, key: DiffReviewKey) -> DiffReviewKeyEvent {
    if !view.visible {
        return DiffReviewKeyEvent::Ignored;
    }
    let len = view.files.len();
    match key {
        DiffReviewKey::Up => {
            if len > 0 {
                view.selected = view.selected.saturating_sub(1);
                view.scroll = 0;
            }
            DiffReviewKeyEvent::Consumed
        }
        DiffReviewKey::Down => {
            if len > 0 {
                view.selected = (view.selected + 1).min(len - 1);
                view.scroll = 0;
            }
            DiffReviewKeyEvent::Consumed
        }
        DiffReviewKey::PageUp => {
            view.scroll = view.scroll.saturating_sub(8);
            DiffReviewKeyEvent::Consumed
        }
        DiffReviewKey::PageDown => {
            view.scroll = view.scroll.saturating_add(8);
            DiffReviewKeyEvent::Consumed
        }
        DiffReviewKey::Approve => match view.files.get(view.selected) {
            Some(f) => DiffReviewKeyEvent::Event(DiffReviewEvent::ApproveFile(f.path.clone())),
            None => DiffReviewKeyEvent::Consumed,
        },
        DiffReviewKey::Reject => match view.files.get(view.selected) {
            Some(f) => DiffReviewKeyEvent::Event(DiffReviewEvent::RejectFile(f.path.clone())),
            None => DiffReviewKeyEvent::Consumed,
        },
        DiffReviewKey::Open => match view.files.get(view.selected) {
            Some(f) => DiffReviewKeyEvent::Event(DiffReviewEvent::OpenFile(f.path.clone())),
            None => DiffReviewKeyEvent::Consumed,
        },
        DiffReviewKey::Escape => {
            view.visible = false;
            DiffReviewKeyEvent::Event(DiffReviewEvent::Dismiss)
        }
    }
}

pub fn render_diff_review(f: &mut Frame, view: &DiffReviewView, area: Rect) {
    if !view.visible {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Review ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if view.files.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  no pending changes",
                Style::default().fg(Color::DarkGray),
            ))),
            inner,
        );
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(inner);

    // File list
    let mut left: Vec<Line<'static>> = Vec::new();
    for (i, f) in view.files.iter().enumerate() {
        let is_sel = i == view.selected;
        let style = if is_sel {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        left.push(Line::from(vec![
            Span::styled(format!(" {}", f.path), style),
            Span::styled(format!(" +{} ", f.added), Style::default().fg(Color::Green)),
            Span::styled(format!("-{}", f.removed), Style::default().fg(Color::Red)),
        ]));
    }
    f.render_widget(Paragraph::new(left).wrap(Wrap { trim: false }), chunks[0]);

    // Hunks for the selected file
    let mut right: Vec<Line<'static>> = Vec::new();
    if let Some(file) = view.files.get(view.selected) {
        for hunk in &file.hunks {
            right.push(Line::from(Span::styled(
                hunk.header.clone(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            for line in &hunk.lines {
                let (prefix, color) = match line.kind {
                    DiffLineKind::Context => (" ", Color::Gray),
                    DiffLineKind::Added => ("+", Color::Green),
                    DiffLineKind::Removed => ("-", Color::Red),
                };
                right.push(Line::from(Span::styled(
                    format!("{prefix}{}", line.text),
                    Style::default().fg(color),
                )));
            }
        }
    }
    let height = chunks[1].height as usize;
    let total = right.len();
    let max_scroll = total.saturating_sub(height);
    let scroll = view.scroll.min(max_scroll);
    let window: Vec<Line<'static>> = right.into_iter().skip(scroll).take(height).collect();
    f.render_widget(Paragraph::new(window).wrap(Wrap { trim: false }), chunks[1]);
}
