//! Shortcuts section rendering

use crate::constants::SCROLL_BUFFER_LINES;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use super::super::catalog::{
    Shortcut, build_shortcuts_content, get_all_shortcuts, get_shortcuts_count,
};

pub fn render_shortcuts_section(
    f: &mut Frame,
    state: &mut crate::app::AppState,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
    area: Rect,
) {
    // Render search input
    let search_term = &state.layout.command_palette.input;
    let search_prompt = ">";
    let cursor = "|";
    let placeholder = "Type to filter (e.g. 'ctrl+')";

    let search_spans = if search_term.is_empty() {
        vec![
            Span::raw(" "),
            Span::styled(search_prompt, state.core.theme.style(StyleKey::AppTitle)),
            Span::raw(" "),
            Span::styled(cursor, state.core.theme.style(StyleKey::Accent)),
            Span::styled(placeholder, state.core.theme.style(StyleKey::Muted)),
            Span::raw(" "),
        ]
    } else {
        vec![
            Span::raw(" "),
            Span::styled(search_prompt, state.core.theme.style(StyleKey::AppTitle)),
            Span::raw(" "),
            Span::styled(search_term, state.core.theme.style(StyleKey::Text)),
            Span::styled(cursor, state.core.theme.style(StyleKey::Accent)),
        ]
    };

    f.render_widget(
        Paragraph::new(Line::from(search_spans))
            .block(Block::default().border_style(state.core.theme.style(StyleKey::Muted))),
        search_area,
    );

    // Get shortcuts content (filtered or freshly built)
    let search_lower = search_term.to_lowercase();
    let all_lines = if search_term.is_empty() {
        build_shortcuts_content(&state.core.theme, Some(area.width as usize))
    } else {
        build_filtered_shortcuts(&search_lower, area, state)
    };

    let total_lines = all_lines.len();
    let height = content_area.height as usize;
    let keybind_style = state.core.theme.style(StyleKey::KeybindBadge);
    let shortcuts_count = count_shortcuts(&all_lines, search_term, keybind_style);

    // Calculate scroll position
    let max_scroll = total_lines.saturating_sub(height.saturating_sub(SCROLL_BUFFER_LINES));

    state.layout.command_palette.shortcuts_scroll = state.layout.command_palette.shortcuts_scroll.min(max_scroll);
    let scroll = state.layout.command_palette.shortcuts_scroll;

    // Add top arrow indicator if there are hidden items above
    let mut visible_lines = Vec::new();
    let has_content_above = scroll > 0;
    if has_content_above {
        visible_lines.push(Line::from(vec![Span::styled(" ▲", Style::default())]));
    }

    // Create visible lines
    for i in 0..height {
        let line_index = scroll + i;
        if line_index < all_lines.len() {
            visible_lines.push(all_lines[line_index].clone());
        } else {
            visible_lines.push(Line::from(""));
        }
    }

    let content_paragraph = Paragraph::new(visible_lines)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .style(Style::default());

    f.render_widget(content_paragraph, content_area);

    // Calculate cumulative shortcuts count
    let cumulative_shortcuts_count =
        count_cumulative_shortcuts(&all_lines, scroll, height, keybind_style);

    // Scroll indicators
    let has_content_above = scroll > 0;
    let has_content_below = scroll < max_scroll;

    if has_content_above || has_content_below {
        let mut indicator_spans = vec![];

        indicator_spans.push(Span::styled(
            format!(" ({}/{})", cumulative_shortcuts_count, shortcuts_count),
            Style::default(),
        ));

        if has_content_below {
            indicator_spans.push(Span::styled(" ▼", state.core.theme.style(StyleKey::Muted)));
        }

        let indicator_paragraph = Paragraph::new(Line::from(indicator_spans));
        f.render_widget(indicator_paragraph, scroll_area);
    } else {
        f.render_widget(Paragraph::new(""), scroll_area);
    }

    // Help text
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑/↓", state.core.theme.style(StyleKey::Muted)),
        Span::styled(" scroll", state.core.theme.style(StyleKey::Accent)),
        Span::raw("  "),
        Span::styled("tab", state.core.theme.style(StyleKey::Muted)),
        Span::styled(" switch", state.core.theme.style(StyleKey::Accent)),
        Span::raw("  "),
        Span::styled("esc", state.core.theme.style(StyleKey::Muted)),
        Span::styled(" close", state.core.theme.style(StyleKey::Accent)),
    ]));

    f.render_widget(help, help_area);
}

/// Build filtered shortcuts lines for dynamic search results
fn build_filtered_shortcuts(
    search_lower: &str,
    area: Rect,
    state: &crate::app::AppState,
) -> Vec<Line<'static>> {
    let all_shortcuts = get_all_shortcuts();
    let filtered: Vec<&Shortcut> = all_shortcuts
        .iter()
        .filter(|s| {
            s.key.to_lowercase().contains(search_lower)
                || s.description.to_lowercase().contains(search_lower)
                || s.category.to_lowercase().contains(search_lower)
        })
        .collect();

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    let mut categories: std::collections::HashMap<&str, Vec<&Shortcut>> =
        std::collections::HashMap::new();
    for s in filtered {
        categories.entry(&s.category).or_default().push(s);
    }

    let category_order = vec![
        "Navigation",
        "Text Input",
        "Tool Management",
        "UI Controls",
        "Commands",
        "File Search",
        "Mouse",
    ];

    for category_name in &category_order {
        if let Some(category_shortcuts) = categories.get(category_name) {
            let category_style = state
                .core.theme
                .style(StyleKey::CategoryHeader)
                .add_modifier(Modifier::BOLD);
            let category_width = area.width.saturating_sub(category_name.len() as u16 + 5) as usize;
            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", category_name), category_style),
                Span::styled(
                    "─".repeat(category_width).to_string(),
                    state.core.theme.style(StyleKey::Muted),
                ),
            ]));

            for shortcut in category_shortcuts {
                let key_formatted = format!(" {:<25}", shortcut.key);
                let description_formatted = format!("{:<40} ", shortcut.description);

                lines.push(Line::from(vec![
                    Span::styled(
                        key_formatted,
                        state
                            .core.theme
                            .style(StyleKey::KeybindBadge)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(description_formatted, state.core.theme.style(StyleKey::Text)),
                ]));
            }
            lines.push(Line::from(""));
        }
    }
    lines
}

/// Count the total number of shortcuts in displayed lines
fn count_shortcuts(all_lines: &[Line], search_term: &str, keybind_style: Style) -> usize {
    if search_term.is_empty() {
        get_shortcuts_count()
    } else {
        let keybind_fg = keybind_style.fg;
        let mut count = 0;
        for line in all_lines {
            for span in &line.spans {
                if span.style.fg == keybind_fg {
                    count += 1;
                    break;
                }
            }
        }
        count
    }
}

/// Count shortcuts from beginning up to the current scroll position + visible area
fn count_cumulative_shortcuts(
    all_lines: &[Line],
    scroll: usize,
    height: usize,
    keybind_style: Style,
) -> usize {
    let keybind_fg = keybind_style.fg;
    let mut count = 0;
    for line_index in 0..=(scroll + height).min(all_lines.len().saturating_sub(1)) {
        if line_index < all_lines.len() {
            let line = &all_lines[line_index];
            for span in &line.spans {
                if span.style.fg == keybind_fg && span.style.add_modifier.contains(Modifier::BOLD) {
                    count += 1;
                    break;
                }
            }
        }
    }
    count
}
