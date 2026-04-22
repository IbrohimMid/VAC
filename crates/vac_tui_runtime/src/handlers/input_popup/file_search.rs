//! File-search and file-picker overlay input handlers (PR-T6).

use crate::app::{AppState, InputEvent, OutputEvent};
use crate::handlers::HandlerContext;
use crate::handlers::file_search;
use crate::overlay::OverlayId;
use tokio::sync::mpsc::Sender;

pub(super) fn handle_file_search(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    if state.all_files.is_empty() {
        state.all_files = crate::services::build_file_index(&state.project_root);
    }
    if state.file_search_results.is_empty() {
        let q = state.file_search_query.clone();
        let results = crate::services::fuzzy_search_files(&q, &state.all_files, 50);
        let max = results.len().saturating_sub(1);
        state.file_search_results = results;
        state.file_search_selected_idx = state.file_search_selected_idx.min(max);
    }
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => {
            let _ = file_search::close(&mut ctx);
        }
        InputEvent::InputChanged(c) => {
            let mut q = ctx.state.file_search_query.clone();
            q.push(c);
            let _ = file_search::update_query(&mut ctx, q);
        }
        InputEvent::InputBackspace => {
            let mut q = ctx.state.file_search_query.clone();
            q.pop();
            let _ = file_search::update_query(&mut ctx, q);
        }
        InputEvent::Up => {
            let _ = file_search::select_prev(&mut ctx);
        }
        InputEvent::Down => {
            let _ = file_search::select_next(&mut ctx);
        }
        InputEvent::InputSubmitted => {
            let _ = file_search::insert_selected(&mut ctx);
        }
        _ => {}
    }
}

pub(super) fn handle_file_picker(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    match event {
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(state, OverlayId::FilePicker);
            state.file_picker_multi_selected.clear();
            state.file_picker_query.clear();
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            state.file_picker_selected = state.file_picker_selected.saturating_sub(1);
            update_file_picker_preview(state);
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            let len = state.file_picker_results.len();
            if len > 0 {
                state.file_picker_selected =
                    (state.file_picker_selected + 1).min(len.saturating_sub(1));
                update_file_picker_preview(state);
            }
        }
        // Space toggles multi-selection
        InputEvent::InputChanged(' ') => {
            let idx = state.file_picker_selected;
            if state.file_picker_multi_selected.contains(&idx) {
                state.file_picker_multi_selected.remove(&idx);
            } else if state
                .file_picker_results
                .get(idx)
                .map(|p| p.is_file())
                .unwrap_or(false)
            {
                state.file_picker_multi_selected.insert(idx);
            }
        }
        // Tab: navigate into directory
        InputEvent::Tab => {
            if let Some(path) = state
                .file_picker_results
                .get(state.file_picker_selected)
                .cloned()
            {
                if path.is_dir() {
                    state.file_picker_cwd = path;
                    state.file_picker_selected = 0;
                    state.file_picker_multi_selected.clear();
                    refresh_file_picker_results(state);
                }
            }
        }
        // Backspace on empty query: go up a dir
        InputEvent::InputBackspace => {
            if state.file_picker_query.is_empty() {
                if let Some(parent) = state.file_picker_cwd.parent().map(|p| p.to_path_buf()) {
                    state.file_picker_cwd = parent;
                    state.file_picker_selected = 0;
                    refresh_file_picker_results(state);
                }
            } else {
                state.file_picker_query.pop();
                state.file_picker_selected = 0;
                refresh_file_picker_results(state);
            }
        }
        InputEvent::InputChanged(ch) => {
            state.file_picker_query.push(ch);
            state.file_picker_selected = 0;
            refresh_file_picker_results(state);
        }
        InputEvent::InputSubmitted => {
            let selected: Vec<std::path::PathBuf> = if state.file_picker_multi_selected.is_empty() {
                state
                    .file_picker_results
                    .get(state.file_picker_selected)
                    .filter(|p| p.is_file())
                    .cloned()
                    .into_iter()
                    .collect()
            } else {
                let mut sel: Vec<_> = state.file_picker_multi_selected.iter().copied().collect();
                sel.sort();
                sel.into_iter()
                    .filter_map(|i| state.file_picker_results.get(i))
                    .filter(|p| p.is_file())
                    .cloned()
                    .collect()
            };
            if !selected.is_empty() {
                let _ = output_tx.try_send(OutputEvent::FilesAttached(selected));
            }
            crate::overlay::close_overlay(state, OverlayId::FilePicker);
            state.file_picker_multi_selected.clear();
            state.file_picker_query.clear();
        }
        _ => {}
    }
}

pub fn refresh_file_picker_results_pub(state: &mut AppState) {
    refresh_file_picker_results(state);
}

fn refresh_file_picker_results(state: &mut AppState) {
    let query = state.file_picker_query.to_lowercase();
    let type_filter = state.file_picker_type_filter.clone();
    let cwd = state.file_picker_cwd.clone();

    let mut results: Vec<std::path::PathBuf> = std::fs::read_dir(&cwd)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            // type filter (glob-style extension)
            if let Some(ref ext_pat) = type_filter {
                if p.is_file() {
                    let matches = p
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| ext_pat.contains(e))
                        .unwrap_or(false);
                    if !matches {
                        return false;
                    }
                }
            }
            // name query filter
            if query.is_empty() {
                return true;
            }
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase().contains(&query))
                .unwrap_or(false)
        })
        .collect();

    results.sort_by(|a, b| {
        // dirs first, then files
        match (a.is_dir(), b.is_dir()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });
    state.file_picker_results = results;
    update_file_picker_preview(state);
}

fn update_file_picker_preview(state: &mut AppState) {
    let preview = state
        .file_picker_results
        .get(state.file_picker_selected)
        .filter(|p| p.is_file())
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|content| content.lines().take(40).collect::<Vec<_>>().join("\n"));
    state.file_picker_preview = preview;
}
