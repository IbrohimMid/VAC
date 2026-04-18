//! View Module

use crate::tui::app::{ActivityKind, AppState, WorkbenchTab, WorkspaceFocus};
use crate::tui::services::ToastStyle;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};

/// Main view function
pub fn view(f: &mut Frame, state: &mut AppState) {
    let banner_h = crate::tui::services::banner::banner_height(state);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(banner_h),
            Constraint::Min(1),
            Constraint::Length(1), // statusline
            Constraint::Length(1), // footer
        ])
        .split(f.area());

    render_header(f, state, chunks[0]);
    if banner_h > 0 {
        crate::tui::services::banner::render_banner(f, chunks[1], state);
    } else {
        state.banner_click_regions.clear();
        state.banner_dismiss_region = None;
    }
    render_workspace(f, state, chunks[2]);
    crate::tui::services::statusline::render_statusline(f, state, chunks[3]);
    render_footer(f, state, chunks[4]);

    if state.show_command_palette {
        render_command_palette(f, state);
    }

    if state.show_shortcuts {
        render_shortcuts(f, state);
    }

    if state.show_isolation_switcher {
        crate::tui::services::isolation_switcher::render_isolation_switcher(f, state);
    }

    if state.show_profile_switcher {
        crate::tui::services::profile_switcher::render_profile_switcher(f, state);
    }

    if state.show_rulebook_switcher {
        crate::tui::services::rulebook_switcher::render_rulebook_switcher(f, state);
    }

    if state.show_message_action_popup {
        crate::tui::services::message_action_popup::render_message_action_popup(f, state);
    }

    if state.show_model_switcher {
        render_model_switcher(f, state);
    }

    if state.show_file_search {
        render_file_search(f, state);
    }

    if state.show_changeset {
        render_changeset(f, state);
    }

    if state.show_file_changes_popup {
        crate::tui::services::file_changes_popup::render_file_changes_popup(f, state);
    }

    if state.plan_review_open {
        crate::tui::services::plan_review::render_plan_review(f, state);
    }

    if state.show_ask_user_popup {
        crate::tui::services::ask_user::render_ask_user_popup(f, state);
    }

    if state.shell_popup_visible {
        render_shell_popup(f, state);
    }

    if !state.toasts.is_empty() {
        render_toast(f, state);
    }

    if state.show_helper_dropdown {
        let area = f.area();
        let width = (area.width / 2).max(40).min(area.width.saturating_sub(2));
        let count = state.filtered_helpers.len().min(5) as u16;
        let height = count + 2; // + borders or arrows
        let x = area.x + 1;
        let y = area.y + area.height.saturating_sub(height + 2); // above footer

        let rect = Rect {
            x,
            y,
            width,
            height,
        };
        f.render_widget(Clear, rect);
        crate::tui::services::helper_dropdown::render_file_search_dropdown(f, state, rect);
    } else if state.at_trigger_active && !state.at_results.is_empty() {
        render_at_dropdown(f, state);
    }
}

fn render_model_switcher(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("Filter ", Style::default().fg(Color::DarkGray)),
        Span::raw(&state.model_switcher_filter),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Model Switcher"),
    );
    f.render_widget(input, chunks[0]);

    let models = state.model_switcher_filtered();
    let items: Vec<ListItem> = models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == state.model_switcher_selected_idx {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{}  ", m.provider),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(m.name.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Models"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(list, chunks[1]);
}

fn render_file_search(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(80, 70, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("Query ", Style::default().fg(Color::DarkGray)),
        Span::raw(&state.file_search_query),
    ]))
    .block(Block::default().borders(Borders::ALL).title("File Search"));
    f.render_widget(input, chunks[0]);

    let items: Vec<ListItem> = state
        .file_search_results
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let style = if i == state.file_search_selected_idx {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(path.clone(), style)))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Files"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(list, chunks[1]);
}

