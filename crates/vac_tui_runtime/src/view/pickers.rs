//! File picker, context chips, task tray, theme picker, session resume overlays

use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use super::overlays::centered_rect;

pub(super) fn render_file_picker(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let cwd_label = state.file_picker_cwd.to_string_lossy().to_string();
    let title = format!(
        " Files  {}  (Space=select  Tab=enter  Bsp=up  Enter=confirm  Esc) ",
        cwd_label
    );
    let search_block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(state.theme.style(StyleKey::OverlayBorder));
    let search_para = Paragraph::new(state.file_picker_query.as_str())
        .block(search_block)
        .style(state.theme.style(StyleKey::InputFg));
    f.render_widget(search_para, chunks[0]);

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(state.theme.style(StyleKey::BorderNormal));
    let list_inner = list_block.inner(split[0]);
    f.render_widget(list_block, split[0]);

    let items: Vec<ListItem> = state
        .file_picker_results
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let is_dir = path.is_dir();
            let selected = state.file_picker_multi_selected.contains(&i);
            let prefix = if selected {
                "[✓] "
            } else if is_dir {
                " ▶  "
            } else {
                "    "
            };
            let label = format!("{prefix}{name}");
            let style = if i == state.file_picker_selected {
                state.theme.style(StyleKey::OverlaySelected)
            } else if is_dir {
                state.theme.style(StyleKey::Accent)
            } else {
                state.theme.style(StyleKey::Normal)
            };
            ListItem::new(Line::styled(label, style))
        })
        .collect();
    f.render_widget(List::new(items), list_inner);

    // Preview pane
    let preview_block = Block::default()
        .title(" Preview ")
        .borders(Borders::ALL)
        .border_style(state.theme.style(StyleKey::BorderNormal));
    let preview_inner = preview_block.inner(split[1]);
    f.render_widget(preview_block, split[1]);
    let preview_text = state
        .file_picker_preview
        .as_deref()
        .unwrap_or("(select a file to preview)");
    let preview_para = Paragraph::new(preview_text)
        .wrap(Wrap { trim: false })
        .style(state.theme.style(StyleKey::CodeFg));
    f.render_widget(preview_para, preview_inner);
}

pub fn render_context_chips(f: &mut Frame, state: &AppState, area: Rect) {
    if state.context_chips.is_empty() {
        return;
    }
    let mut spans: Vec<Span> = Vec::new();
    for (i, chip) in state.context_chips.iter().enumerate() {
        let is_focused = state.context_chip_cursor == Some(i);
        let style = if is_focused {
            state.theme.style(StyleKey::OverlaySelected)
        } else {
            state.theme.style(StyleKey::Accent)
        };
        spans.push(Span::styled(format!(" @{} ", chip.label), style));
        spans.push(Span::raw(" "));
    }
    let para = Paragraph::new(Line::from(spans));
    f.render_widget(para, area);
}

pub(super) fn render_task_tray(f: &mut Frame, state: &mut AppState) {
    use vac_runtime::jobs::JobStatus;
    let area = f.area();
    let width = 52u16.min(area.width.saturating_sub(2));
    let jobs: Vec<_> = if state.task_tray_filter_active_only {
        state
            .runtime
            .jobs
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Running | JobStatus::Queued))
            .collect()
    } else {
        state.runtime.jobs.iter().collect()
    };
    let height = (jobs.len() as u16 + 4)
        .min(area.height.saturating_sub(2))
        .max(5);
    let x = area.x + area.width.saturating_sub(width + 1);
    let y = area.y + area.height.saturating_sub(height + 1);
    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);

    let filter_label = if state.task_tray_filter_active_only {
        " [active] "
    } else {
        " [all] "
    };
    let title = format!(" Tasks{filter_label}(f=filter  x=cancel  Esc) ");
    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(state.theme.style(StyleKey::OverlayBorder));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let items: Vec<ListItem> = jobs
        .iter()
        .enumerate()
        .map(|(i, job)| {
            let (status_sym, status_style) = match &job.status {
                JobStatus::Running => ("▶ ", state.theme.style(StyleKey::TaskRunning)),
                JobStatus::Queued => ("⏳", state.theme.style(StyleKey::TaskQueued)),
                JobStatus::Completed => ("✓ ", state.theme.style(StyleKey::TaskCompleted)),
                JobStatus::Failed(_) => ("✗ ", state.theme.style(StyleKey::TaskFailed)),
                JobStatus::Cancelled => ("— ", state.theme.style(StyleKey::Muted)),
            };
            let label = format!("{status_sym}{:?}", job.kind);
            let line = if i == state.task_tray_selected {
                Line::styled(label, state.theme.style(StyleKey::OverlaySelected))
            } else {
                Line::from(vec![Span::styled(label, status_style)])
            };
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);

    // PR-T16 — track per-row click regions for the mouse dispatcher.
    state.task_tray_row_regions.clear();
    for i in 0..jobs.len() {
        let row_y = inner.y.saturating_add(i as u16);
        if row_y >= inner.y.saturating_add(inner.height) {
            break;
        }
        state.task_tray_row_regions.push(Rect {
            x: inner.x,
            y: row_y,
            width: inner.width,
            height: 1,
        });
    }
}

