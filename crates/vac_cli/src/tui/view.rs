//! View Module

use crate::tui::app::{ActivityKind, AppState, WorkbenchTab, WorkspaceFocus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};

/// Main view function
pub fn view(f: &mut Frame, state: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    render_header(f, state, chunks[0]);
    render_workspace(f, state, chunks[1]);
    render_footer(f, state, chunks[2]);

    if state.show_command_palette {
        render_command_palette(f, state);
    }

    if state.show_shortcuts {
        render_shortcuts(f, state);
    }
}

fn render_header(f: &mut Frame, state: &mut AppState, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled("VAC", Style::default().fg(Color::Magenta)));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("session {}", &state.session_id[..8]),
        Style::default().fg(Color::DarkGray),
    ));
    if let Some(title) = &state.session_title {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(title.clone(), Style::default().fg(Color::Cyan)));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!(
            "model {}",
            state
                .current_model
                .as_ref()
                .map(|m| m.name.as_str())
                .unwrap_or("-")
        ),
        Style::default().fg(Color::DarkGray),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        if state.auto_approve {
            "perm AUTO"
        } else {
            "perm MANUAL"
        },
        if state.auto_approve {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        },
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("approvals {}", state.pending_approvals.len()),
        Style::default().fg(Color::Yellow),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("review {}", state.modified_files.len()),
        Style::default().fg(Color::Cyan),
    ));

    let widget = Paragraph::new(Line::from(spans));
    f.render_widget(widget, area);
}

fn render_workspace(f: &mut Frame, state: &mut AppState, area: Rect) {
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(5)])
        .split(body[0]);

    render_messages(f, state, left[0]);
    render_input(f, state, left[1]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(body[1]);

    render_operator_panel(f, state, right[0]);
    render_activity_panel(f, state, right[1]);
    render_workbench_panel(f, state, right[2]);
}

fn focus_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn render_messages(f: &mut Frame, state: &mut AppState, area: Rect) {
    use crate::tui::services::message::render_tool_call_pending;
    use crate::tui::services::message::{render_assistant_message_with_width, render_user_message};

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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    "Conversation",
                    focus_style(state.focus == WorkspaceFocus::Conversation),
                )),
        )
        .wrap(Wrap { trim: false })
        .scroll((state.scroll as u16, 0));
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    "Input",
                    focus_style(state.focus == WorkspaceFocus::Input),
                )),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);

    if state.focus == WorkspaceFocus::Input && !state.show_command_palette && !state.show_shortcuts {
        let (row, col) = state.input.cursor;
        let cy = area.y + 1 + (row as u16).min(area.height.saturating_sub(3));
        let cx = area.x + 1 + (col as u16).min(area.width.saturating_sub(3));
        f.set_cursor_position((cx, cy));
    }
}

fn render_operator_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    if state.loading {
        let spinner = match state.spinner_frame % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            _ => "⠸",
        };
        lines.push(Line::from(vec![
            Span::styled(spinner, Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled("thinking", Style::default().fg(Color::Magenta)),
        ]));
    } else if state.is_streaming {
        lines.push(Line::styled(
            "streaming (Esc to cancel)",
            Style::default().fg(Color::Magenta),
        ));
    } else {
        lines.push(Line::styled(
            "idle",
            Style::default().fg(Color::DarkGray),
        ));
    }

    lines.push(Line::from(vec![
        Span::styled("tools ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", state.pending_tool_calls.len()),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled("  approvals ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", state.pending_approvals.len()),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled("  modified ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", state.modified_files.len()),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    if !state.shell_output.trim().is_empty() {
        let last = state
            .shell_output
            .lines()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        lines.push(Line::raw(""));
        for l in last.lines() {
            lines.push(Line::from(vec![
                Span::styled("shell ", Style::default().fg(Color::DarkGray)),
                Span::raw(l.to_string()),
            ]));
        }
    }

    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled("Operator", Style::default().fg(Color::Cyan))),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}

fn activity_icon(kind: ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Status => "•",
        ActivityKind::Tool => "🔧",
        ActivityKind::Approval => "⚑",
        ActivityKind::Review => "Δ",
        ActivityKind::Session => "⎇",
        ActivityKind::Error => "!",
    }
}