fn render_changeset(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(90, 80, f.area());
    f.render_widget(Clear, area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    let entries = state.changeset_store.entries();
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == state.changeset_selected_idx {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let indicator = match entry.state {
                crate::tui::services::FileState::Created => "[+]",
                crate::tui::services::FileState::Modified => "[~]",
                crate::tui::services::FileState::Removed => "[-]",
                crate::tui::services::FileState::Reverted => "[✓]",
                crate::tui::services::FileState::FailedRestore => "[✗]",
            };
            let indicator_color = match entry.state {
                crate::tui::services::FileState::Created => Color::Green,
                crate::tui::services::FileState::Modified => Color::Yellow,
                crate::tui::services::FileState::Removed => Color::Red,
                crate::tui::services::FileState::Reverted => Color::Cyan,
                crate::tui::services::FileState::FailedRestore => Color::Red,
            };
            ListItem::new(Line::from(vec![
                Span::styled(indicator, Style::default().fg(indicator_color)),
                Span::raw(" "),
                Span::styled(entry.path.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Changeset"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(list, body[0]);

    let width = body[1].width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(diff) = &state.changeset_diff {
        if let Some(err) = &diff.last_error {
            lines.push(Line::styled(
                err.clone(),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        } else if let (Some(old), Some(new)) = (&diff.old_content, &diff.new_content) {
            lines.extend(crate::tui::services::preview_file_diff(
                &diff.path, old, new, width,
            ));
        } else {
            lines.push(Line::styled(
                "No diff available",
                Style::default().fg(Color::DarkGray),
            ));
        }
    } else {
        lines.push(Line::styled(
            "Select a file to preview diff",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let detail = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Preview"))
        .wrap(Wrap { trim: false })
        .scroll((state.changeset_diff_scroll as u16, 0));
    f.render_widget(detail, body[1]);
}

fn render_toast(f: &mut Frame, state: &mut AppState) {
    let Some(toast) = state.toasts.last() else {
        return;
    };

    let area = f.area();
    let max_width = area.width.saturating_sub(2).min(60);
    let text_width = toast.message.chars().count() as u16;
    let width = (text_width + 4).min(max_width).max(10);
    let height = 3u16.min(area.height.saturating_sub(1)).max(1);
    let x = area.x + area.width.saturating_sub(width + 1);
    let y = area.y + 1;

    let bg = match toast.style {
        ToastStyle::Success => Color::Green,
        ToastStyle::Error => Color::Red,
        ToastStyle::Info => Color::Blue,
    };
    let fg = Color::Black;

    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let widget = Paragraph::new(Line::from(Span::raw(toast.message.clone())))
        .style(Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(bg).fg(fg)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(widget, rect);
}

fn render_header(f: &mut Frame, state: &mut AppState, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();

    if std::env::var("VAC_INSIDE_ISOLATION").is_ok() {
        spans.push(Span::styled(
            "[ISOLATED] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::styled("VAC", Style::default().fg(Color::Magenta)));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("session {}", &state.session_id[..8]),
        Style::default().fg(Color::DarkGray),
    ));
    if let Some(title) = &state.session_title {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            title.clone(),
            Style::default().fg(Color::Cyan),
        ));
    }

    spans.push(Span::raw(" | "));
    spans.push(Span::styled(
        format!("env:{}", state.active_isolation_mode),
        Style::default().fg(Color::Cyan),
    ));

    spans.push(Span::raw(" | "));
    spans.push(Span::styled(
        format!("prof:{}", state.active_profile),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));

    // Runtime visibility badges
    if let Some(snapshot) = &state.runtime_state_snapshot {
        spans.push(Span::raw("  "));
        let (exec_label, exec_color) = match snapshot.execution_environment {
            vac_core::ExecutionEnvironment::Host => ("host", Color::Yellow),
            vac_core::ExecutionEnvironment::IsolatedBatch => ("isolated-batch", Color::Green),
            vac_core::ExecutionEnvironment::IsolatedInteractive => {
                ("isolated-interactive", Color::Cyan)
            }
        };
        spans.push(Span::styled(
            format!("exec:{}", exec_label),
            Style::default().fg(exec_color).add_modifier(Modifier::BOLD),
        ));

        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("intent:{}", snapshot.task_intent_mode),
            Style::default().fg(Color::Cyan),
        ));

        spans.push(Span::raw("  "));
        let env_color = if snapshot.environment_mode.contains("trusted-networked") {
            Color::Red
        } else {
            Color::Green
        };
        spans.push(Span::styled(
            format!("env:{}", snapshot.environment_mode),
            Style::default().fg(env_color),
        ));
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
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        },
    ));

    // VIL Status Badge
    let score = state.vil_status.validation_score;
    let score_label = if score >= 0.9 {
        "A"
    } else if score >= 0.7 {
        "B"
    } else {
        "C"
    };
    let badge_color = if score >= 0.9 {
        Color::Green
    } else if score >= 0.7 {
        Color::Yellow
    } else {
        Color::Red
    };

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("VIL:{}", score_label),
        Style::default()
            .fg(badge_color)
            .add_modifier(Modifier::BOLD),
    ));

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("approvals {}", state.pending_approvals.len()),
        Style::default().fg(Color::Yellow),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("review {}", state.changeset_store.active_entries().len()),
        Style::default().fg(Color::Cyan),
    ));

    let widget = Paragraph::new(Line::from(spans));
    f.render_widget(widget, area);
}

fn render_workspace(f: &mut Frame, state: &mut AppState, area: Rect) {
    let main_area = if state.side_panel_visible {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(state.side_panel_width),
                Constraint::Min(0),
            ])
            .split(area);

        crate::tui::services::side_panel::render_side_panel(f, state, h_chunks[0]);
        h_chunks[1]
    } else {
        area
    };

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(main_area);

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
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn render_messages(f: &mut Frame, state: &mut AppState, area: Rect) {
    state.message_area_y = area.y;
    state.message_area_height = area.height;

    use crate::tui::app::types::RenderedMessageCache;
    use crate::tui::services::message::render_tool_call_pending;
    use crate::tui::services::message::{render_assistant_message_with_width, render_user_message};
    use ratatui::text::Line;
    use ratatui::text::Span;
    use std::sync::Arc;

    let width = area.width.saturating_sub(2) as usize; // account for border
    let mut lines: Vec<Line<'static>> = Vec::new();

    let start_time = std::time::Instant::now();
    let mut hits = 0;
    let mut misses = 0;

    // Prune cache to a max size (e.g. 100) to act as LRU-ish
    if state.per_message_cache.len() > 200 {
        // Just clear it if it gets too big for now
        state.per_message_cache.clear();
    }

    state.line_to_message_map.clear();
    for msg in &state.messages {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        msg.content.hash(&mut hasher);
        msg.role.hash(&mut hasher);
        let content_hash = hasher.finish();

        if let Some(cached) = state.per_message_cache.get(&msg.id) {
            if cached.content_hash == content_hash && cached.width == width {
                hits += 1;
                let n = cached.rendered_lines.len();
                lines.extend(cached.rendered_lines.iter().cloned());
                lines.push(Line::raw(""));
                for _ in 0..=n {
                    state.line_to_message_map.push(msg.id);
                }
                continue;
            }
        }

        misses += 1;
        let mut msg_lines = Vec::new();
        match msg.role.as_str() {
            "user" => {
                msg_lines.extend(render_user_message(&msg.content, width));
            }
            "assistant" => {
                msg_lines.extend(render_assistant_message_with_width(&msg.content, width));
            }
            _ => {
                msg_lines.extend(msg.content.lines().map(|l| Line::raw(l.to_string())));
            }
        }

        let n = msg_lines.len();
        state.per_message_cache.insert(
            msg.id,
            RenderedMessageCache {
                content_hash,
                rendered_lines: Arc::new(msg_lines.clone()),
                width,
            },
        );

        lines.extend(msg_lines);
        lines.push(Line::raw("")); // spacing between messages
        for _ in 0..=n {
            state.line_to_message_map.push(msg.id);
        }
    }

    state.render_metrics.cache_hits += hits;
    state.render_metrics.cache_misses += misses;
    state.render_metrics.last_render_time_us = start_time.elapsed().as_micros() as u64;

    // Render pending tool calls from state
    for tc in &state.pending_tool_calls {
        lines.extend(render_tool_call_pending(tc));
    }

    // Cache the lines for text selection
    state.assembled_lines_cache = Some((state.messages.clone(), width, lines.clone()));

    // Apply text selection highlight
    let highlighted_lines = crate::tui::services::text_selection::apply_selection_highlight(
        lines,
        &state.selection_state,
        state.scroll,
    );

    let widget = Paragraph::new(highlighted_lines)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Conversation",
            focus_style(state.focus == WorkspaceFocus::Conversation),
        )))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll as u16, 0));
    f.render_widget(widget, area);
}

