//! View Module

use crate::app::{ActivityKind, AppState, WorkspaceFocus};
use crate::services::ToastStyle;
use crate::ui::style::focus_style;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};

/// Main view function
pub fn view(f: &mut Frame, state: &mut AppState) {
    let banner_h = crate::services::banner::banner_height(state);
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
        crate::services::banner::render_banner(f, chunks[1], state);
    } else {
        state.banner_click_regions.clear();
        state.banner_dismiss_region = None;
    }
    render_workspace(f, state, chunks[2]);
    crate::services::statusline::render_statusline(f, state, chunks[3]);
    render_footer(f, state, chunks[4]);

    if state.overlay_manager.is_active(crate::overlay::OverlayId::CommandPalette) {
        render_command_palette(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::Shortcuts) {
        render_shortcuts(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::IsolationSwitcher) {
        crate::services::isolation_switcher::render_isolation_switcher(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::ProfileSwitcher) {
        crate::services::profile_switcher::render_profile_switcher(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::RulebookSwitcher) {
        crate::services::rulebook_switcher::render_rulebook_switcher(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::MessageAction) {
        crate::services::message_action_popup::render_message_action_popup(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::ModelSwitcher) {
        render_model_switcher(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::FileSearch) {
        render_file_search(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::Changeset) {
        render_changeset(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::FileChanges) {
        crate::services::file_changes_popup::render_file_changes_popup(f, state);
    }

    if state.plan.review_open {
        crate::services::plan_review::render_plan_review(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::AskUser) {
        crate::services::ask_user::render_ask_user_popup(f, state);
    }

    if state.shell.session_store.popup_visible {
        render_shell_popup(f, state);
    }

    if !state.toasts.is_empty() {
        render_toast(f, state);
    }

    if state.overlay_manager.is_active(crate::overlay::OverlayId::HelperDropdown) {
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
        crate::services::helper_dropdown::render_file_search_dropdown(f, state, rect);
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
                vac_changeset::FileState::Created => "[+]",
                vac_changeset::FileState::Modified => "[~]",
                vac_changeset::FileState::Removed => "[-]",
                vac_changeset::FileState::Reverted => "[✓]",
                vac_changeset::FileState::FailedRestore => "[✗]",
            };
            let indicator_color = match entry.state {
                vac_changeset::FileState::Created => Color::Green,
                vac_changeset::FileState::Modified => Color::Yellow,
                vac_changeset::FileState::Removed => Color::Red,
                vac_changeset::FileState::Reverted => Color::Cyan,
                vac_changeset::FileState::FailedRestore => Color::Red,
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
            lines.extend(crate::services::preview_file_diff(
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
    let score = state.vil.status.validation_score;
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

        crate::services::side_panel::render_side_panel(f, state, h_chunks[0]);
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


fn render_messages(f: &mut Frame, state: &mut AppState, area: Rect) {
    state.message_area_y = area.y;
    state.message_area_height = area.height;

    use crate::app::types::RenderedMessageCache;
    use crate::services::message::render_tool_call_pending;
    use crate::services::message::{render_assistant_message_with_width, render_user_message};
    use ratatui::text::Line;
    use ratatui::text::Span;
    use std::sync::Arc;

    let width = area.width.saturating_sub(2) as usize; // account for border
    let mut lines: Vec<Line<'static>> = Vec::new();

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

    // Render pending tool calls from state
    for tc in &state.pending_tool_calls {
        lines.extend(render_tool_call_pending(tc));
    }

    // Cache the lines for text selection
    state.assembled_lines_cache = Some((state.messages.clone(), width, lines.clone()));

    // Apply text selection highlight
    let highlighted_lines = crate::services::text_selection::apply_selection_highlight(
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

    if state.focus == WorkspaceFocus::Input && !state.overlay_manager.is_active(crate::overlay::OverlayId::CommandPalette) && !state.overlay_manager.is_active(crate::overlay::OverlayId::Shortcuts)
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
    use crate::services::clipboard_paste::{
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

    if let Some(snapshot) = &state.runtime.snapshot {
        let (exec_label, exec_color) = match snapshot.execution_environment {
            vac_core::ExecutionEnvironment::Host => ("host", Color::Yellow),
            vac_core::ExecutionEnvironment::IsolatedBatch => ("isolated-batch", Color::Green),
            vac_core::ExecutionEnvironment::IsolatedInteractive => ("isolated-interactive", Color::Cyan),
        };
        let env_color = if snapshot.environment_mode.contains("trusted-networked") { Color::Red } else { Color::Green };
        lines.push(Line::from(vec![
            Span::styled("exec ", Style::default().fg(Color::DarkGray)),
            Span::styled(exec_label, Style::default().fg(exec_color)),
            Span::raw("  "),
            Span::styled("intent ", Style::default().fg(Color::DarkGray)),
            Span::styled(snapshot.task_intent_mode.to_string(), Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled("env ", Style::default().fg(Color::DarkGray)),
            Span::styled(snapshot.environment_mode.clone(), Style::default().fg(env_color)),
        ]));
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

    if let Some(session) = state.shell.session_store.active() {
        if session.command.is_some() || !session.output.trim().is_empty() {
            let shell_state = if session.command.is_some() {
                if session.backgrounded {
                    "background"
                } else {
                    "active"
                }
            } else if let Some(code) = session.exit_code {
                if code == 0 { "completed" } else { "failed" }
            } else {
                "idle"
            };
            lines.push(Line::from(vec![
                Span::styled("shell ", Style::default().fg(Color::DarkGray)),
                Span::styled(shell_state, Style::default().fg(Color::Cyan)),
                Span::raw("  "),
                Span::styled(session.label.clone(), Style::default().fg(Color::DarkGray)),
            ]));
        }

        if !session.output.trim().is_empty() {
            let last = session
                .output
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
    }

    if !state.mcp_server_states.is_empty() {
        let connected = state
            .mcp_server_states
            .values()
            .filter(|s| s.is_connected())
            .count();
        let total = state.mcp_server_states.len();
        let color = if connected == total { Color::Green } else { Color::Yellow };
        lines.push(Line::from(vec![
            Span::styled("mcp ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}/{} connected", connected, total), Style::default().fg(color)),
        ]));
    }

    // Render budget indicator — only shown when over 16ms
    if state.render_metrics.ema_render_time_us > 16_000 {
        lines.push(Line::from(vec![
            Span::styled("render ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}ms avg ⚠", state.render_metrics.ema_render_time_us / 1000),
                Style::default().fg(Color::Red),
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
            "No activity yet. Events, approvals, and runtime updates will appear here.",
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

    let idx = crate::workbench::active_tab_index(&state.workbench_tab);
    let tabs = Tabs::new(crate::workbench::tab_labels(state))
        .select(idx)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Workbench",
            focus_style(state.focus == WorkspaceFocus::Workbench),
        )))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, chunks[0]);

    crate::workbench::render_active_tab(f, state, chunks[1]);
}
fn render_shell_popup(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(80, 55, f.area());
    f.render_widget(Clear, area);

    let active_session = state.shell.session_store.active();
    let title = if let Some(session) = active_session {
        if let Some(shell) = &session.command {
            if session.waiting_for_input {
                format!("Shell [{}] waiting for input", shell.command)
            } else {
                format!("Shell [{}] active", shell.command)
            }
        } else if let Some(code) = session.exit_code {
            format!("Shell {} completed (exit {code})", session.label)
        } else {
            format!("Shell {}", session.label)
        }
    } else {
        "Shell".to_string()
    };

    let mut lines: Vec<Line> = Vec::new();
    let content: Vec<&str> = active_session
        .map(|session| session.output.lines().collect())
        .unwrap_or_default();
    let max_lines = area.height.saturating_sub(4) as usize;
    let start = content.len().saturating_sub(max_lines);
    for line in content.into_iter().skip(start) {
        lines.push(Line::raw(line.to_string()));
    }

    if lines.is_empty() {
        lines.push(Line::styled(
            "Shell session active. Type a command and press Enter to send input.",
            Style::default().fg(Color::DarkGray),
        ));
    }

    if let Some(err) = active_session.and_then(|session| session.last_error.as_ref()) {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Last error: {err}"),
            Style::default().fg(Color::Red),
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Ctrl+Z backgrounds | /shell-focus restores | /shell-kill terminates",
        Style::default().fg(Color::DarkGray),
    ));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
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

    if state.shell.session_store.popup_visible
        && state
            .shell
            .session_store
            .active()
            .and_then(|session| session.command.as_ref())
            .is_some()
    {
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

    let ctx = crate::action_registry::ActionContext::from_app_state(state);
    let mut hints = Vec::new();
    for spec in crate::action_registry::footer_specs(ctx) {
        if !(spec.availability)(state) {
            continue;
        }
        if !hints.is_empty() {
            hints.push(Span::raw("  "));
        }
        let key_str = spec.keybindings.join("/");
        hints.push(Span::styled(key_str, Style::default().fg(Color::Cyan)));
        hints.push(Span::styled(
            format!(": {}  ", spec.title.to_lowercase()),
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::app::{AppStateOptions, WorkbenchTab};
    use crate::overlay::{OverlayId, open_overlay};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_to_string(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    fn render_state_to_string(state: &mut AppState) -> String {
        let backend = TestBackend::new(200, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view(f, state)).unwrap();
        render_to_string(&terminal)
    }

    fn normalized_rendered(rendered: &str) -> String {
        rendered.split_whitespace().collect::<Vec<_>>().join(" ")
    }

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

        state.pending_approvals.push(crate::ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: crate::FunctionCall {
                name: "file_write".to_string(),
                arguments: r#"{"file_path":"src/lib.rs"}"#.to_string(),
            },
            metadata: None,
        });
        state.available_models.push(crate::Model {
            id: "kilo-auto/free".to_string(),
            name: "kilo-auto/free".to_string(),
            provider: "anthropic".to_string(),
            supports_reasoning: false,
            ..Default::default()
        });
        open_overlay(&mut state, OverlayId::ModelSwitcher);
        open_overlay(&mut state, OverlayId::FileSearch);
        state.file_search_results = vec!["src/main.rs".to_string()];
        open_overlay(&mut state, OverlayId::Changeset);
        state
            .changeset_store
            .file_modified("src/main.rs".to_string(), "agent".to_string(), false);
        state.modified_files = state.changeset_store.modified_files();

        terminal.draw(|f| view(f, &mut state)).unwrap();
    }

    #[test]
    fn pinned_context_visible_in_view() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });
        state.side_panel_visible = true;
        state.pinned_files.push("src/lib.rs".to_string());
        state.pinned_diagnostics.push("src/main.rs".to_string());

        terminal.draw(|f| view(f, &mut state)).unwrap();
        let rendered = render_to_string(&terminal);
        assert!(rendered.contains("src/lib.rs"));
        assert!(rendered.contains("src/main.rs"));
    }

    #[test]
    fn empty_states_use_explicit_copy() {
        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });
        state.workbench_tab = WorkbenchTab::Approvals;

        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("no active model selected"));
        assert!(rendered.contains("No pending approvals"));

        state.side_panel_visible = true;
        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("No pinned context yet"));
        assert!(rendered.contains("No sessions loaded yet"));
        assert!(rendered.contains("No MCP servers configured"));

        state.workbench_tab = WorkbenchTab::Sessions;
        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("No sessions loaded yet"));

        state.workbench_tab = WorkbenchTab::Runtime;
        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("No runtime jobs loaded yet"));

        state.workbench_tab = WorkbenchTab::Plan;
        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("No plan loaded yet"));
    }
}