pub(super) fn render_theme_picker(f: &mut Frame, state: &mut AppState) {
    use crate::services::theme::ThemePreset;
    let area = centered_rect(40, 30, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Theme Picker  (↑↓ select  Enter apply  Esc cancel) ")
        .borders(Borders::ALL)
        .border_style(state.theme.style(StyleKey::OverlayBorder));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = ThemePreset::ALL
        .iter()
        .enumerate()
        .map(|(i, preset)| {
            let label = format!(
                " {}{}",
                if state.theme.preset == *preset {
                    "● "
                } else {
                    "  "
                },
                preset.label()
            );
            let style = if i == state.theme_picker_selected {
                state.theme.style(StyleKey::OverlaySelected)
            } else {
                state.theme.style(StyleKey::Normal)
            };
            ListItem::new(Line::styled(label, style))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

pub(super) fn render_session_resume(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(70, 70, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let date_hint = match state.session_resume_date_filter_days {
        None => "all time",
        Some(7) => "last 7d",
        Some(30) => "last 30d",
        Some(90) => "last 90d",
        Some(_) => "custom",
    };
    let filtered_count = state.session_resume_filtered_indices.len();
    let total_count = state.session_resume_list.len();
    let title = format!(
        " Resume Session  [{date_hint}]  {filtered_count}/{total_count}  (Tab=date  ↑↓=nav  Enter=open  Esc) "
    );

    let search_block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(state.theme.style(StyleKey::OverlayBorder));
    let search_input = Paragraph::new(state.session_resume_query.as_str())
        .block(search_block)
        .style(state.theme.style(StyleKey::InputFg));
    f.render_widget(search_input, chunks[0]);

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(state.theme.style(StyleKey::BorderNormal));
    let inner = list_block.inner(chunks[1]);
    f.render_widget(list_block, chunks[1]);

    let selected = state.session_resume_selected;
    let indices = state.session_resume_filtered_indices.clone();
    let items: Vec<ListItem> = indices
        .iter()
        .enumerate()
        .filter_map(|(display_i, &list_i)| {
            state
                .session_resume_list
                .get(list_i)
                .map(|e| (display_i, e))
        })
        .map(|(i, entry)| {
            let model_tag = entry.model.as_deref().unwrap_or("-");
            let tok_tag = entry
                .token_count
                .map(|t| format!(" {t}tok"))
                .unwrap_or_default();
            let label = format!(
                " {:16}  {}  {:8}{}  {}",
                entry.project.chars().take(16).collect::<String>(),
                entry.last_active.format("%Y-%m-%d"),
                model_tag.chars().take(8).collect::<String>(),
                tok_tag,
                entry
                    .last_message_preview
                    .chars()
                    .take(40)
                    .collect::<String>(),
            );
            let style = if i == selected {
                state.theme.style(StyleKey::OverlaySelected)
            } else {
                state.theme.style(StyleKey::Normal)
            };
            ListItem::new(Line::styled(label, style))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}
