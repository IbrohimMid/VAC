use crate::{app::AppState, app::CommandSource, services::detect_term::ThemeColors};
use nucleo_matcher::{
    Config, Matcher,
    pattern::{CaseMatching, Normalization, Pattern},
};
use std::cmp::Reverse;

pub fn filter_helpers_sync(state: &mut AppState) {
    let input = state.input.lines.join("");
    if !input.starts_with('/') {
        state.filtered_helpers.clear();
        state.show_helper_dropdown = false;
        return;
    }

    let query = input.trim_start_matches('/');
    if query.is_empty() {
        // Sort by recency/frequency when query is empty
        let mut cmds = state.commands.clone();
        cmds.sort_by_key(|c| {
            let freq = state
                .recent_commands
                .frequencies
                .get(&c.command)
                .copied()
                .unwrap_or(0);
            let recent_idx = state
                .recent_commands
                .history
                .iter()
                .position(|h| h == &c.command)
                .unwrap_or(usize::MAX);
            (Reverse(freq), recent_idx)
        });
        state.filtered_helpers = cmds;
        return;
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

    let mut matches = Vec::new();
    let mut buf = Vec::new();
    for cmd in &state.commands {
        let text = format!("{} {}", cmd.command, cmd.description);
        let utf32 = nucleo_matcher::Utf32Str::new(&text, &mut buf);
        if let Some(score) = pattern.score(utf32, &mut matcher) {
            matches.push((score, cmd.clone()));
        }
    }

    matches.sort_by_key(|(score, cmd)| {
        let freq = state
            .recent_commands
            .frequencies
            .get(&cmd.command)
            .copied()
            .unwrap_or(0);
        (Reverse(*score), Reverse(freq))
    });

    state.filtered_helpers = matches.into_iter().map(|(_, cmd)| cmd).collect();
}
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

pub fn render_helper_dropdown(f: &mut Frame, state: &AppState, dropdown_area: Rect) {
    let input = state.input.lines.join("\n");
    let input = input.trim();
    let show = input.starts_with('/') && !state.filtered_helpers.is_empty();
    if state.show_helper_dropdown && show {
        // filtered_helpers is maintained synchronously by filter_helpers_sync():
        // - When input is just "/", it contains all commands
        // - When input is "/foo", it contains only matching commands
        let commands_to_show = &state.filtered_helpers;

        if commands_to_show.is_empty() {
            return;
        }

        let total_commands = commands_to_show.len();
        const MAX_VISIBLE_ITEMS: usize = 5;
        let visible_height = MAX_VISIBLE_ITEMS.min(total_commands);

        // Create a compact area for the dropdown (matching view.rs calculation)
        let has_content_above = state.helper_scroll > 0;
        let has_content_below = state.helper_scroll < total_commands.saturating_sub(visible_height);
        let arrow_lines =
            if has_content_above { 1 } else { 0 } + if has_content_below { 1 } else { 0 };
        let counter_line = if has_content_above || has_content_below {
            1
        } else {
            0
        };
        let compact_height = (visible_height + arrow_lines + counter_line) as u16;

        let compact_area = Rect {
            x: dropdown_area.x,
            y: dropdown_area.y,
            width: dropdown_area.width,
            height: compact_height,
        };

        // Calculate scroll position
        let max_scroll = total_commands.saturating_sub(visible_height);
        let scroll = if state.helper_scroll > max_scroll {
            max_scroll
        } else {
            state.helper_scroll
        };

        // Find the longest command name to calculate padding
        let max_command_length = commands_to_show
            .iter()
            .map(|h| h.command.len())
            .max()
            .unwrap_or(0);

        // Dropdown colors - use explicit background for visibility
        let dropdown_bg = Color::Rgb(40, 40, 40);
        let dropdown_text = Color::White;
        let dropdown_muted = Color::Gray;
        let highlight_bg = Color::Cyan;
        let highlight_fg = Color::Black;

        // Create visible lines with scroll indicators
        let mut visible_lines = Vec::new();

        // Add top arrow indicator if there are hidden items above
        let has_content_above = scroll > 0;
        if has_content_above {
            visible_lines.push(Line::from(vec![Span::styled(
                " ▲",
                Style::default().fg(dropdown_muted).bg(dropdown_bg),
            )]));
        }

        // Create exactly the number of visible lines (no extra spacing)
        for i in 0..visible_height {
            let line_index = scroll + i;
            if line_index < total_commands {
                let command = &commands_to_show[line_index];
                let padding_needed = max_command_length - command.command.len();
                let padding = " ".repeat(padding_needed);
                let is_selected = line_index == state.helper_selected;

                let command_style = if is_selected {
                    Style::default().fg(highlight_fg).bg(highlight_bg)
                } else {
                    Style::default().fg(ThemeColors::cyan()).bg(dropdown_bg)
                };

                let description_style = if is_selected {
                    Style::default().fg(highlight_fg).bg(highlight_bg)
                } else {
                    Style::default().fg(dropdown_text).bg(dropdown_bg)
                };

                let padding_style = if is_selected {
                    Style::default().fg(highlight_fg).bg(highlight_bg)
                } else {
                    Style::default().fg(dropdown_muted).bg(dropdown_bg)
                };

                let description_text = if matches!(command.source, CommandSource::Custom { .. }) {
                    format!(" – [custom] {}", command.description)
                } else {
                    format!(" – {}", command.description)
                };

                let spans = vec![
                    Span::styled(format!("  {}  ", command.command), command_style),
                    Span::styled(padding, padding_style),
                    Span::styled(description_text, description_style),
                ];

                visible_lines.push(Line::from(spans));
            } else {
                visible_lines.push(Line::from(""));
            }
        }

        // Add bottom arrow indicator if there are hidden items below
        if has_content_below {
            visible_lines.push(Line::from(vec![Span::styled(
                " ▼",
                Style::default().fg(dropdown_muted).bg(dropdown_bg),
            )]));
        }

        // Calculate current selected item position (1-based)
        let current_position = state.helper_selected + 1;

        // Create navigation indicators
        let mut indicator_spans = vec![];

        if has_content_above || has_content_below {
            // Show current position counter
            indicator_spans.push(Span::styled(
                format!(" ({}/{})", current_position, total_commands),
                Style::default().fg(dropdown_muted).bg(dropdown_bg),
            ));
        }

        // Add counter as a separate line if needed
        if !indicator_spans.is_empty() {
            visible_lines.push(Line::from(indicator_spans));
        }

        // Render the content using a List widget for more compact display
        let items: Vec<ListItem> = visible_lines.into_iter().map(ListItem::new).collect();

        let list = List::new(items)
            .block(Block::default())
            .style(Style::default().bg(dropdown_bg).fg(dropdown_text));

        f.render_widget(list, compact_area);
    }
}

pub fn render_file_search_dropdown(f: &mut Frame, state: &AppState, area: Rect) {
    if !state.show_helper_dropdown && !state.show_file_search {
        return;
    }
    if state.show_file_search && !state.file_search_results.is_empty() {
        render_file_dropdown(f, state, area);
    } else if state.show_helper_dropdown && !state.filtered_helpers.is_empty() {
        render_helper_dropdown(f, state, area);
    }
}

fn render_file_dropdown(f: &mut Frame, state: &AppState, area: Rect) {
    let files = &state.file_search_results;
    if files.is_empty() {
        return;
    }

    // Set title and styling based on trigger
    let (title, title_color) = ("📁 Files", ThemeColors::cyan());
    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let style = if i == state.file_search_selected_idx {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(ThemeColors::text())
            };

            let display_text = format!("{} {}", get_file_icon(item), item);
            ListItem::new(Line::from(Span::styled(display_text, style)))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.file_search_selected_idx));

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(title_color)),
    );

    f.render_stateful_widget(list, area, &mut list_state);
}

// Helper function to get file icons based on extension
fn get_file_icon(filename: &str) -> &'static str {
    if filename.ends_with('/') {
        return "📁";
    }

    match filename.split('.').next_back() {
        Some("rs") => "🦀",
        Some("toml") => "⚙️",
        Some("md") => "📝",
        Some("txt") => "📄",
        Some("json") => "📋",
        Some("js") | Some("ts") => "🟨",
        Some("py") => "🐍",
        Some("html") => "🌐",
        Some("css") => "🎨",
        Some("yml") | Some("yaml") => "📄",
        Some("lock") => "🔒",
        Some("sh") => "💻",
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") => "🖼️",
        _ => "📄",
    }
}