fn render_input(f: &mut Frame, state: &mut AppState, area: Rect) {
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
            Style::default().fg(Color::DarkGray),
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

    if state.focus == WorkspaceFocus::Input && !state.show_command_palette && !state.show_shortcuts
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

fn render_paste_tray(f: &mut Frame, state: &AppState, area: Rect) {
    use crate::tui::services::clipboard_paste::{
        PastedKind, kind_badge, preview_text, size_label, token_estimate,
    };

    // Header line: paste count + reorder-mode hint + clear hint.
    let mode_hint = if state.pending_paste_reorder_mode {
        Span::styled(
            " [REORDER — J/K swap, r exit]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " j/k select, d remove, r reorder, Enter preview",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )
    };
    let header = Line::from(vec![
        Span::styled("📎 ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} attachment(s)", state.pending_pastes.len()),
            Style::default().fg(Color::DarkGray),
        ),
        mode_hint,
        Span::styled(
            "  (Ctrl+U clear)",
            Style::default()
                .fg(Color::DarkGray)
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
        let badge_color = match &item.kind {
            PastedKind::Text { .. } => Color::Cyan,
            PastedKind::Image { .. } => Color::Magenta,
        };
        let row_style = if is_selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let spans = vec![
            Span::styled(
                format!("{} ", cursor),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                kind_badge(&item.kind).to_string(),
                Style::default()
                    .fg(badge_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(format!("#{}", item.id), row_style),
            Span::raw(" "),
            Span::styled(size_label(&item.kind), Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(
                format!("~{}tok", token_estimate(&item.kind)),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(preview_text(&item.kind), row_style),
        ];
        lines.push(Line::from(spans));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, area);
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
        lines.push(Line::styled("idle", Style::default().fg(Color::DarkGray)));
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
            format!("{}", state.changeset_store.active_entries().len()),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    if state.active_shell_command.is_some() || !state.shell_output.trim().is_empty() {
        let shell_state = if state.active_shell_command.is_some() {
            if state.shell_backgrounded {
                "background"
            } else {
                "active"
            }
        } else if let Some(code) = state.shell_exit_code {
            if code == 0 { "completed" } else { "failed" }
        } else {
            "idle"
        };
        lines.push(Line::from(vec![
            Span::styled("shell ", Style::default().fg(Color::DarkGray)),
            Span::styled(shell_state, Style::default().fg(Color::Cyan)),
        ]));
    }

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

    if !state.mcp_server_states.is_empty() {
        let connected = state
            .mcp_server_states
            .values()
            .filter(|s| s.is_connected())
            .count();
        let total = state.mcp_server_states.len();
        let color = if connected == total {
            Color::Green
        } else {
            Color::Yellow
        };
        lines.push(Line::from(vec![
            Span::styled("mcp ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}/{} connected", connected, total),
                Style::default().fg(color),
            ),
        ]));
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
        ActivityKind::Mcp => "🔌",
        ActivityKind::Isolation => "🛡",
        ActivityKind::Shell => "⚡",
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
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Activity",
            focus_style(state.focus == WorkspaceFocus::Activity),
        )))
        .wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}

fn render_workbench_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);

    let plan_label = match &state.plan_metadata {
        Some(m) => format!("Plan [{}]", m.status),
        None => "Plan".to_string(),
    };
    let tabs = vec![
        format!("Approvals ({})", state.pending_approvals.len()),
        format!("Review ({})", state.changeset_store.active_entries().len()),
        format!("Sessions ({})", state.sessions.len()),
        format!("Agents ({})", state.agent_tasks.len()),
        format!("Runtime ({})", state.runtime_jobs.len()),
        plan_label,
        format!("VIL ({})", state.vil_status.validation_issues.len()),
    ];
    let idx = match state.workbench_tab {
        WorkbenchTab::Approvals => 0,
        WorkbenchTab::Review => 1,
        WorkbenchTab::Sessions => 2,
        WorkbenchTab::Agents => 3,
        WorkbenchTab::Runtime => 4,
        WorkbenchTab::Plan => 5,
        WorkbenchTab::Vil => 6,
    };

    let tabs = Tabs::new(tabs)
        .select(idx)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Workbench",
            focus_style(state.focus == WorkspaceFocus::Workbench),
        )))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    match state.workbench_tab {
        WorkbenchTab::Approvals => render_approvals_workbench(f, state, chunks[1]),
        WorkbenchTab::Review => render_review_pane(f, state, chunks[1]),
        WorkbenchTab::Sessions => render_sessions_pane(f, state, chunks[1]),
        WorkbenchTab::Agents => render_agents_pane(f, state, chunks[1]),
        WorkbenchTab::Runtime => render_runtime_pane(f, state, chunks[1]),
        WorkbenchTab::Plan => render_plan_pane(f, state, chunks[1]),
        WorkbenchTab::Vil => crate::tui::services::vil_workbench::render(f, state, chunks[1]),
    }
}

