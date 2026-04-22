//! Session-resume overlay input handler (PR-T8).

use crate::app::{AppState, InputEvent, OutputEvent};
use crate::overlay::OverlayId;
use tokio::sync::mpsc::Sender;

pub(super) fn handle_session_resume(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    match event {
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(state, OverlayId::SessionResume);
            state.session_resume_query.clear();
            state.session_resume_filtered_indices.clear();
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            state.session_resume_selected = state.session_resume_selected.saturating_sub(1);
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            let count = state.session_resume_filtered_indices.len();
            state.session_resume_selected =
                (state.session_resume_selected + 1).min(count.saturating_sub(1));
        }
        InputEvent::InputChanged(ch) => {
            state.session_resume_query.push(ch);
            state.session_resume_selected = 0;
            refresh_session_resume_filtered(state);
        }
        InputEvent::InputBackspace => {
            state.session_resume_query.pop();
            state.session_resume_selected = 0;
            refresh_session_resume_filtered(state);
        }
        InputEvent::Tab => {
            // Cycle date filter: all → 7d → 30d → 90d → all
            state.session_resume_date_filter_days = match state.session_resume_date_filter_days {
                None => Some(7),
                Some(7) => Some(30),
                Some(30) => Some(90),
                Some(_) => None,
            };
            state.session_resume_selected = 0;
            refresh_session_resume_filtered(state);
        }
        InputEvent::InputSubmitted => {
            let idx = state
                .session_resume_filtered_indices
                .get(state.session_resume_selected)
                .copied();
            if let Some(i) = idx {
                if let Some(entry) = state.session_resume_list.get(i) {
                    let id = entry.session_id;
                    let _ = output_tx.try_send(OutputEvent::ResumeSession(id.to_string()));
                }
            }
            crate::overlay::close_overlay(state, OverlayId::SessionResume);
            state.session_resume_query.clear();
            state.session_resume_filtered_indices.clear();
        }
        _ => {}
    }
}

/// Recompute `session_resume_filtered_indices` from current query + date filter.
/// Uses nucleo fuzzy scoring on `"{project}/{title} — {last_message_preview}"`.
pub(crate) fn refresh_session_resume_filtered(state: &mut AppState) {
    use nucleo_matcher::{
        Matcher, Utf32Str,
        pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    };

    let cutoff = state
        .session_resume_date_filter_days
        .map(|days| chrono::Utc::now() - chrono::Duration::days(days as i64));

    let q = state.session_resume_query.trim();

    if q.is_empty() {
        // No query — return all entries passing date filter, sorted newest first
        let mut indices: Vec<usize> = state
            .session_resume_list
            .iter()
            .enumerate()
            .filter(|(_, e)| cutoff.map_or(true, |c| e.last_active > c))
            .map(|(i, _)| i)
            .collect();
        indices.sort_by(|&a, &b| {
            state.session_resume_list[b]
                .last_active
                .cmp(&state.session_resume_list[a].last_active)
        });
        state.session_resume_filtered_indices = indices;
        return;
    }

    let pattern = Pattern::new(
        q,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
    let mut utf32buf = Vec::new();

    let mut scored: Vec<(u32, usize)> = state
        .session_resume_list
        .iter()
        .enumerate()
        .filter(|(_, e)| cutoff.map_or(true, |c| e.last_active > c))
        .filter_map(|(i, e)| {
            let haystack_str = format!(
                "{}/{} \u{2014} {}",
                e.project, e.title, e.last_message_preview
            );
            let haystack = Utf32Str::new(&haystack_str, &mut utf32buf);
            pattern.score(haystack, &mut matcher).map(|s| (s, i))
        })
        .collect();

    // Sort descending by score, then descending by last_active for ties
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| {
            state.session_resume_list[b.1]
                .last_active
                .cmp(&state.session_resume_list[a.1].last_active)
        })
    });

    state.session_resume_filtered_indices = scored.into_iter().map(|(_, i)| i).collect();
}
