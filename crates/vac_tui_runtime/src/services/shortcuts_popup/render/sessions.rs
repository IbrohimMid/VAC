//! Sessions section rendering

use crate::services::detect_term::ThemeColors;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn render_sessions_section(
    f: &mut Frame,
    state: &mut crate::app::AppState,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
) {
    let search_term = &state.command_palette_input;
    let search_prompt = ">";
    let cursor = "|";
    let placeholder = "Type to filter sessions";

    let search_spans = if search_term.is_empty() {
        vec![
            Span::raw(" "),
            Span::styled(search_prompt, Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(cursor, Style::default().fg(ThemeColors::cyan())),
            Span::styled(placeholder, Style::default().fg(ThemeColors::dark_gray())),
        ]
    } else {
        vec![
            Span::raw(" "),
            Span::styled(search_prompt, Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(
                search_term.clone(),
                Style::default()
                    .fg(ThemeColors::text())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::default().fg(ThemeColors::cyan())),
        ]
    };

    f.render_widget(Paragraph::new(Line::from(search_spans)), search_area);

    // Filter sessions by search term
    let search_lower = search_term.to_lowercase();
    let filtered_sessions: Vec<(usize, &crate::app::SessionInfo)> = state
        .sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| search_term.is_empty() || s.title.to_lowercase().contains(&search_lower))
        .collect();

    let total_filtered = filtered_sessions.len();
    let height = content_area.height as usize;

    if filtered_sessions.is_empty() {
        let empty_message = if state.sessions.is_empty() {
            " No sessions available"
        } else {
            " No sessions match your search"
        };
        let empty_widget = Paragraph::new(Line::from(vec![Span::styled(
            empty_message,
            Style::default().fg(ThemeColors::dark_gray()),
        )]));
        f.render_widget(empty_widget, content_area);
        f.render_widget(Paragraph::new(""), scroll_area);
    } else {
        let selected_in_filtered = state
            .sessions_selected_idx
            .min(total_filtered.saturating_sub(1));

        let max_scroll = total_filtered.saturating_sub(height);
        let scroll = if selected_in_filtered >= height {
            (selected_in_filtered - height + 1).min(max_scroll)
        } else {
            0
        };

        let visible_end = (scroll + height).min(total_filtered);

        let has_content_above = scroll > 0;
        let has_content_below = visible_end < total_filtered;

        let mut visible_lines: Vec<Line> = Vec::new();

        if has_content_above {
            visible_lines.push(Line::from(vec![Span::styled(
                " ▲",
                Style::default().fg(ThemeColors::dark_gray()),
            )]));
        }

        for (_filtered_idx, (original_idx, session)) in filtered_sessions
            .iter()
            .enumerate()
            .skip(scroll)
            .take(height)
        {
            let formatted_datetime = if let Ok(dt) =
                chrono::DateTime::parse_from_rfc3339(&session.updated_at.replace(" UTC", "+00:00"))
            {
                dt.format("%Y-%m-%d %H:%M").to_string()
            } else {
                let parts = session.updated_at.split('T').collect::<Vec<_>>();
                let date = parts.first().unwrap_or(&"");
                let time = parts.get(1).and_then(|t| t.split('.').next()).unwrap_or("");
                format!("{} {}", date, time)
            };

            let text = format!(" {} . {}", formatted_datetime, session.title);
            let is_selected = *original_idx == state.sessions_selected_idx;

            let (fg, bg) = if is_selected {
                (ThemeColors::highlight_fg(), ThemeColors::highlight_bg())
            } else {
                (ratatui::style::Color::White, ratatui::style::Color::Reset)
            };

            let style = if is_selected {
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(fg).bg(bg)
            };

            visible_lines.push(Line::from(vec![Span::styled(text, style)]));
        }

        let content_paragraph = Paragraph::new(visible_lines);
        f.render_widget(content_paragraph, content_area);

        if has_content_above || has_content_below {
            let mut indicator_spans = vec![];

            indicator_spans.push(Span::styled(
                format!(" ({}/{})", visible_end, total_filtered),
                Style::default(),
            ));

            if has_content_below {
                indicator_spans.push(Span::styled(
                    " ▼",
                    Style::default().fg(ThemeColors::dark_gray()),
                ));
            }

            let indicator_paragraph = Paragraph::new(Line::from(indicator_spans));
            f.render_widget(indicator_paragraph, scroll_area);
        }
    }

    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑/↓", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" navigate", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("enter", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" select", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("tab", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" switch", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("esc", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" close", Style::default().fg(ThemeColors::cyan())),
    ]));

    f.render_widget(help, help_area);
}
