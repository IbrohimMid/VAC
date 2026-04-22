//! Review tab — inspect and revert file changes.

use super::WorkbenchTabView;
use crate::app::AppState;
use crate::services::theme::StyleKey;
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
                let rect = ratatui::layout::Rect::new(inner_x, inner_y + idx as u16, inner_w, 1);
                state.review_file_row_regions.push((path.clone(), rect));
            }
        }
        let items: Vec<ListItem> = files
            .iter()
            .enumerate()
            .map(|(idx, path)| {
                let is_selected = idx == state.review.selected_idx;
                let style = if is_selected {
                    state
                        .theme
                        .style(StyleKey::Warning)
                        .add_modifier(Modifier::BOLD)
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

        // PR-T17 / R8b + M1 — image-preview pane. When the selected path
        // ends in a known image extension we replace the text-diff body
        // with an ASCII placeholder frame that reports the filename and
        // intrinsic pixel dimensions. The frame is sized to the diff pane
        // so it remains readable at any terminal size.
        //
        // M1: we no longer call `prepare_image_preview` directly here.
        // That function does synchronous disk I/O + PNG header decode, so
        // invoking it from inside `terminal.draw` stalled the tokio
        // runtime until the read completed. The render path is now pure:
        // it looks the absolute path up in `state.image_preview_cache`
        // and either renders the cached result, renders a deterministic
        // error line, or renders a "Loading…" placeholder while asking
        // the cache to schedule a background load. The load itself runs
        // on a short-lived OS thread owned by the cache; its result is
        // delivered through an mpsc channel that the event loop drains
        // via `drain_pending` before the next frame.
        //
        // R8c continues to work as before: once the cache has a
        // successful preview we hand the raw PNG bytes to
        // `pending_kitty_emission` so the post-draw flush can emit a
        // native Kitty DCS sequence on top of the ASCII fallback.
        let image_branch: Option<(Vec<Line>, Option<Vec<u8>>)> = state
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
                use crate::services::image_preview_cache::ImagePreviewCacheEntry;
                match state.image_preview_cache.get(&abs_path).cloned() {
                    Some(ImagePreviewCacheEntry::Ready(Ok(preview))) => {
                        let label = format!("{} ({}x{})", path, preview.width, preview.height);
                        let lines: Vec<Line> = crate::services::kitty_image::render_ascii_fallback(
                            inner_w, inner_h, &label,
                        )
                        .into_iter()
                        .map(|s| Line::raw(s))
                        .collect();
                        (lines, Some(preview.bytes))
                    }
                    Some(ImagePreviewCacheEntry::Ready(Err(e))) => (
                        vec![Line::from(Span::styled(
                            format!("Cannot preview image: {e}"),
                            state.theme.style(StyleKey::Error),
                        ))],
                        None,
                    ),
                    Some(ImagePreviewCacheEntry::Loading) | None => {
                        // Schedule the load on first sight; subsequent
                        // frames observe `Loading` and short-circuit the
                        // spawn inside `request_load`.
                        state.image_preview_cache.request_load(abs_path.clone());
                        let label = format!("{} (loading…)", path);
                        let lines: Vec<Line> = crate::services::kitty_image::render_ascii_fallback(
                            inner_w, inner_h, &label,
                        )
                        .into_iter()
                        .map(|s| Line::raw(s))
                        .collect();
                        (lines, None)
                    }
                }
            });
        let (image_preview_lines, image_preview_bytes): (Option<Vec<Line>>, Option<Vec<u8>>) =
            match image_branch {
                Some((lines, bytes)) => (Some(lines), bytes),
                None => (None, None),
            };

        let diff_lines: Vec<Line> = if let Some(lines) = image_preview_lines {
            lines
        } else if let Some(diff) = &state.review.diff {
            if let (Some(old), Some(new)) =
                (diff.old_content.as_deref(), diff.new_content.as_deref())
            {
                // T13: VIL-aware diff for .vwfd.yaml files
                let is_vwfd = state
                    .review
                    .selected_path
                    .as_deref()
                    .map(|p| p.ends_with(".vwfd.yaml") || p.ends_with(".vwfd.yml"))
                    .unwrap_or(false);
                if is_vwfd {
                    if let (Ok(old_doc), Ok(new_doc)) =
                        (vil_vwfd::from_yaml(old), vil_vwfd::from_yaml(new))
                    {
                        let vwfd_diff = vac_changeset::formats::vwfd::diff(&old_doc, &new_doc);
                        crate::services::vwfd_diff_render::build_lines(&vwfd_diff, &state.theme)
                            .into_iter()
                            .map(|l| {
                                Line::from(
                                    l.spans
                                        .into_iter()
                                        .map(|s| Span::styled(s.content.into_owned(), s.style))
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .collect()
                    } else {
                        // Fallback to generic diff if VWFD parse fails
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
                    }
                } else {
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
                }
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

        // PR-T17 R8c — queue native Kitty graphics emission for this frame
        // only when the startup probe confirmed support. The stored tuple is
        // `(target rect, raw PNG bytes)`; the event loop flushes it after
        // `terminal.draw()` via `emit_positioned_kitty_image`, which both
        // positions the cursor and encodes the DCS payload. Keeping raw PNG
        // bytes here (rather than pre-encoded DCS) means the emission path
        // is exercised end-to-end by kitty_image tests. On non-Kitty
        // terminals we do not populate the field so no escape bytes ever
        // leak to stdout.
        if state.startup.kitty_graphics {
            if let Some(bytes) = image_preview_bytes {
                if !bytes.is_empty() {
                    state.pending_kitty_emission = Some((body[1], bytes));
                }
            }
        }
    }
}
