//! Input and paste tray rendering

use crate::app::AppState;
use crate::app::WorkspaceFocus;
use crate::services::clipboard_paste::{
    PastedKind, kind_badge, preview_text, size_label, token_estimate,
};
use crate::services::theme::StyleKey;
use crate::ui::style::focus_style;
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
    let tray_rows = if !state.layout.paste.pending_pastes.is_empty() {
        paste_tray_rows(state.layout.paste.pending_pastes.len())
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
    if state.composer.input.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type your message... (Ctrl+P for commands)",
            state.core.theme.style(crate::services::theme::StyleKey::Muted),
        )));
    } else {
        for line in &state.composer.input.lines {
            lines.push(Line::raw(line.as_str()));
        }
    }

    // Split off lint issue rows below the input when there are active issues.
    let lint_issues = state.composer.vil_expr_lint.issues();
    let lint_rows = lint_issues.len().min(3) as u16; // cap at 3 visible
    let (actual_input_area, lint_area) = if lint_rows > 0 && input_area.height > lint_rows + 3 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(lint_rows)])
            .split(input_area);
        (split[0], Some(split[1]))
    } else {
        (input_area, None)
    };

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Input",
            focus_style(state.layout.focus == WorkspaceFocus::Input, &state.core.theme),
        )))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, actual_input_area);

    // Render vil-expr lint issues (PR-T12.1).
    if let Some(area) = lint_area {
        let issue_lines: Vec<Line<'_>> = lint_issues
            .iter()
            .take(3)
            .map(|issue| {
                let style = match &issue.severity {
                    vil_expr::Severity::Error => state.core.theme.style(StyleKey::ValidationError),
                    vil_expr::Severity::Warning => state.core.theme.style(StyleKey::ValidationWarning),
                };
                Line::from(Span::styled(
                    format!(
                        "  {} [{}:{}] {}",
                        severity_icon(&issue.severity),
                        issue.line,
                        issue.col,
                        issue.message
                    ),
                    style,
                ))
            })
            .collect();
        f.render_widget(Paragraph::new(issue_lines), area);
    }

    if state.layout.focus == WorkspaceFocus::Input
        && !state
            .layout.overlay_manager
            .is_active(crate::overlay::OverlayId::CommandPalette)
        && !state
            .layout.overlay_manager
            .is_active(crate::overlay::OverlayId::Shortcuts)
    {
        let (row, col) = state.composer.input.cursor;
        let cy =
            actual_input_area.y + 1 + (row as u16).min(actual_input_area.height.saturating_sub(3));
        let cx =
            actual_input_area.x + 1 + (col as u16).min(actual_input_area.width.saturating_sub(3));
        f.set_cursor_position((cx, cy));
    }
}

fn severity_icon(severity: &vil_expr::Severity) -> &'static str {
    match severity {
        vil_expr::Severity::Error => "E",
        vil_expr::Severity::Warning => "W",
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
    let mode_hint = if state.layout.paste.pending_paste_reorder_mode {
        Span::styled(
            " [REORDER — J/K swap, r exit]",
            state
                .core.theme
                .style(crate::services::theme::StyleKey::Warning)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " j/k select, d remove, r reorder, Enter preview",
            state
                .core.theme
                .style(crate::services::theme::StyleKey::Muted)
                .add_modifier(Modifier::DIM),
        )
    };
    let header = Line::from(vec![
        Span::styled(
            "📎 ",
            state.core.theme.style(crate::services::theme::StyleKey::Muted),
        ),
        Span::styled(
            format!("{} attachment(s)", state.layout.paste.pending_pastes.len()),
            state.core.theme.style(crate::services::theme::StyleKey::Muted),
        ),
        mode_hint,
        Span::styled(
            "  (Ctrl+U clear)",
            state
                .core.theme
                .style(crate::services::theme::StyleKey::Muted)
                .add_modifier(Modifier::DIM),
        ),
    ]);

    let selected = state
        .layout.paste.pending_paste_selected
        .min(state.layout.paste.pending_pastes.len().saturating_sub(1));

    // Show a sliding window of cards so the selected index is always visible.
    let capacity = (area.height.saturating_sub(1)) as usize;
    let total = state.layout.paste.pending_pastes.len();
    let start = if total <= capacity || selected < capacity {
        0
    } else {
        selected + 1 - capacity
    };
    let end = (start + capacity).min(total);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(end - start + 1);
    lines.push(header);
    for (i, item) in state.layout.paste.pending_pastes[start..end].iter().enumerate() {
        let abs = start + i;
        let is_selected = abs == selected;
        let cursor = if is_selected {
            if state.layout.paste.pending_paste_reorder_mode {
                "»"
            } else {
                ">"
            }
        } else {
            " "
        };
        let badge_style = match &item.kind {
            PastedKind::Text { .. } => state.core.theme.style(crate::services::theme::StyleKey::Accent),
            PastedKind::Image { .. } => state
                .core.theme
                .style(crate::services::theme::StyleKey::Streaming),
        };
        let row_style = if is_selected {
            state
                .core.theme
                .style(crate::services::theme::StyleKey::Normal)
                .add_modifier(Modifier::BOLD)
        } else {
            state.core.theme.style(crate::services::theme::StyleKey::Muted)
        };
        let spans = vec![
            Span::styled(
                format!("{} ", cursor),
                state
                    .core.theme
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
            Span::styled(
                size_label(&item.kind),
                state.core.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            Span::raw(" "),
            Span::styled(
                format!("~{}tok", token_estimate(&item.kind)),
                state.core.theme.style(crate::services::theme::StyleKey::Success),
            ),
            Span::raw("  "),
            Span::styled(preview_text(&item.kind), row_style),
        ];
        lines.push(Line::from(spans));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, area);
}
