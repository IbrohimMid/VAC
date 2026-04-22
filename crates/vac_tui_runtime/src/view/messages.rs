//! Messages panel rendering

use crate::app::AppState;
use crate::app::WorkspaceFocus;
use crate::ui::style::focus_style;
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn render_messages(f: &mut Frame, state: &mut AppState, area: Rect) {
    state.message_area_y = area.y;
    state.message_area_height = area.height;

    use crate::app::types::RenderedMessageCache;
    use crate::services::message::render_tool_call_pending;
    use crate::services::message::{render_assistant_message_with_width, render_user_message};
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
    for tc in &state.approvals.pending_tool_calls {
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(ratatui::text::Span::styled(
                    "Conversation",
                    focus_style(state.focus == WorkspaceFocus::Conversation, &state.theme),
                )),
        )
        .wrap(Wrap { trim: false })
        .scroll((state.scroll as u16, 0));
    f.render_widget(widget, area);
}
