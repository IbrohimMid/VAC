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
    state.layout.message_ui.message_area_y = area.y;
    state.layout.message_ui.message_area_height = area.height;

    use crate::app::types::RenderedMessageCache;
    use crate::services::message::render_tool_call_pending;
    use crate::services::message::{render_assistant_message_with_width, render_user_message};
    use std::sync::Arc;

    let width = area.width.saturating_sub(2) as usize; // account for border
    let mut lines: Vec<Line<'static>> = Vec::new();

    let mut hits = 0;
    let mut misses = 0;

    // Prune cache to a max size (e.g. 100) to act as LRU-ish
    if state.layout.message_ui.per_message_cache.len() > 200 {
        // Just clear it if it gets too big for now
        state.layout.message_ui.per_message_cache.clear();
    }

    state.layout.message_ui.line_to_message_map.clear();
    for msg in &state.transcript.messages {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        msg.content.hash(&mut hasher);
        msg.role.hash(&mut hasher);
        let content_hash = hasher.finish();

        if let Some(cached) = state.layout.message_ui.per_message_cache.get(&msg.id) {
            if cached.content_hash == content_hash && cached.width == width {
                hits += 1;
                let n = cached.rendered_lines.len();
                lines.extend(cached.rendered_lines.iter().cloned());
                lines.push(Line::raw(""));
                for _ in 0..=n {
                    state.layout.message_ui.line_to_message_map.push(msg.id);
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
        state.layout.message_ui.per_message_cache.insert(
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
            state.layout.message_ui.line_to_message_map.push(msg.id);
        }
    }

    state.core.render_metrics.cache_hits += hits;
    state.core.render_metrics.cache_misses += misses;

    // Wave 3 #04 — inline approval card. If there's a pending approval,
    // render it as a bordered block at the tail of the transcript so
    // the operator sees the decision in the same flow as the tool
    // request that produced it. Shows only the head of the queue; the
    // rest remain accessible via the workbench approvals tab.
    let pending_approvals = &state.execution.approvals.pending_approvals;
    if let Some(head) = pending_approvals.first() {
        lines.push(Line::raw(""));
        lines.extend(crate::services::message::render_approval_card(
            head,
            0,
            pending_approvals.len(),
            width,
        ));
    }

    // Wave 3 #03 — tool timeline. Render a single grouped block beneath
    // the transcript covering (a) in-flight / queued tool calls and
    // (b) a live streaming / thinking indicator with a context-budget
    // bar. Emits only when something is actually pending so the idle
    // state stays clean.
    let pending = &state.execution.approvals.pending_tool_calls;
    let streaming = state.transcript.streaming.is_streaming;
    if !pending.is_empty() || streaming {
        lines.push(Line::raw(""));
        lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
            "  ── tool timeline ──",
            state.core.theme.style(crate::services::theme::StyleKey::Muted),
        )));
        for tc in pending {
            lines.extend(render_tool_call_pending(tc));
        }
        if streaming {
            let label = if pending.is_empty() {
                "thinking"
            } else {
                "reviewing tool results"
            };
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("  "),
                ratatui::text::Span::styled(
                    "✦ ",
                    state.core.theme.style(crate::services::theme::StyleKey::Accent),
                ),
                ratatui::text::Span::styled(
                    label.to_string(),
                    state.core.theme.style(crate::services::theme::StyleKey::Muted),
                ),
                ratatui::text::Span::styled(
                    " ▊",
                    state.core.theme.style(crate::services::theme::StyleKey::Accent),
                ),
            ]));
        }
        // Context budget bar. Sources the current token total from the
        // billing facet and renders a 20-cell bar against a 200k budget
        // (the order-of-magnitude context window for current models).
        let used = state.operator_config.billing.total_session.total_tokens as f64;
        let budget = 200_000f64;
        let frac = (used / budget).clamp(0.0, 1.0);
        let filled = (frac * 20.0).round() as usize;
        let bar: String = std::iter::repeat('▓')
            .take(filled)
            .chain(std::iter::repeat('░').take(20 - filled))
            .collect();
        lines.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::raw("  "),
            ratatui::text::Span::styled(
                "context ",
                state.core.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            ratatui::text::Span::styled(
                bar,
                state.core.theme.style(crate::services::theme::StyleKey::Accent),
            ),
            ratatui::text::Span::styled(
                format!(" {} / 200k", used as u64),
                state.core.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            ratatui::text::Span::styled(
                "  · esc to interrupt",
                state.core.theme.style(crate::services::theme::StyleKey::Muted),
            ),
        ]));
    }

    // Cache the lines for text selection
    state.layout.message_ui.assembled_lines_cache = Some((state.transcript.messages.clone(), width, lines.clone()));

    // Apply text selection highlight
    let highlighted_lines = crate::services::text_selection::apply_selection_highlight(
        lines,
        &state.composer.selection_state,
        state.layout.scroll.messages,
    );

    let widget = Paragraph::new(highlighted_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(ratatui::text::Span::styled(
                    "Conversation",
                    focus_style(state.layout.focus == WorkspaceFocus::Conversation, &state.core.theme),
                )),
        )
        .wrap(Wrap { trim: false })
        .scroll((state.layout.scroll.messages as u16, 0));
    f.render_widget(widget, area);
}