fn render_plan_pane(f: &mut Frame, state: &mut AppState, area: Rect) {
    let body_text = if state.plan_draft.is_empty() {
        "No plan loaded. Run /plan to create one.".to_string()
    } else {
        crate::tui::services::plan::extract_plan_body(&state.plan_draft).to_string()
    };

    let mut lines: Vec<Line> = Vec::new();
    if let Some(meta) = &state.plan_metadata {
        lines.push(Line::from(vec![
            Span::styled("Title: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                meta.title.clone(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        let (status_label, status_color) = match meta.status {
            crate::tui::services::plan::PlanStatus::Drafting => ("drafting", Color::Yellow),
            crate::tui::services::plan::PlanStatus::PendingReview => {
                ("pending_review", Color::Cyan)
            }
            crate::tui::services::plan::PlanStatus::Approved => ("approved", Color::Green),
        };
        lines.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled(status_label.to_string(), Style::default().fg(status_color)),
            Span::styled(
                format!("  v{}", meta.version),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        lines.push(Line::raw(""));
    }
    for line in body_text.lines() {
        lines.push(Line::raw(line.to_string()));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  e: edit in $EDITOR  |  a: approve  |  r: request changes  |  /plan-review: overlay",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Plan",
            focus_style(state.focus == WorkspaceFocus::Workbench),
        )))
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
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

        if let Some(expl) = state
            .approval_explanations
            .get(&tc.id)
            .and_then(|v| v.clone())
        {
            lines.push(Line::styled(
                "Explanation",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for l in expl.lines() {
                lines.push(Line::styled(
                    l.to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
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

fn render_shell_popup(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(80, 55, f.area());
    f.render_widget(Clear, area);

    let title = if let Some(shell) = &state.active_shell_command {
        if state.shell_waiting_for_input {
            format!("Shell [{}] waiting for input", shell.command)
        } else {
            format!("Shell [{}] active", shell.command)
        }
    } else if let Some(code) = state.shell_exit_code {
        format!("Shell completed (exit {code})")
    } else {
        "Shell".to_string()
    };

    let mut lines: Vec<Line> = Vec::new();
    let content: Vec<&str> = state.shell_output.lines().collect();
    let max_lines = area.height.saturating_sub(4) as usize;
    let start = content.len().saturating_sub(max_lines);
    for line in content.into_iter().skip(start) {
        lines.push(Line::raw(line.to_string()));
    }

    if lines.is_empty() {
        lines.push(Line::styled(
            "Shell session started. Use the input box and press Enter to send input.",
            Style::default().fg(Color::DarkGray),
        ));
    }

    if let Some(err) = &state.shell_last_error {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Last error: {err}"),
            Style::default().fg(Color::Red),
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Ctrl+Z: background  /shell-focus: refocus  /shell-kill: terminate",
        Style::default().fg(Color::DarkGray),
    ));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
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

    let header =
        Paragraph::new(filter_line).block(Block::default().borders(Borders::ALL).title(title));
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
            let checkpoint_icon = if s.has_checkpoint { "●" } else { "○" };
            ListItem::new(Line::from(vec![
                Span::styled(
                    checkpoint_icon,
                    Style::default().fg(if s.has_checkpoint {
                        Color::Green
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::raw(" "),
                Span::styled(&s.last_activity, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(&s.title, style),
                Span::styled(
                    format!(" ({}t)", s.task_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Sessions ({})", state.sessions.len())),
    );
    f.render_widget(list, body[0]);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(sel) = state.sessions.get(state.sessions_selected_idx) {
        lines.push(Line::from(vec![
            Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(sel.title.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(sel.id.chars().take(16).collect::<String>()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Last active: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(sel.last_activity.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Tasks: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(sel.task_count.to_string()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Checkpoint: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            if sel.has_checkpoint {
                Span::styled("available ●", Style::default().fg(Color::Green))
            } else {
                Span::styled("none ○", Style::default().fg(Color::DarkGray))
            },
        ]));
        if !sel.checkpoints.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Checkpoints:",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for cp in sel.checkpoints.iter().take(4) {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::raw(cp.clone()),
                ]));
            }
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Enter: restore  r: resume checkpoint",
            Style::default().fg(Color::DarkGray),
        ));
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

fn render_agents_pane(f: &mut Frame, state: &mut AppState, area: Rect) {
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(area);

    let mut queued = 0usize;
    let mut running = 0usize;
    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut cancelled = 0usize;
    for task in &state.agent_tasks {
        match &task.status {
            vac_runtime::AgentTaskStatus::Queued => queued += 1,
            vac_runtime::AgentTaskStatus::Running => running += 1,
            vac_runtime::AgentTaskStatus::Completed => completed += 1,
            vac_runtime::AgentTaskStatus::Failed(_) => failed += 1,
            vac_runtime::AgentTaskStatus::Cancelled => cancelled += 1,
        }
    }

    let items: Vec<ListItem> = state
        .agent_tasks
        .iter()
        .enumerate()
        .map(|(idx, task)| {
            let selected = idx == state.agent_selected_idx;
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let status = match &task.status {
                vac_runtime::AgentTaskStatus::Queued => {
                    Span::styled("Q", Style::default().fg(Color::DarkGray))
                }
                vac_runtime::AgentTaskStatus::Running => {
                    Span::styled("R", Style::default().fg(Color::Cyan))
                }
                vac_runtime::AgentTaskStatus::Completed => {
                    Span::styled("C", Style::default().fg(Color::Green))
                }
                vac_runtime::AgentTaskStatus::Failed(_) => {
                    Span::styled("F", Style::default().fg(Color::Red))
                }
                vac_runtime::AgentTaskStatus::Cancelled => {
                    Span::styled("X", Style::default().fg(Color::Yellow))
                }
            };
            let role = Span::styled(
                task.role.label(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            );
            let short_id = task.id.to_string().chars().take(8).collect::<String>();
            let mut desc = task.description.clone();
            if desc.chars().count() > 48 {
                desc = desc.chars().take(45).collect::<String>() + "...";
            }
            ListItem::new(Line::from(vec![
                status,
                Span::raw(" "),
                role,
                Span::raw(" "),
                Span::styled(short_id, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(desc, style),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Queue"));
    f.render_widget(list, body[0]);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Tasks: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("Q {queued}"), Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(format!("R {running}"), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(format!("C {completed}"), Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::styled(format!("F {failed}"), Style::default().fg(Color::Red)),
        Span::raw("  "),
        Span::styled(format!("X {cancelled}"), Style::default().fg(Color::Yellow)),
    ]));
    lines.push(Line::raw(""));

    if let Some(snapshot) = &state.agent_state_snapshot {
        lines.push(Line::styled(
            "Workers:",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        for w in &snapshot.workers {
            let role = Span::styled(
                w.role.label(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            );
            let status = match &w.status {
                vac_runtime::AgentWorkerStatus::Idle => {
                    Span::styled("idle", Style::default().fg(Color::DarkGray))
                }
                vac_runtime::AgentWorkerStatus::Running { task_id, .. } => Span::styled(
                    format!(
                        "running {}",
                        task_id.to_string().chars().take(8).collect::<String>()
                    ),
                    Style::default().fg(Color::Cyan),
                ),
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                role,
                Span::raw(" "),
                Span::styled(w.worker_id.clone(), Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                status,
            ]));
            if let Some(out) = &w.last_output {
                let mut o = out.clone();
                if o.chars().count() > 72 {
                    o = o.chars().take(69).collect::<String>() + "...";
                }
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(o, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(
                snapshot
                    .updated_at
                    .format("%Y-%m-%d %H:%M:%S UTC")
                    .to_string(),
            ),
        ]));
        lines.push(Line::raw(""));
    } else {
        lines.push(Line::styled(
            "No agent scheduler state found.",
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(Line::raw(""));
    }

    if let Some(task) = state.agent_tasks.get(state.agent_selected_idx) {
        lines.push(Line::styled(
            "Selected:",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::from(vec![
            Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
            Span::raw(task.id.to_string()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Role: ", Style::default().fg(Color::DarkGray)),
            Span::raw(task.role.label()),
        ]));
        let status = match &task.status {
            vac_runtime::AgentTaskStatus::Queued => "Queued".to_string(),
            vac_runtime::AgentTaskStatus::Running => "Running".to_string(),
            vac_runtime::AgentTaskStatus::Completed => "Completed".to_string(),
            vac_runtime::AgentTaskStatus::Failed(e) => format!("Failed: {e}"),
            vac_runtime::AgentTaskStatus::Cancelled => "Cancelled".to_string(),
        };
        lines.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            Span::raw(status),
        ]));
        if let Some(out) = &task.output_summary {
            lines.push(Line::from(vec![
                Span::styled("Output: ", Style::default().fg(Color::DarkGray)),
                Span::raw(out.clone()),
            ]));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "j/k: navigate  r: refresh",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        lines.push(Line::styled(
            "No tasks enqueued.",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let detail = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Agents"))
        .wrap(Wrap { trim: true })
        .scroll((state.agent_detail_scroll as u16, 0));
    f.render_widget(detail, body[1]);
}

fn render_runtime_pane(f: &mut Frame, state: &mut AppState, area: Rect) {
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(area);

    let mut queued = 0usize;
    let mut running = 0usize;
    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut cancelled = 0usize;
    for job in &state.runtime_jobs {
        match &job.status {
            vac_runtime::JobStatus::Queued => queued += 1,
            vac_runtime::JobStatus::Running => running += 1,
            vac_runtime::JobStatus::Completed => completed += 1,
            vac_runtime::JobStatus::Failed(_) => failed += 1,
            vac_runtime::JobStatus::Cancelled => cancelled += 1,
        }
    }

    let items: Vec<ListItem> = state
        .runtime_jobs
        .iter()
        .enumerate()
        .map(|(idx, job)| {
            let selected = idx == state.runtime_selected_idx;
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let status = match &job.status {
                vac_runtime::JobStatus::Queued => {
                    Span::styled("Q", Style::default().fg(Color::DarkGray))
                }
                vac_runtime::JobStatus::Running => {
                    Span::styled("R", Style::default().fg(Color::Cyan))
                }
                vac_runtime::JobStatus::Completed => {
                    Span::styled("C", Style::default().fg(Color::Green))
                }
                vac_runtime::JobStatus::Failed(_) => {
                    Span::styled("F", Style::default().fg(Color::Red))
                }
                vac_runtime::JobStatus::Cancelled => {
                    Span::styled("X", Style::default().fg(Color::Yellow))
                }
            };
            ListItem::new(Line::from(vec![
                status,
                Span::raw(" "),
                Span::styled(
                    job.id.to_string().chars().take(8).collect::<String>(),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(" "),
                Span::styled(job.kind_name(), style),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Jobs"));
    f.render_widget(list, body[0]);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Jobs: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("Q {queued}"), Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(format!("R {running}"), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(format!("C {completed}"), Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::styled(format!("F {failed}"), Style::default().fg(Color::Red)),
        Span::raw("  "),
        Span::styled(format!("X {cancelled}"), Style::default().fg(Color::Yellow)),
    ]));
    lines.push(Line::raw(""));

    if let Some(snapshot) = &state.runtime_state_snapshot {
        lines.push(Line::from(vec![
            Span::styled("Autopilot: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(snapshot.mode.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Intent: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(snapshot.task_intent_mode.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Environment: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(snapshot.environment_mode.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Execution: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{:?}", snapshot.execution_environment)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Queue: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(snapshot.queue_len.to_string()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("State: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{:?}", snapshot.state)),
        ]));
        if let Some(err) = &snapshot.last_error {
            lines.push(Line::from(vec![
                Span::styled(
                    "Last error: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(err.clone(), Style::default().fg(Color::Red)),
            ]));
        }
        if let Some(job_id) = snapshot.current_job {
            lines.push(Line::from(vec![
                Span::styled(
                    "Current job: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(job_id.to_string()),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(
                snapshot
                    .updated_at
                    .format("%Y-%m-%d %H:%M:%S UTC")
                    .to_string(),
            ),
        ]));
        match &snapshot.state {
            vac_runtime::AutopilotState::WaitingApproval { tool_call_id } => {
                lines.push(Line::from(vec![
                    Span::styled("Approval: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(tool_call_id.clone(), Style::default().fg(Color::Yellow)),
                ]));
            }
            vac_runtime::AutopilotState::Backoff { until } => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Backoff until: ",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        until.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
            }
            _ => {}
        }
        lines.push(Line::raw(""));
    }

    if !state.mcp_server_states.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "MCP Servers:",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        for (name, conn_state) in &state.mcp_server_states {
            let (status, color) = if conn_state.is_connected() {
                ("✅ connected", Color::Green)
            } else {
                ("❌ unreachable", Color::Red)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(name.clone(), Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(status, Style::default().fg(color)),
            ]));
            if let vac_tools::mcp::McpConnectionStatus::Unreachable(reason) = &conn_state.status {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(reason.clone(), Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
        lines.push(Line::raw(""));
    }

    if let Some(job) = state.runtime_jobs.get(state.runtime_selected_idx) {
        lines.push(Line::from(vec![
            Span::styled("Job: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(job.id.to_string()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Kind: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(job.kind_name()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(match &job.status {
                vac_runtime::JobStatus::Queued => "Queued".to_string(),
                vac_runtime::JobStatus::Running => "Running".to_string(),
                vac_runtime::JobStatus::Completed => "Completed".to_string(),
                vac_runtime::JobStatus::Failed(err) => format!("Failed: {err}"),
                vac_runtime::JobStatus::Cancelled => "Cancelled".to_string(),
            }),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Retries: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}/{}", job.retry_count, job.max_retries)),
        ]));
        if let Some(summary) = &job.result_summary {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Summary",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for line in summary.lines() {
                lines.push(Line::raw(line.to_string()));
            }
        }
    } else {
        lines.push(Line::styled(
            "No runtime jobs loaded. Press r to refresh.",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let detail = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Runtime Detail"),
        )
        .wrap(Wrap { trim: false })
        .scroll((state.runtime_detail_scroll as u16, 0));
    f.render_widget(detail, body[1]);
}

fn render_at_dropdown(f: &mut Frame, state: &mut AppState) {
    let area = f.area();
    let count = state.at_results.len().min(8) as u16;
    if count == 0 {
        return;
    }
    // Position: bottom-left of screen, above footer, width = 50% of screen
    let width = (area.width / 2).max(30).min(area.width.saturating_sub(2));
    let height = count + 2; // border
    let x = area.x + 1;
    let y = area.y + area.height.saturating_sub(height + 2); // above footer

    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);

    let items: Vec<ListItem> = state
        .at_results
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let selected = i
                == state
                    .at_selected_idx
                    .min(state.at_results.len().saturating_sub(1));
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(path.clone(), style)))
        })
        .collect();

    let query_hint = if state.at_query.is_empty() {
        "@".to_string()
    } else {
        format!("@{}", state.at_query)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(query_hint, Style::default().fg(Color::Cyan))),
    );
    f.render_widget(list, rect);
}

fn render_footer(f: &mut Frame, state: &mut AppState, area: Rect) {
    // Reject reason prompt takes priority
    if let Some(reason) = &state.reject_reason_input {
        let hints = vec![
            Span::styled(
                "REJECT REASON ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(reason.as_str()),
            Span::styled("█", Style::default().fg(Color::Yellow)),
            Span::styled(
                "  Enter: confirm  Esc: skip",
                Style::default().fg(Color::DarkGray),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    if state.at_trigger_active {
        let hints = vec![
            Span::styled(
                "@ FILE ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(&state.at_query, Style::default().fg(Color::White)),
            Span::styled(
                "  ↑↓: select  Enter: insert  Esc: cancel",
                Style::default().fg(Color::DarkGray),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    if state.shell_popup_visible && state.active_shell_command.is_some() {
        let hints = vec![
            Span::styled(
                "SHELL ",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  Ctrl+Z: background  Esc: close  Ctrl+C: kill",
                Style::default().fg(Color::DarkGray),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    let registry = crate::tui::action_registry::ActionRegistry::new();
    let ctx = crate::tui::action_registry::ActionContext::from_app_state(state);
    let actions = registry.get_actions_for_context(ctx);

    let mut hints = Vec::new();
    for action in actions {
        if !hints.is_empty() {
            hints.push(Span::raw("  "));
        }
        let key_str = action.keys.join("/");
        hints.push(Span::styled(key_str, Style::default().fg(Color::Cyan)));
        hints.push(Span::styled(
            format!(": {}  ", action.description.to_lowercase()),
            Style::default().fg(Color::DarkGray),
        ));
    }

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
        "Ctrl+F - Toggle auto-approve",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::AppStateOptions;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn view_smoke_renders() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });

        state.pending_approvals.push(crate::tui::ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: crate::tui::FunctionCall {
                name: "file_write".to_string(),
                arguments: r#"{"file_path":"src/lib.rs"}"#.to_string(),
            },
            metadata: None,
        });
        state.available_models.push(crate::tui::Model {
            id: "kilo-auto/free".to_string(),
            name: "kilo-auto/free".to_string(),
            provider: "anthropic".to_string(),
            supports_reasoning: false,
        });
        state.show_model_switcher = true;
        state.show_file_search = true;
        state.file_search_results = vec!["src/main.rs".to_string()];
        state.show_changeset = true;
        state
            .changeset_store
            .file_modified("src/main.rs".to_string(), "agent".to_string(), false);
        state.modified_files = state.changeset_store.modified_files();

        terminal.draw(|f| view(f, &mut state)).unwrap();
    }
}
