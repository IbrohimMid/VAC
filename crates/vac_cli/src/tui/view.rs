//! View Module

use crate::tui::app::AppState;
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
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(f.area());

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .split(main_chunks[0]);

    // Render messages
    render_messages(f, state, left_chunks[0]);

    // Render input
    render_input(f, state, left_chunks[1]);

    // Render status bar
    render_status(f, state, left_chunks[2]);

    // Render side panel
    render_side_panel(f, state, main_chunks[1]);

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

    if state.show_file_changes_popup {
        render_file_changes_popup(f, state);
    }

    if state.review_open {
        render_review_workstation(f, state);
    }
}

fn render_messages(f: &mut Frame, state: &mut AppState, area: Rect) {
    use crate::tui::services::message::{render_user_message, render_assistant_message_with_width};
    use crate::tui::services::message::render_tool_call_pending;

    let width = area.width.saturating_sub(2) as usize; // account for border
    let mut lines: Vec<Line> = Vec::new();

    for msg in &state.messages {
        match msg.role.as_str() {
            "user" => {
                lines.extend(render_user_message(&msg.content, width));
            }
            "assistant" => {
                lines.extend(render_assistant_message_with_width(&msg.content, width));
            }
            _ => {
                lines.extend(msg.content.lines().map(|l| Line::raw(l.to_string())));
            }
        }
        lines.push(Line::raw("")); // spacing between messages
    }

    // Render pending tool calls from state
    for tc in &state.pending_tool_calls {
        lines.extend(render_tool_call_pending(tc));
    }

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Messages"))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
}

