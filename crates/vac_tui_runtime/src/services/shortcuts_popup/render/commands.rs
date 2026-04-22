//! Commands section rendering

use crate::constants::SCROLL_BUFFER_LINES;
use crate::services::theme::StyleKey;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::super::search::filter_commands;

pub fn render_commands_section(
    f: &mut Frame,
    state: &crate::app::AppState,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
    area: Rect,
) {
    // Render search input
    let search_prompt = ">";
    let cursor = "|";
    let placeholder = "Type to filter";

    let search_spans = if state.command_palette_input.is_empty() {
        vec![
            Span::raw(" "), // Small space before
            Span::styled(search_prompt, state.theme.style(StyleKey::AppTitle)),
            Span::raw(" "),
            Span::styled(cursor, state.theme.style(StyleKey::Accent)),
            Span::styled(placeholder, state.theme.style(StyleKey::Muted)),
            Span::raw(" "), // Small space after
        ]
    } else {
        vec![
            Span::raw(" "), // Small space before
            Span::styled(search_prompt, state.theme.style(StyleKey::AppTitle)),
            Span::raw(" "),
            Span::styled(
                &state.command_palette_input,
                state
                    .theme
                    .style(StyleKey::Text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, state.theme.style(StyleKey::Accent)),
            Span::raw(" "), // Small space after
        ]
    };

    let search_text = ratatui::text::Text::from(vec![
        Line::from(""), // Empty line above
        Line::from(search_spans),
        Line::from(""), // Empty line below
    ]);
    let search_paragraph = Paragraph::new(search_text);
    f.render_widget(search_paragraph, search_area);

    // Get filtered commands
    let filtered_commands = filter_commands(&state.command_palette_input, state);
    let total_commands = filtered_commands.len();
    let height = content_area.height as usize;

    // Calculate scroll position
    let max_scroll = total_commands.saturating_sub(height.saturating_sub(SCROLL_BUFFER_LINES));
    let scroll = if state.command_palette_scroll > max_scroll {
        max_scroll
    } else {
        state.command_palette_scroll
    };

    // Add top arrow indicator if there are hidden items above
    let mut visible_lines = Vec::new();
    let has_content_above = scroll > 0;
    if has_content_above {
        visible_lines.push(Line::from(vec![Span::styled(" ▲", Style::default())]));
    }

    // Create visible lines
    for i in 0..height {
        let line_index = scroll + i;
        if line_index < total_commands {
            let command = &filtered_commands[line_index];
            let available_width = area.width as usize - 2; // Account for borders
            let is_selected = line_index == state.command_palette_selected;
            let bg_color = if is_selected {
                state
                    .theme
                    .style(StyleKey::HighlightBg)
                    .fg
                    .unwrap_or(Color::Reset)
            } else {
                Color::Reset
            };
            let text_color = if is_selected {
                state
                    .theme
                    .style(StyleKey::HighlightFg)
                    .fg
                    .unwrap_or(Color::Reset)
            } else {
                state
                    .theme
                    .style(StyleKey::Text)
                    .fg
                    .unwrap_or(Color::Reset)
            };

            let name_formatted = format!(
                " {:<width$}",
                command.name,
                width = available_width.saturating_sub(command.shortcut.len() + 2)
            );
            let shortcut_formatted = format!("{} ", command.shortcut);

            let shortcut_fg = if is_selected {
                state
                    .theme
                    .style(StyleKey::HighlightFg)
                    .fg
                    .unwrap_or(Color::Reset)
            } else {
                state
                    .theme
                    .style(StyleKey::Muted)
                    .fg
                    .unwrap_or(Color::Reset)
            };

            let spans = vec![
                Span::styled(name_formatted, Style::default().fg(text_color).bg(bg_color)),
                Span::styled(
                    shortcut_formatted,
                    Style::default().fg(shortcut_fg).bg(bg_color),
                ),
            ];

            visible_lines.push(Line::from(spans));
        } else {
            visible_lines.push(Line::from(""));
        }
    }

    // Render content
    let content_paragraph = Paragraph::new(visible_lines)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .style(Style::default());

    f.render_widget(content_paragraph, content_area);

    // Scroll indicators
    let has_content_below = scroll < max_scroll;

    if has_content_above || has_content_below {
        let mut indicator_spans = vec![];

        let cumulative = (scroll + height).min(total_commands);
        indicator_spans.push(Span::styled(
            format!(" ({}/{})", cumulative, total_commands),
            Style::default(),
        ));

        if has_content_below {
            indicator_spans.push(Span::styled(
                " ▼",
                state.theme.style(StyleKey::Muted),
            ));
        }

        let indicator_paragraph = Paragraph::new(Line::from(indicator_spans));
        f.render_widget(indicator_paragraph, scroll_area);
    } else {
        f.render_widget(Paragraph::new(""), scroll_area);
    }

    // Help text
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑/↓", state.theme.style(StyleKey::Muted)),
        Span::styled(" navigate", state.theme.style(StyleKey::Accent)),
        Span::raw("  "),
        Span::styled("enter", state.theme.style(StyleKey::Muted)),
        Span::styled(" select", state.theme.style(StyleKey::Accent)),
        Span::raw("  "),
        Span::styled("tab", state.theme.style(StyleKey::Muted)),
        Span::styled(" switch", state.theme.style(StyleKey::Accent)),
        Span::raw("  "),
        Span::styled("esc", state.theme.style(StyleKey::Muted)),
        Span::styled(" close", state.theme.style(StyleKey::Accent)),
    ]));

    f.render_widget(help, help_area);
}
