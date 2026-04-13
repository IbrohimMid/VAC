//! View Module

use crate::tui::app::{AppState, HelperCommand, Message};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Main view function
pub fn view(f: &mut Frame, state: &mut AppState) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    // Render messages
    render_messages(f, state, main_chunks[0]);

    // Render input
    render_input(f, state, main_chunks[1]);

    // Render status bar
    render_status(f, state, main_chunks[2]);

    // Render overlays (command palette, dialogs)
    if state.show_command_palette {
        render_command_palette(f, state);
    }

    if state.is_dialog_open {
        render_approval_dialog(f, state);
    }

    if state.show_shortcuts {
        render_shortcuts(f, state);
    }
}

fn render_messages(f: &mut Frame, state: &mut AppState, area: Rect) {
    let lines: Vec<Line> = state
        .messages
        .iter()
        .flat_map(|msg| {
            let prefix = match msg.role.as_str() {
                "user" => "You: ",
                "assistant" => "VAC: ",
                _ => "",
            };
            let color = match msg.role.as_str() {
                "user" => Color::Cyan,
                "assistant" => Color::Green,
                _ => Color::White,
            };
            msg.content
                .lines()
                .enumerate()
                .map(move |(i, line)| {
                    if i == 0 {
                        Line::from(vec![
                            Span::styled(prefix, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                            Span::raw(line),
                        ])
                    } else {
                        Line::raw(line)
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Messages"))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
}

fn render_input(f: &mut Frame, state: &mut AppState, area: Rect) {
    let input_text = if state.input.is_empty() {
        Span::styled("Type your message... (Ctrl+P for commands)", Style::default().fg(Color::DarkGray))
    } else {
        Span::raw(&state.input)
    };
    let widget = Paragraph::new(Line::from(input_text))
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(widget, area);
}

fn render_status(f: &mut Frame, state: &mut AppState, area: Rect) {
    let status_text = if state.loading {
        let spinner = match state.spinner_frame % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            3 => "⠸",
            _ => "⠋",
        };
        format!("{} VAC thinking...", spinner)
    } else if state.is_streaming {
        "📡 VAC streaming... (ESC to cancel)".to_string()
    } else if let Some(title) = &state.session_title {
        format!("VAC | {} | {}", title, &state.session_id[..8])
    } else {
        format!("VAC | Session: {} | Ctrl+C to quit", &state.session_id[..8])
    };
    let widget = Paragraph::new(Line::from(Span::styled(
        status_text,
        Style::default().fg(Color::Magenta),
    )));
    f.render_widget(widget, area);
}

fn render_command_palette(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Input
    let input = Paragraph::new(Line::from(vec![
        Span::styled("/", Style::default().fg(Color::Yellow)),
        Span::raw(&state.command_palette_input),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Command"));
    f.render_widget(input, chunks[0]);

    // Commands list
    let filtered = state.filtered_commands();
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let style = if i == state.command_palette_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(&cmd.command, style),
                Span::raw(" - "),
                Span::styled(&cmd.description, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Commands"));
    f.render_widget(list, chunks[1]);
}

fn render_approval_dialog(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // Tool call info
    let tool_info = if let Some(tc) = &state.dialog_command {
        format!("Tool: {}\nArguments: {}", tc.function.name, tc.function.arguments)
    } else {
        "No tool call".to_string()
    };
    let info = Paragraph::new(tool_info)
        .block(Block::default().borders(Borders::ALL).title("Approve Tool?"));
    f.render_widget(info, chunks[0]);

    // Buttons
    let buttons = vec![
        ("Approve (Enter)", state.dialog_selected == 0),
        ("Reject (r)", state.dialog_selected == 1),
        ("Cancel (Esc)", state.dialog_selected == 2),
    ];
    let button_items: Vec<ListItem> = buttons
        .iter()
        .map(|(text, selected)| {
            let style = if *selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::styled(*text, style))
        })
        .collect();
    let list = List::new(button_items)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(list, chunks[1]);
}

fn render_shortcuts(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let shortcuts = vec![
        "Ctrl+P - Command palette",
        "Ctrl+C - Quit",
        "Esc - Cancel/Close",
        "Up/Down - Scroll/Navigate",
        "Enter - Submit/Select",
        "Ctrl+L - Toggle mouse capture",
    ];
    let items: Vec<ListItem> = shortcuts
        .iter()
        .map(|s| ListItem::new(Line::raw(*s)))
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Shortcuts (Esc to close)"));
    f.render_widget(list, area);
}

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