fn render_activity_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;
    let total = state.activity.len();
    let max_visible = height.min(total);
    let start = total.saturating_sub(max_visible + state.activity_scroll);
    let end = (start + max_visible).min(total);

    let mut lines: Vec<Line> = Vec::new();
    for item in &state.activity[start..end] {
        let ts = item.at.format("%H:%M:%S").to_string();
        lines.push(Line::from(vec![
            Span::styled(ts, Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(activity_icon(item.kind), Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::raw(item.message.clone()),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "no activity yet",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let widget = Paragraph::new(lines)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Activity",
                focus_style(state.focus == WorkspaceFocus::Activity),
            )),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}

fn render_workbench_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);

    let tabs = vec![
        format!("Approvals ({})", state.pending_approvals.len()),
        format!("Review ({})", state.modified_files.len()),
        format!("Sessions ({})", state.sessions.len()),
    ];
    let idx = match state.workbench_tab {
        WorkbenchTab::Approvals => 0,
        WorkbenchTab::Review => 1,
        WorkbenchTab::Sessions => 2,
    };

    let tabs = Tabs::new(tabs)
        .select(idx)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Workbench",
            focus_style(state.focus == WorkspaceFocus::Workbench),
        )))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, chunks[0]);

    match state.workbench_tab {
        WorkbenchTab::Approvals => render_approvals_workbench(f, state, chunks[1]),
        WorkbenchTab::Review => render_review_pane(f, state, chunks[1]),
        WorkbenchTab::Sessions => render_sessions_pane(f, state, chunks[1]),
    }
}

