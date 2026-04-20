//! Input and paste tray rendering

use crate::app::AppState;
use crate::ui::style::focus_style;
use crate::app::WorkspaceFocus;
use crate::services::clipboard_paste::{PastedKind, kind_badge, preview_text, size_label, token_estimate};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn render_input(f: &mut Frame, state: &mut AppState, area: Rect) {
    // Split off a tray above the input when there are pending pastes.
    // Unit 5 (Wave 3.1): tray grows to one row per paste (up to 6) when there
    // are any pending pastes, so each card shows kind/size/tokens/preview.
    let tray_rows = if !state.pending_pastes.is_empty() {
        paste_tray_rows(state.pending_pastes.len())
    } else {
        0
    };
    let (tray_area, input_area) = if tray_rows > 0 && area.height >= tray_rows + 2 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(tray_rows), Constraint::Min(2)])
            .split(area);
        (Some(split[0]), split[1])
    } else {
        (None, area)
    };

    if let Some(tray) = tray_area {
        render_paste_tray(f, state, tray);
    }

    let mut lines = Vec::new();
    if state.input.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type your message... (Ctrl+P for commands)",
            state.theme.style(crate::services::theme::StyleKey::Muted),
        )));
    } else {
        for line in &state.input.lines {
            lines.push(Line::raw(line.as_str()));
        }
    }

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Input",
            focus_style(state.focus == WorkspaceFocus::Input),
        )))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, input_area);

    if state.focus == WorkspaceFocus::Input
        && !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::CommandPalette)
        && !state
            .overlay_manager
            .is_active(crate::overlay::OverlayId::Shortcuts)
    {
        let (row, col) = state.input.cursor;
        let cy = input_area.y + 1 + (row as u16).min(input_area.height.saturating_sub(3));
        let cx = input_area.x + 1 + (col as u16).min(input_area.width.saturating_sub(3));
        f.set_cursor_position((cx, cy));
    }
}

/// Height (rows) allocated to the paste tray for `n` pending pastes.
/// One row per paste up to a cap, plus one header row.
pub(crate) fn paste_tray_rows(n: usize) -> u16 {
    // cap visible cards at 6; user can still navigate beyond with j/k.
    let visible = n.min(6) as u16;
    visible + 1
}

pub(super) fn render_paste_tray(f: &mut Frame, state: &AppState, area: Rect) {
    // Header line: paste count + reorder-mode hint + clear hint.
    let mode_hint = if state.pending_paste_reorder_mode {
        Span::styled(
            " [REORDER — J/K swap, r exit]",
            state
                .theme
                .style(crate::services::theme::StyleKey::Warning)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " j/k select, d remove, r reorder, Enter preview",
            state
                .theme
                .style(crate::services::theme::StyleKey::Muted)
                .add_modifier(Modifier::DIM),
        )
    };
    let header = Line::from(vec![
        Span::styled("📎 ", state.theme.style(crate::services::theme::StyleKey::Muted)),
        Span::styled(
            format!("{} attachment(s)", state.pending_pastes.len()),
            state.theme.style(crate::services::theme::StyleKey::Muted),
        ),
        mode_hint,
        Span::styled(
            "  (Ctrl+U clear)",
            state
                .theme
                .style(crate::services::theme::StyleKey::Muted)
                .add_modifier(Modifier::DIM),
        ),
    ]);

    let selected = state
        .pending_paste_selected
        .min(state.pending_pastes.len().saturating_sub(1));

    // Show a sliding window of cards so the selected index is always visible.
    let capacity = (area.height.saturating_sub(1)) as usize;
    let total = state.pending_pastes.len();
    let start = if total <= capacity || selected < capacity {
        0
    } else {
        selected + 1 - capacity
    };
    let end = (start + capacity).min(total);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(end - start + 1);
    lines.push(header);
    for (i, item) in state.pending_pastes[start..end].iter().enumerate() {
        let abs = start + i;
        let is_selected = abs == selected;
        let cursor = if is_selected {
            if state.pending_paste_reorder_mode {
                "»"
            } else {
                ">"
            }
        } else {
            " "
        };
        let badge_style = match &item.kind {
            PastedKind::Text { .. } => state.theme.style(crate::services::theme::StyleKey::Accent),
            PastedKind::Image { .. } => state.theme.style(crate::services::theme::StyleKey::Streaming),
        };
        let row_style = if is_selected {
            state
                .theme
                .style(crate::services::theme::StyleKey::Normal)
                .add_modifier(Modifier::BOLD)
        } else {
            state.theme.style(crate::services::theme::StyleKey::Muted)
        };
        let spans = vec![
            Span::styled(
                format!("{} ", cursor),
                state
                    .theme
                    .style(crate::services::theme::StyleKey::Warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                kind_badge(&item.kind).to_string(),
                badge_style.add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(format!("#{}", item.id), row_style),
            Span::raw(" "),
            Span::styled(size_label(&item.kind), state.theme.style(crate::services::theme::StyleKey::Muted)),
            Span::raw(" "),
            Span::styled(
                format!("~{}tok", token_estimate(&item.kind)),
                state.theme.style(crate::services::theme::StyleKey::Success),
            ),
            Span::raw("  "),
            Span::styled(preview_text(&item.kind), row_style),
        ];
        lines.push(Line::from(spans));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, area);
}
