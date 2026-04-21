//! Review tab — inspect and revert file changes.

use super::WorkbenchTabView;
use crate::services::theme::StyleKey;
use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

pub struct ReviewTab;

impl WorkbenchTabView for ReviewTab {
    fn tab_label(state: &AppState) -> String {
        format!("Review ({})", state.changeset_store.active_entries().len())
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);

        let title = format!("Review ({})", state.review_filtered_paths().len());
        let filter_line = if state.review.filter.is_empty() {
            Line::from(vec![
                Span::styled("Filter: ", state.theme.style(StyleKey::Accent)),
                Span::styled("type to filter…", state.theme.style(StyleKey::Muted)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Filter: ", state.theme.style(StyleKey::Accent)),
                Span::styled(
                    &state.review.filter,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ])
        };
        let header =
            Paragraph::new(filter_line).block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(header, chunks[0]);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(chunks[1]);

        let files = state.review_filtered_paths();
        // PR-T16 P1 — record per-row click regions. List inner area begins
        // at (body[0].x + 1, body[0].y + 1) and each row occupies 1 line.
        state.review_file_row_regions.clear();
        if body[0].width > 2 && body[0].height > 2 {
            let inner_x = body[0].x + 1;
            let inner_y = body[0].y + 1;
            let inner_w = body[0].width - 2;
            let inner_h = body[0].height - 2;
            for (idx, path) in files.iter().enumerate() {
                if idx as u16 >= inner_h {
                    break;
                }
                let rect = ratatui::layout::Rect::new(
                    inner_x,
                    inner_y + idx as u16,
                    inner_w,
                    1,
                );
                state
                    .review_file_row_regions
                    .push((path.clone(), rect));
            }
        }
        let items: Vec<ListItem> = files
            .iter()
            .enumerate()
            .map(|(idx, path)| {
                let is_selected = idx == state.review.selected_idx;
                let style = if is_selected {
                    state.theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD)
                } else {
                    state.theme.style(StyleKey::Normal)
                };

                let (status_span, snap_span) = match state.review.items.get(path) {
                    Some(it) => {
                        let status = match it.status {
                            crate::app::ReviewItemStatus::Pending => {
                                Span::styled("• ", state.theme.style(StyleKey::Muted))
                            }
                            crate::app::ReviewItemStatus::Restored => {
                                Span::styled("✓ ", state.theme.style(StyleKey::Success))
                            }
                            crate::app::ReviewItemStatus::Failed => {
                                Span::styled("! ", state.theme.style(StyleKey::Error))
                            }
                        };
                        let snap = if it.has_snapshot {
                            Span::styled("S ", state.theme.style(StyleKey::Accent))
                        } else {
                            Span::styled("- ", state.theme.style(StyleKey::Muted))
                        };
                        (status, snap)
                    }
                    None => (
                        Span::styled("• ", state.theme.style(StyleKey::Muted)),
                        Span::styled("? ", state.theme.style(StyleKey::Muted)),
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
        let diff_title = if let Some(path) = &state.review.selected_path {
            format!("Diff: {path}")
        } else {
            "Diff".to_string()
        };

        // PR-T17 / R8b — image-preview pane. When the selected path ends in a
        // known image extension we replace the text-diff body with an ASCII
        // placeholder frame that reports the filename and intrinsic pixel
        // dimensions. The frame is sized to the diff pane so it remains
        // readable at any terminal size. Native Kitty DCS emission over the
        // same bytes is wired separately in R8c once the post-frame stdout
        // hook lands; today we intentionally draw the fallback on *both*
        // kitty-supported and unsupported terminals so the UI stays
        // consistent across the feature gate.
        let image_preview_lines: Option<Vec<Line>> = state
            .review
            .selected_path
            .as_ref()
            .filter(|p| crate::services::review_preview::is_image_path(p))
            .map(|path| {
                let abs_path = if std::path::Path::new(path).is_absolute() {
                    std::path::PathBuf::from(path)
                } else {
                    state.project_root.join(path)
                };
                let inner_w = body[1].width.saturating_sub(2).max(4);
                let inner_h = body[1].height.saturating_sub(2).max(3);
                match crate::services::review_preview::prepare_image_preview(
                    &abs_path,
                    crate::services::review_preview::IMAGE_PREVIEW_MAX_BYTES,
                ) {
                    Ok(preview) => {
                        let label = format!(
                            "{} ({}x{})",
                            path, preview.width, preview.height
                        );
                        crate::services::kitty_image::render_ascii_fallback(
                            inner_w, inner_h, &label,
                        )
                        .into_iter()
                        .map(|s| Line::raw(s))
                        .collect::<Vec<Line>>()
                    }
                    Err(e) => vec![Line::from(Span::styled(
                        format!("Cannot preview image: {e}"),
                        state.theme.style(StyleKey::Error),
                    ))],
                }
            });

        let diff_lines: Vec<Line> = if let Some(lines) = image_preview_lines {
            lines
        } else if let Some(diff) = &state.review.diff {
            if let (Some(old), Some(new)) =
                (diff.old_content.as_deref(), diff.new_content.as_deref())
            {
                // PR-T15 P1 — overlay inline LSP diagnostics on new-side rows
                // when a snapshot + selected path are available.
                let selected_path = state.review.selected_path.clone();
                let path_buf = selected_path.as_deref().map(std::path::Path::new);
                crate::services::review::render_diff_viewport_with_diagnostics(
                    old,
                    new,
                    body[1].width as usize,
                    diff.scroll,
                    diff_height,
                    state.lsp_diagnostics.as_ref(),
                    path_buf,
                )
            } else if let Some(err) = &diff.last_error {
                vec![Line::from(Span::styled(
                    err.clone(),
                    state.theme.style(StyleKey::Error),
                ))]
            } else {
                vec![Line::raw("No diff loaded.")]
            }
        } else if let Some(path) = state.review.selected_path.clone()
            && let Some(it) = state.review.items.get(&path)
            && let Some(err) = &it.last_error
        {
            vec![Line::from(Span::styled(
                err.clone(),
                state.theme.style(StyleKey::Error),
            ))]
        } else {
            vec![Line::raw("Enter: toggle diff • PgUp/PgDn: scroll")]
        };

        let diff = Paragraph::new(diff_lines)
            .block(Block::default().borders(Borders::ALL).title(diff_title))
            .wrap(Wrap { trim: false });
        f.render_widget(diff, body[1]);
    }
}