fn render_approvals_workbench(f: &mut Frame, state: &mut AppState, area: Rect) {
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let items: Vec<ListItem> = state
        .pending_approvals
        .iter()
        .enumerate()
        .map(|(idx, tc)| {
            let selected = idx == state.approval_selected_idx;
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let id_short = tc.id.chars().take(8).collect::<String>();
            ListItem::new(Line::from(vec![
                Span::styled(id_short, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(tc.function.name.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Pending"));
    f.render_widget(list, body[0]);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(tc) = state.pending_approvals.get(state.approval_selected_idx) {
        lines.push(Line::from(vec![
            Span::styled("Tool: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(tc.function.name.clone(), Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::raw(""));

        if let Some(expl) = state.approval_explanations.get(&tc.id).and_then(|v| v.clone()) {
            lines.push(Line::styled(
                "Explanation",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for l in expl.lines() {
                lines.push(Line::styled(l.to_string(), Style::default().fg(Color::DarkGray)));
            }
            lines.push(Line::raw(""));
        }

        let args = &tc.function.arguments;
        if tc.function.name == "file_edit" {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                let file_path = v.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                let old_str = v.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
                let new_str = v.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
                lines.extend(crate::tui::services::file_diff::preview_file_diff(
                    file_path,
                    old_str,
                    new_str,
                    body[1].width as usize,
                ));
            }
        } else if tc.function.name == "file_write" {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                let file_path = v.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                let content = v.get("content").and_then(|v| v.as_str()).unwrap_or("");
                lines.extend(crate::tui::services::file_diff::preview_file_diff(
                    file_path,
                    "",
                    content,
                    body[1].width as usize,
                ));
            }
        } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
            let formatted = serde_json::to_string_pretty(&v).unwrap_or_else(|_| args.to_string());
            lines.push(Line::styled(
                "Arguments",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for line in formatted.lines() {
                lines.push(Line::raw(line.to_string()));
            }
        } else {
            lines.push(Line::styled(
                "Arguments",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for line in args.lines() {
                lines.push(Line::raw(line.to_string()));
            }
        }
    } else {
        lines.push(Line::styled(
            "No pending approvals",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let detail = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: false })
        .scroll((state.approval_detail_scroll as u16, 0));
    f.render_widget(detail, body[1]);
}

fn render_review_pane(f: &mut Frame, state: &mut AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let title = format!("Review ({})", state.review_filtered_paths().len());
    let filter_line = if state.review_filter.is_empty() {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
            Span::styled("type to filter…", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                &state.review_filter,
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let header = Paragraph::new(filter_line).block(Block::default().borders(Borders::ALL).title(title));
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
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let (status_span, snap_span) = match state.review_items.get(path) {
                Some(it) => {
                    let status = match it.status {
                        crate::tui::app::ReviewItemStatus::Pending => {
                            Span::styled("• ", Style::default().fg(Color::DarkGray))
                        }
                        crate::tui::app::ReviewItemStatus::Restored => {
                            Span::styled("✓ ", Style::default().fg(Color::Green))
                        }
                        crate::tui::app::ReviewItemStatus::Failed => {
                            Span::styled("! ", Style::default().fg(Color::Red))
                        }
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
            vec![Line::from(Span::styled(
                err.clone(),
                Style::default().fg(Color::Red),
            ))]
        } else {
            vec![Line::raw("No diff loaded.")]
        }
    } else if let Some(path) = state.review_selected_path.clone()
        && let Some(it) = state.review_items.get(&path)
        && let Some(err) = &it.last_error
    {
        vec![Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        ))]
    } else {
        vec![Line::raw("Enter: toggle diff • PgUp/PgDn: scroll")]
    };

    let diff = Paragraph::new(diff_lines)
        .block(Block::default().borders(Borders::ALL).title(diff_title))
        .wrap(Wrap { trim: false });
    f.render_widget(diff, body[1]);
}

fn render_sessions_pane(f: &mut Frame, state: &mut AppState, area: Rect) {
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let items: Vec<ListItem> = state
        .sessions
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            let sel = idx == state.sessions_selected_idx;
            let style = if sel {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(&s.updated_at, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(&s.title, style),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Sessions"));
    f.render_widget(list, body[0]);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(sel) = state.sessions.get(state.sessions_selected_idx) {
        lines.push(Line::from(vec![
            Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(sel.title.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(sel.id.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(sel.updated_at.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Checkpoints: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}", sel.checkpoints.len())),
        ]));
        if !sel.checkpoints.is_empty() {
            lines.push(Line::raw(""));
            for cp in sel.checkpoints.iter().take(6) {
                lines.push(Line::raw(cp.clone()));
            }
        }
    } else {
        lines.push(Line::styled(
            "No sessions loaded (/sessions)",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let detail = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: true });
    f.render_widget(detail, body[1]);
}

fn render_footer(f: &mut Frame, state: &mut AppState, area: Rect) {
    let hints: Vec<Span> = match state.focus {
        WorkspaceFocus::Input => vec![
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(": send  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+P", Style::default().fg(Color::Cyan)),
            Span::styled(": commands  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::styled(": next pane  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+Tab", Style::default().fg(Color::Cyan)),
            Span::styled(": next tab", Style::default().fg(Color::DarkGray)),
        ],
        WorkspaceFocus::Conversation => vec![
            Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
            Span::styled(": scroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::styled(": next pane", Style::default().fg(Color::DarkGray)),
        ],
        WorkspaceFocus::Activity => vec![
            Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
            Span::styled(": scroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::styled(": next pane", Style::default().fg(Color::DarkGray)),
        ],
        WorkspaceFocus::Workbench => match state.workbench_tab {
            WorkbenchTab::Approvals => vec![
                Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
                Span::styled(": select  ", Style::default().fg(Color::DarkGray)),
                Span::styled("a", Style::default().fg(Color::Cyan)),
                Span::styled(": approve  ", Style::default().fg(Color::DarkGray)),
                Span::styled("r", Style::default().fg(Color::Cyan)),
                Span::styled(": reject  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Ctrl+Tab", Style::default().fg(Color::Cyan)),
                Span::styled(": next tab  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::styled(": next pane", Style::default().fg(Color::DarkGray)),
            ],
            WorkbenchTab::Review => vec![
                Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
                Span::styled(": select  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(": diff  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Ctrl+x/y/z", Style::default().fg(Color::Cyan)),
                Span::styled(": revert  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Ctrl+n", Style::default().fg(Color::Cyan)),
                Span::styled(": edit  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Ctrl+Tab", Style::default().fg(Color::Cyan)),
                Span::styled(": next tab", Style::default().fg(Color::DarkGray)),
            ],
            WorkbenchTab::Sessions => vec![
                Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
                Span::styled(": select  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(": restore  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Ctrl+Tab", Style::default().fg(Color::Cyan)),
                Span::styled(": next tab", Style::default().fg(Color::DarkGray)),
            ],
        },
    };

    let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
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
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
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

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Commands"));
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
        "Tab - Cycle focus panes",
        "Ctrl+Tab - Cycle workbench tabs",
        "a/r - Approve/Reject selected (Approvals tab)",
    ];
    let items: Vec<ListItem> = shortcuts
        .iter()
        .map(|s| ListItem::new(Line::raw(*s)))
        .collect();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Shortcuts (Esc to close)"),
    );
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
