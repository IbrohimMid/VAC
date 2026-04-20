//! Main rendering for the unified popup

use crate::app::ShortcutsPopupMode;
use crate::services::detect_term::ThemeColors;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

pub mod commands;
pub mod sessions;
pub mod shortcuts;

pub use commands::render_commands_section;
pub use sessions::render_sessions_section;
pub use shortcuts::render_shortcuts_section;

pub fn render_shortcuts_popup(f: &mut Frame, state: &mut crate::app::AppState) {
    // Calculate popup size (60% width, fit height to content)
    let area = centered_rect(60, 80, f.area());

    f.render_widget(ratatui::widgets::Clear, area);

    // Create the main block with border and background
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ThemeColors::cyan()));

    // Split area for title, tabs, and content - layout differs by mode
    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };

    // Render title inside the popup
    let title = " Command Palette";
    let title_style = Style::default()
        .fg(ThemeColors::title())
        .add_modifier(Modifier::BOLD);
    let title_line = Line::from(Span::styled(title, title_style));
    let title_paragraph = Paragraph::new(title_line);

    // Render tabs
    let tab_titles = vec![" Commands ", " Shortcuts ", " Sessions "];
    let selected_tab = match state.shortcuts_mode {
        ShortcutsPopupMode::Commands => 0,
        ShortcutsPopupMode::Shortcuts => 1,
        ShortcutsPopupMode::Sessions => 2,
    };
    let tabs = Tabs::new(tab_titles)
        .select(selected_tab)
        .style(Style::default().fg(ThemeColors::muted()))
        .highlight_style(
            Style::default()
                .fg(ThemeColors::accent())
                .add_modifier(Modifier::BOLD),
        )
        .divider(" | ");

    // Render content based on mode with mode-specific layouts
    match state.shortcuts_mode {
        ShortcutsPopupMode::Commands => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Title
                    Constraint::Length(1), // Tabs
                    Constraint::Length(3), // Search
                    Constraint::Min(3),    // Content
                    Constraint::Length(1), // Scroll indicators
                    Constraint::Length(1), // Help text
                ])
                .split(inner_area);

            f.render_widget(title_paragraph, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            render_commands_section(f, state, chunks[2], chunks[3], chunks[4], chunks[5], area);
        }
        ShortcutsPopupMode::Shortcuts => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Title
                    Constraint::Length(1), // Tabs
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Search
                    Constraint::Min(3),    // Content
                    Constraint::Length(1), // Scroll indicators
                    Constraint::Length(1), // Help text
                ])
                .split(inner_area);

            f.render_widget(title_paragraph, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            render_shortcuts_section(f, state, chunks[3], chunks[4], chunks[5], chunks[6], area);
        }
        ShortcutsPopupMode::Sessions => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Title
                    Constraint::Length(1), // Tabs
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Search
                    Constraint::Length(1), // Spacer between search and list
                    Constraint::Min(3),    // Content (sessions list)
                    Constraint::Length(1), // Scroll indicators
                    Constraint::Length(1), // Help text
                ])
                .split(inner_area);

            f.render_widget(title_paragraph, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            render_sessions_section(f, state, chunks[3], chunks[5], chunks[6], chunks[7]);
        }
    }

    // Render the border with title last (so it's on top)
    f.render_widget(block, area);
}

/// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