fn render_input(f: &mut Frame, state: &mut AppState, area: Rect) {
    let mut lines = Vec::new();
    if state.input.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type your message... (Ctrl+P for commands)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for line in &state.input.lines {
            lines.push(Line::raw(line.as_str()));
        }
    }

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Input"))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);

    if !state.input.is_empty()
        && !state.show_command_palette
        && !state.is_dialog_open
        && !state.show_shortcuts
    {
        let (row, col) = state.input.cursor;
        let cy = area.y + 1 + (row as u16).min(area.height.saturating_sub(3));
        let cx = area.x + 1 + (col as u16).min(area.width.saturating_sub(3));
        f.set_cursor_position((cx, cy));
    }
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
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(80), Constraint::Percentage(20)])
        .split(area);

    // Tool call info
    let mut lines = Vec::new();
    
    // Approval Queue View
    let queue_info = format!("Queue: {} pending | {} approved | {} rejected", 
        state.pending_tool_calls.len(), state.approved_tools.len(), state.rejected_tools.len());
    lines.push(Line::styled(queue_info, Style::default().fg(Color::Cyan)));
    lines.push(Line::from(""));

    // Permission Mode & Explainability
    if state.auto_approve {
        lines.push(Line::styled("Permission Mode: AUTO-APPROVE (Ctrl+A to toggle)", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));
    } else {
        lines.push(Line::styled("Permission Mode: MANUAL (Ctrl+A to toggle)", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
    }
    
    if let Some(reason) = &state.permission_explanation {
        lines.push(Line::from(""));
        lines.push(Line::styled("Why is permission needed?", Style::default().add_modifier(Modifier::BOLD)));
        lines.push(Line::styled(reason, Style::default().fg(Color::DarkGray)));
    }
    lines.push(Line::from(""));

    if let Some(tc) = &state.dialog_command {
        lines.push(Line::from(vec![
            Span::styled("Tool: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(tc.function.name.clone(), Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::from(""));

        let args = &tc.function.arguments;
        if tc.function.name == "SearchReplace" {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                let file_path = v.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                let old_str = v.get("old_str").and_then(|v| v.as_str()).unwrap_or("");
                let new_str = v.get("new_str").and_then(|v| v.as_str()).unwrap_or("");
                lines.extend(crate::tui::services::file_diff::preview_file_diff(
                    file_path,
                    old_str,
                    new_str,
                    area.width as usize,
                ));
            } else {
                lines.push(Line::from(args.to_string()));
            }
        } else if tc.function.name == "Write" {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                let file_path = v.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                let content = v.get("content").and_then(|v| v.as_str()).unwrap_or("");
                lines.extend(crate::tui::services::file_diff::preview_file_diff(
                    file_path,
                    "",
                    content,
                    area.width as usize,
                ));
            } else {
                lines.push(Line::from(args.to_string()));
            }
        } else {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                let formatted = serde_json::to_string_pretty(&v).unwrap_or_else(|_| args.to_string());
                for line in formatted.lines() {
                    lines.push(Line::from(line.to_string()));
                }
            } else {
                for line in args.lines() {
                    lines.push(Line::from(line.to_string()));
                }
            }
        }
    } else {
        lines.push(Line::from("No tool call"));
    }

    let info = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Approve Tool?"))
        .wrap(Wrap { trim: false });
    f.render_widget(info, chunks[0]);

    // Buttons
    let buttons = vec![
        ("Approve (Enter)", state.dialog_selected == 0),
        ("Reject (r)", state.dialog_selected == 1),
        ("Close & Reject (Esc)", state.dialog_selected == 2),
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

fn render_shortcuts(f: &mut Frame, _state: &mut AppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let shortcuts = vec![
        "Ctrl+P - Command palette",
        "Ctrl+C - Quit",
        "Esc - Cancel/Close",
        "Up/Down - Scroll/Navigate",
        "Enter - Submit/Select",
        "Ctrl+L - Toggle mouse capture",
        "Ctrl+X - Revert selected (Review)",
        "Ctrl+Y - Revert filtered (Review)",
        "Ctrl+Z - Revert all (Review)",
        "Ctrl+N - Open in editor (Review)",
        "PageUp/PageDown - Scroll diff (Review)",
        "Ctrl+G - Open review workstation",
        "Ctrl+O - Toggle auto-approve",
    ];
    let items: Vec<ListItem> = shortcuts
        .iter()
        .map(|s| ListItem::new(Line::raw(*s)))
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Shortcuts (Esc to close)"));
    f.render_widget(list, area);
}

fn render_file_changes_popup(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(60, 60, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search
            Constraint::Min(3),    // Content
            Constraint::Length(1), // Footer
        ])
        .split(area);

    let query = state.file_changes_search.to_lowercase();
    let filtered_files: Vec<_> = state.modified_files
        .iter()
        .filter(|f| query.is_empty() || f.to_lowercase().contains(&query))
        .collect();

    // Search input
    let search_text = if state.file_changes_search.is_empty() {
        vec![
            Span::styled(">", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled("Type to filter...", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled(">", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(&state.file_changes_search, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("|", Style::default().fg(Color::Cyan)),
        ]
    };

    let search_p = Paragraph::new(Line::from(search_text))
        .block(Block::default().borders(Borders::ALL).title("Modified Files (Review Mode)"));
    f.render_widget(search_p, chunks[0]);

    // Files list
    let mut visible_lines = Vec::new();
    let height = chunks[1].height as usize;
    let total_items = filtered_files.len();
    let scroll = state.file_changes_scroll;

    for i in 0..height {
        let idx = scroll + i;
        if idx >= total_items {
            break;
        }
        let file = filtered_files[idx];
        let is_selected = idx == state.file_changes_selected;
        let style = if is_selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        visible_lines.push(ListItem::new(Line::styled(format!("  {} ", file), style)));
    }

    let list = List::new(visible_lines)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(list, chunks[1]);

    // Footer
    let footer_text = vec![
        Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
        Span::styled(": Navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+x", Style::default().fg(Color::Cyan)),
        Span::styled(": Revert  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+z", Style::default().fg(Color::Cyan)),
        Span::styled(": Revert All  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+n", Style::default().fg(Color::Cyan)),
        Span::styled(": Edit  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(": Close", Style::default().fg(Color::DarkGray)),
    ];
    let footer = Paragraph::new(Line::from(footer_text)).alignment(ratatui::layout::Alignment::Left);
    f.render_widget(footer, chunks[2]);
}

fn render_review_workstation(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(90, 90, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    let header = {
        let title = format!("Review Workstation ({})", state.review_filtered_paths().len());
        let search_text = if state.review_filter.is_empty() {
            vec![
                Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
                Span::styled("type to filter…", Style::default().fg(Color::DarkGray)),
            ]
        } else {
            vec![
                Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
                Span::styled(&state.review_filter, Style::default().add_modifier(Modifier::BOLD)),
            ]
        };
        Paragraph::new(Line::from(search_text))
            .block(Block::default().borders(Borders::ALL).title(title))
    };
    f.render_widget(header, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    let files = state.review_filtered_paths();
    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(idx, path)| {
            let is_selected = idx == state.review_selected_idx;
            let style = if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let (status_span, snap_span) = match state.review_items.get(path) {
                Some(it) => {
                    let status = match it.status {
                        crate::tui::app::ReviewItemStatus::Pending => Span::styled("• ", Style::default().fg(Color::DarkGray)),
                        crate::tui::app::ReviewItemStatus::Restored => Span::styled("✓ ", Style::default().fg(Color::Green)),
                        crate::tui::app::ReviewItemStatus::Failed => Span::styled("! ", Style::default().fg(Color::Red)),
                    };
                    let snap = if it.has_snapshot {
                        Span::styled("S ", Style::default().fg(Color::Cyan))
                    } else {
                        Span::styled("- ", Style::default().fg(Color::DarkGray))
                    };
                    (status, snap)
                }
                None => (
                    Span::styled("• ", Style::default().fg(Color::DarkGray)),
                    Span::styled("? ", Style::default().fg(Color::DarkGray)),
                ),
            };

            ListItem::new(Line::from(vec![
                status_span,
                snap_span,
                Span::styled(path.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Files"));
    f.render_widget(list, body[0]);

    let diff_height = body[1].height.saturating_sub(2) as usize;
    let diff_title = if let Some(path) = &state.review_selected_path {
        format!("Diff: {path}")
    } else {
        "Diff".to_string()
    };

    let diff_lines: Vec<Line> = if let Some(diff) = &state.review_diff {
        if let (Some(old), Some(new)) = (diff.old_content.as_deref(), diff.new_content.as_deref()) {
            crate::tui::services::review::render_diff_viewport(
                old,
                new,
                body[1].width as usize,
                diff.scroll,
                diff_height,
            )
        } else if let Some(err) = &diff.last_error {
            vec![Line::from(Span::styled(err.clone(), Style::default().fg(Color::Red)))]
        } else {
            vec![Line::raw("No diff loaded.")]
        }
    } else if let Some(path) = state.review_selected_path.clone()
        && let Some(it) = state.review_items.get(&path)
        && let Some(err) = &it.last_error
    {
        vec![Line::from(Span::styled(err.clone(), Style::default().fg(Color::Red)))]
    } else {
        vec![Line::raw("Enter: toggle diff • PageUp/PageDown: scroll diff")]
    };

    let diff = Paragraph::new(diff_lines)
        .block(Block::default().borders(Borders::ALL).title(diff_title))
        .wrap(Wrap { trim: false });
    f.render_widget(diff, body[1]);

    let footer_text = vec![
        Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
        Span::styled(": Select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::styled(": Diff  ", Style::default().fg(Color::DarkGray)),
        Span::styled("PgUp/PgDn", Style::default().fg(Color::Cyan)),
        Span::styled(": Scroll  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+x", Style::default().fg(Color::Cyan)),
        Span::styled(": Revert selected  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+y", Style::default().fg(Color::Cyan)),
        Span::styled(": Revert filtered  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+z", Style::default().fg(Color::Cyan)),
        Span::styled(": Revert all  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+n", Style::default().fg(Color::Cyan)),
        Span::styled(": Edit  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(": Close", Style::default().fg(Color::DarkGray)),
    ];
    let footer = Paragraph::new(Line::from(footer_text)).alignment(ratatui::layout::Alignment::Left);
    f.render_widget(footer, chunks[2]);
}

fn render_side_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let mut lines = Vec::new();
    
    // Add run-state metadata
    lines.push(Line::from(vec![
        Span::styled("Session ID: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(&state.session_id[..8]),
    ]));
    
    if let Some(title) = &state.session_title {
        lines.push(Line::from(vec![
            Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(title),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Active Tools:", Style::default().add_modifier(Modifier::BOLD))));
    
    if state.pending_tool_calls.is_empty() && state.approved_tools.is_empty() {
        lines.push(Line::from(Span::styled("  None", Style::default().fg(Color::DarkGray))));
    } else {
        for call in &state.pending_tool_calls {
            lines.push(Line::from(vec![
                Span::styled("  [?] ", Style::default().fg(Color::Yellow)),
                Span::raw(&call.function.name),
            ]));
        }
        for call in &state.approved_tools {
            lines.push(Line::from(vec![
                Span::styled("  [✓] ", Style::default().fg(Color::Green)),
                Span::raw(&call.function.name),
            ]));
        }
    }

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Status & Tools"));
    f.render_widget(widget, area);
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
