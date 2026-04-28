//! Detail pane rendering helpers for VIL Issue Workstation.
//!
//! Extracted from `render.rs` to keep that file focused on the main
//! layout (status panel, issue list, top-level `render()` entry point).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{AppState, VilIssue, VilIssueKind};
use crate::services::diagnostics_overlay::HoverDetail;
use crate::services::theme::{StyleKey, Theme};
use vac_core::lsp::types::LspSeverity;

/// Style for a VIL issue kind badge.
pub fn kind_style(theme: &Theme, kind: VilIssueKind) -> Style {
    match kind {
        VilIssueKind::Semantic => theme.style(StyleKey::Error),
        VilIssueKind::ZeroCopy => theme.style(StyleKey::Warning),
        VilIssueKind::Plumbing => theme.style(StyleKey::Accent),
        VilIssueKind::IrDrift => theme.style(StyleKey::Warning),
        VilIssueKind::CanonicalTerm => theme.style(StyleKey::Muted),
        VilIssueKind::Other => theme.style(StyleKey::Normal),
    }
}

/// Render a floating hover popup showing LSP diagnostic detail.
pub fn render_hover_popup(
    f: &mut Frame,
    state: &mut AppState,
    list_area: Rect,
    clamp_area: Rect,
    detail: &HoverDetail,
) {
    let severity_style = match detail.severity {
        LspSeverity::Error => state.core.theme.style(StyleKey::Error),
        LspSeverity::Warning => state.core.theme.style(StyleKey::Warning),
        LspSeverity::Information => state.core.theme.style(StyleKey::Accent),
        LspSeverity::Hint => state.core.theme.style(StyleKey::Muted),
    };

    let mut lines: Vec<Line> = Vec::new();

    // Header: severity + optional code
    let header = if let Some(code) = &detail.code {
        format!("[{:?}] {}", detail.severity, code)
    } else {
        format!("[{:?}]", detail.severity)
    };
    lines.push(Line::from(Span::styled(
        header,
        severity_style.add_modifier(Modifier::BOLD),
    )));

    // Source
    if let Some(src) = &detail.source {
        lines.push(Line::from(Span::styled(
            src.clone(),
            state.core.theme.style(StyleKey::Muted),
        )));
    }

    // Message — wrapped at 50 chars
    for l in textwrap_lines(&detail.message, 50) {
        lines.push(Line::from(Span::raw(l)));
    }

    let popup_h = (lines.len() + 2) as u16;
    let popup_w = 54u16;

    // Anchor below the hovered row, clamped to clamp_area
    let anchor_y = list_area
        .y
        .saturating_add(detail.line_index as u16 + 2)
        .min(clamp_area.y + clamp_area.height.saturating_sub(popup_h));
    let anchor_x = list_area.x.min(
        clamp_area
            .x
            .saturating_add(clamp_area.width.saturating_sub(popup_w)),
    );

    let popup_rect = Rect::new(
        anchor_x.clamp(
            clamp_area.x,
            clamp_area.x + clamp_area.width.saturating_sub(popup_w),
        ),
        anchor_y.clamp(
            clamp_area.y,
            clamp_area.y + clamp_area.height.saturating_sub(popup_h),
        ),
        popup_w.min(clamp_area.width),
        popup_h.min(clamp_area.height),
    );

    state.layout.lsp_ui.hover_popup_region = Some(popup_rect);

    f.render_widget(Clear, popup_rect);
    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(severity_style)
                .title(Span::styled(
                    "Hover",
                    severity_style.add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(para, popup_rect);
}

/// Render the lineage / detail panel on the right side.
pub fn render_lineage_panel(f: &mut Frame, state: &AppState, area: Rect, view: &[&VilIssue]) {
    if view.is_empty() {
        let widget = Paragraph::new(Line::styled(
            "Select an issue to see details",
            state.core.theme.style(StyleKey::Muted),
        ))
        .block(Block::default().borders(Borders::ALL).title("Details"))
        .wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    let sel = state.vil_domain.vil.workbench_selected.min(view.len() - 1);
    let issue = view[sel];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(0)])
        .split(area);

    // Top: issue metadata
    let mut meta_lines = vec![
        Line::from(vec![
            Span::styled("Kind: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                issue.kind.label().to_string(),
                kind_style(&state.core.theme, issue.kind),
            ),
        ]),
        Line::from(vec![
            Span::styled("Source: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                issue.source.clone(),
                state.core.theme.style(StyleKey::Muted),
            ),
        ]),
    ];
    if let (Some(f), Some(l)) = (&issue.file, issue.line) {
        meta_lines.push(Line::from(vec![
            Span::styled("Location: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{}:{}", f, l),
                state.core.theme.style(StyleKey::Accent),
            ),
        ]));
    }
    if issue.inferred {
        meta_lines.push(Line::from(Span::styled(
            "[inferred]",
            state
                .core
                .theme
                .style(StyleKey::Muted)
                .add_modifier(Modifier::ITALIC),
        )));
    }
    let meta = Paragraph::new(meta_lines)
        .block(Block::default().borders(Borders::ALL).title("Metadata"))
        .wrap(Wrap { trim: true });
    f.render_widget(meta, chunks[0]);

    // Bottom: repair proposal or raw message
    let detail_text: Vec<Line> = if let Some(repair) = &issue.repair_proposal {
        let mut lines = vec![Line::from(Span::styled(
            "Repair suggestion:",
            state
                .core
                .theme
                .style(StyleKey::Success)
                .add_modifier(Modifier::BOLD),
        ))];
        for l in textwrap_lines(repair, (chunks[1].width.saturating_sub(4)) as usize) {
            lines.push(Line::from(Span::raw(l)));
        }
        lines
    } else if let Some(raw) = &issue.raw {
        textwrap_lines(raw, (chunks[1].width.saturating_sub(4)) as usize)
            .into_iter()
            .map(|l| Line::from(Span::raw(l)))
            .collect()
    } else {
        textwrap_lines(&issue.message, (chunks[1].width.saturating_sub(4)) as usize)
            .into_iter()
            .map(|l| Line::from(Span::raw(l)))
            .collect()
    };

    let detail_para = Paragraph::new(detail_text)
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: false });
    f.render_widget(detail_para, chunks[1]);
}

/// Word-wrap `text` to `max_width` Unicode scalar values per line.
pub fn textwrap_lines(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.chars().count() <= max_width {
            result.push(paragraph.to_string());
            continue;
        }
        let mut current = String::new();
        let mut current_chars = 0usize;
        for word in paragraph.split_whitespace() {
            let word_chars = word.chars().count();
            if current.is_empty() {
                current.push_str(word);
                current_chars = word_chars;
            } else if current_chars + 1 + word_chars <= max_width {
                current.push(' ');
                current.push_str(word);
                current_chars += 1 + word_chars;
            } else {
                result.push(std::mem::take(&mut current));
                current.push_str(word);
                current_chars = word_chars;
            }
        }
        if !current.is_empty() {
            result.push(current);
        }
    }
    result
}
