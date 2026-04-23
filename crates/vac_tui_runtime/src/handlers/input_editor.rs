use crate::app::AppState;

/// Resolve a screen row to the message ID at that position.
pub fn message_at_row(state: &AppState, row: u16) -> Option<uuid::Uuid> {
    let row_in_area = (row as usize)
        .checked_sub(state.layout.message_ui.message_area_y as usize)?
        .checked_sub(1)?;
    let line_idx = row_in_area + state.layout.scroll.messages;

    if let Some(id) = state.layout.message_ui.line_to_message_map.get(line_idx).copied() {
        return Some(id);
    }

    let mut cumulative = 0usize;
    for msg in &state.transcript.messages {
        let msg_lines = state
            .layout.message_ui.per_message_cache
            .get(&msg.id)
            .map(|c| c.rendered_lines.len() + 1)
            .unwrap_or(1);
        if line_idx < cumulative + msg_lines {
            return Some(msg.id);
        }
        cumulative += msg_lines;
    }
    None
}

pub fn plan_open_editor(state: &mut AppState) {
    crate::handlers::plan::open_editor(state);
}

pub fn plan_write_status(state: &mut AppState, new_status: crate::services::plan::PlanStatus) {
    crate::handlers::plan::write_plan_status(state, new_status);
}
