//! Render functions and helpers for VIL Issue Workstation.

use std::path::Path;

use crate::services::diagnostics_overlay::{
    HoverDetail, gutter_mark_for_line, render_gutter_cell, render_line_with_diagnostics,
    squiggly_spans_for_line,
};
use vac_core::lsp::types::LspSeverity;
use crate::services::theme::{StyleKey, Theme};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::app::{AppState, VilIssue, VilIssueKind};

/// Render the full VIL Issue Workstation tab body.
pub fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Status panel
            Constraint::Length(3), // Tabs
            Constraint::Min(1),    // Body
        ])
        .split(area);

    render_status_panel(f, state, chunks[0]);

    // PR-T16 P1 — snapshot issues into an owned Vec so subsequent refs do
    // not borrow from `state`; this frees us to mutate region fields below.
    let issues_owned: Vec<VilIssue> = super::classify_issues(state)
        .into_iter()
        .cloned()
        .collect();
    let issues: Vec<&VilIssue> = issues_owned.iter().collect();
    let counts = super::group_counts(&issues);

    // --- top strip: Semantic (N) │ ZeroCopy (N) │ ... │ All (N) ---
    let mut titles: Vec<String> = super::KIND_ORDER
        .iter()
        .map(|k| format!("{} ({})", k.label(), counts.get(k).copied().unwrap_or(0)))
        .collect();
    titles.push(format!("All ({})", issues.len()));

    let selected_tab_idx = match state.vil.workbench_group_filter {
        Some(kind) => super::KIND_ORDER
            .iter()
            .position(|k| *k == kind)
            .unwrap_or(super::KIND_ORDER.len()),
        None => super::KIND_ORDER.len(), // the "All" slot
    };

    let tabs = ratatui::widgets::Tabs::new(titles)
        .select(selected_tab_idx)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Issue Groups",
                state.theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
            )),
        )
        .highlight_style(
            state.theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[1]);

    // --- body: list (left) + lineage panel (right) ---
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[2]);

    let view = super::filtered(state, &issues);
    render_issue_list(f, state, body[0], &view);
    // PR-T16 P1 — expose the full VIL tab body as a focus-grab click region,
    // so a click anywhere inside the tab brings the workbench into focus even
    // when it misses a specific row.
    state.workbench_body_region = Some(area);
    // R7 / PR-T15 — if a hover popup is active, anchor it against the issue
    // list rect so dismissal hit-testing matches what the user sees. The
    // popup must render AFTER the list so it paints on top, and it must be
    // clamped to the overall VIL tab area so it never overflows the tab.
    let hover_to_draw = state.active_hover.clone();
    if let Some(detail) = hover_to_draw {
        render_hover_popup(f, state, body[0], area, &detail);
    } else {
        state.hover_popup_region = None;
    }
    let log_height = body[1].height.saturating_div(3).clamp(6, 12);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(body[1].height.saturating_sub(log_height)),
            Constraint::Length(log_height),
        ])
        .split(body[1]);
    render_lineage_panel(f, state, right[0], &view);
    render_vil_log_panel(f, state, right[1]);
}

fn render_status_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let score = state.vil.status.validation_score;
    let score_label = if score >= 0.9 {
        "A"
    } else if score >= 0.7 {
        "B"
    } else {
        "C"
    };

    let score_style = if score >= 0.9 {
        state.theme.style(StyleKey::ScoreGood)
    } else if score >= 0.7 {
        state.theme.style(StyleKey::ScoreOk)
    } else {
        state.theme.style(StyleKey::ScoreBad)
    };

    let active_rulebook = state
        .vil
        .status
        .active_rulebook
        .clone()
        .or_else(|| {
            if state.selected_rulebooks.is_empty() {
                None
            } else {
                let mut v = state.selected_rulebooks.iter().cloned().collect::<Vec<_>>();
                v.sort();
                Some(v.join(", "))
            }
        })
        .unwrap_or_else(|| "default".to_string());

    // Rulebook Matrix Conflict Detector
    let has_conflict = state.selected_rulebooks.len() > 1
        && (state.selected_rulebooks.contains("strict")
            && state.selected_rulebooks.contains("legacy"));

    let rulebook_display = if has_conflict {
        format!("{} [! CONFLICT DETECTED]", active_rulebook)
    } else {
        active_rulebook
    };

    let trend = ascii_sparkline(
        if state.vil.score_history.is_empty() {
            std::slice::from_ref(&score)
        } else {
            state.vil.score_history.as_slice()
        },
        24,
    );

    let mut header = vec![
        Span::styled("Score: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(score_label, score_style),
        Span::styled(
            format!(" ({:.2})", score),
            state.theme.style(StyleKey::Muted),
        ),
        Span::raw(" │ "),
        Span::styled("Trend: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(trend, state.theme.style(StyleKey::Accent)),
        Span::raw(" │ "),
        Span::styled("Issues: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(state.vil.status.validation_issues.len().to_string()),
    ];

    header.push(Span::raw(" │ "));
    header.push(Span::styled(
        "Semantic: ",
        Style::default().add_modifier(Modifier::BOLD),
    ));
    header.push(if state.vil.status.semantic_mode {
        Span::styled("On", state.theme.style(StyleKey::Success))
    } else {
        Span::styled("Off", state.theme.style(StyleKey::Muted))
    });

    let mut meta = vec![
        Span::styled(
            "Rulebook Matrix: ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            rulebook_display,
            if has_conflict {
                state.theme.style(StyleKey::Error).add_modifier(Modifier::BOLD)
            } else {
                state.theme.style(StyleKey::Accent)
            },
        ),
        Span::raw(" │ "),
        Span::styled("IR: ", Style::default().add_modifier(Modifier::BOLD)),
    ];

    if state.vil.status.ir_generation_active {
        meta.push(Span::styled(
            format!("Active ({})", state.vil.status.ir_metadata_files.len()),
            state.theme.style(StyleKey::Success),
        ));
    } else {
        meta.push(Span::styled(
            "Inactive",
            state.theme.style(StyleKey::Muted),
        ));
    }

    if let Some(profile) = &state.vil.status.profile {
        meta.push(Span::raw(" │ "));
        meta.push(Span::styled(
            "Archetype: ",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        meta.push(Span::styled(
            format!("{}", profile.archetype),
            state.theme.style(StyleKey::Accent),
        ));
    }

    let deps_line = if let Some(profile) = &state.vil.status.profile {
        let deps = compact_list(&profile.vil_deps, 5);
        let constructs = compact_list(&profile.detected_constructs, 6);
        Line::from(vec![
            Span::styled("Deps: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{} ({})", deps, profile.vil_deps.len()),
                state.theme.style(StyleKey::Normal),
            ),
            Span::raw(" │ "),
            Span::styled(
                "Constructs: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} ({})", constructs, profile.detected_constructs.len()),
                state.theme.style(StyleKey::Normal),
            ),
        ])
    } else {
        Line::styled(
            "Scanning profile...",
            state.theme.style(StyleKey::Muted).add_modifier(Modifier::ITALIC),
        )
    };

    // Validation Heatmap & Rulebook Conflict Detector
    let mut recommendations = vec![];
    if score < 0.9 {
        recommendations
            .push("Recommendation: Run Batch Repair (Campaign Mode) to resolve structural drift.");
    }
    if state.vil.status.validation_issues.len() > 10 {
        recommendations.push("Warning: High issue density. Review Rulebook Matrix for conflicts.");
    }

    let mut lines_to_render = vec![Line::from(header), Line::from(meta), deps_line];

    if !recommendations.is_empty() {
        lines_to_render.push(Line::styled(
            recommendations.join(" | "),
            state.theme.style(StyleKey::Warning).add_modifier(Modifier::ITALIC),
        ));
    }

    let p = Paragraph::new(lines_to_render).block(
        Block::default().borders(Borders::ALL).title(Span::styled(
            "VIL Workstation - Rulebook Cockpit & Validation Heatmap",
            state.theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        )),
    );

    f.render_widget(p.wrap(Wrap { trim: true }), area);
}

fn render_vil_log_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let max = area.height.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();
    let entries: Vec<_> = state.vil.event_log.iter().rev().take(max.max(1)).collect();
    for entry in entries.into_iter().rev() {
        let ts = entry.at.format("%H:%M:%S").to_string();
        lines.push(Line::from(vec![
            Span::styled(ts, state.theme.style(StyleKey::Muted)),
            Span::raw(" "),
            Span::raw(entry.message.clone()),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "no VIL events yet",
            state.theme.style(StyleKey::Muted),
        ));
    }
    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("VIL Log (tail)"),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}

fn ascii_sparkline(values: &[f64], width: usize) -> String {
    const LEVELS: &[u8] = b" .:-=+*#%@";
    if width == 0 {
        return String::new();
    }
    if values.is_empty() {
        return " ".repeat(width);
    }
    let start = values.len().saturating_sub(width);
    let slice = &values[start..];
    let mut out = String::with_capacity(width);
    if slice.len() < width {
        out.push_str(&" ".repeat(width - slice.len()));
    }
    for v in slice {
        let clamped = v.clamp(0.0, 1.0);
        let idx = (clamped * (LEVELS.len().saturating_sub(1) as f64)).round() as usize;
        out.push(LEVELS[idx] as char);
    }
    out
}

fn compact_list(items: &[String], max: usize) -> String {
    if items.is_empty() {
        return "-".to_string();
    }
    let shown = items.iter().take(max).cloned().collect::<Vec<_>>();
    if items.len() > max {
        format!("{}, +{}", shown.join(", "), items.len() - max)
    } else {
        shown.join(", ")
    }
}

fn render_issue_list(f: &mut Frame, state: &mut AppState, area: Rect, view: &[&VilIssue]) {
    // PR-T16 P1 — reset issue-row regions at the start of each render.
    state.vil_issue_row_regions.clear();

    let empty_msg = if state.vil.status.validation_issues.is_empty() {
        "No validation issues. Run /vil-status or edit a watched file."
    } else {
        "No issues in the selected group."
    };

    if view.is_empty() {
        let widget = Paragraph::new(Line::styled(
            empty_msg,
            state.theme.style(StyleKey::Muted),
        ))
        .block(Block::default().borders(Borders::ALL).title("Issues"))
        .wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    // PR-T16 P1 — populate click regions for each visible issue row. Inner
    // area is `area` minus its 1-char border. Each row is one line tall.
    if area.width > 2 && area.height > 2 {
        let inner_x = area.x + 1;
        let inner_y = area.y + 1;
        let inner_w = area.width - 2;
        let inner_h = area.height - 2;
        for idx in 0..view.len() {
            if idx as u16 >= inner_h {
                break;
            }
            state.vil_issue_row_regions.push((
                idx,
                Rect::new(inner_x, inner_y + idx as u16, inner_w, 1),
            ));
        }
    }

    let sel = state
        .vil
        .workbench_selected
        .min(view.len().saturating_sub(1));
    let items: Vec<ListItem> = view
        .iter()
        .enumerate()
        .map(|(idx, issue)| {
            let selected = idx == sel;
            let style = if selected {
                state.theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let kind_style = kind_style(&state.theme, issue.kind);
            let kind_tag = format!("[{}]", issue.kind.label());
            let locator = match (&issue.file, issue.line) {
                (Some(f), Some(l)) => format!(" {f}:{l}"),
                (Some(f), None) => format!(" {f}"),
                _ => String::new(),
            };

            // PR-T15 P1 — inline diagnostics overlay: if the LSP snapshot has
            // a diagnostic at (issue.file, issue.line), render the message
            // text via `render_line_with_diagnostics` so the severity color
            // + underline flow through the same code path as Review rows.
            let message = issue.message.clone();
            let (overlay_spans, gutter_mark): (
                Vec<crate::services::diagnostics_overlay::DiagnosticSpan>,
                Option<crate::services::diagnostics_overlay::GutterMark>,
            ) = match (&issue.file, issue.line, state.lsp_diagnostics.as_ref()) {
                (Some(file), Some(line_1based), Some(snap)) => {
                    let line0 = (line_1based.saturating_sub(1)) as u32;
                    let width = message.chars().count() as u32;
                    let spans = squiggly_spans_for_line(snap, Path::new(file), line0, width);
                    let mark = gutter_mark_for_line(snap, Path::new(file), line0);
                    (spans, mark)
                }
                _ => (Vec::new(), None),
            };

            // R6 / PR-T15 — prepend a 2-col severity gutter cell so each row
            // surfaces its highest-severity diagnostic at a glance.
            let mut spans: Vec<Span> = vec![
                render_gutter_cell(gutter_mark.as_ref(), Style::default()),
                Span::styled(kind_tag, kind_style),
                Span::styled(locator, state.theme.style(StyleKey::Accent)),
                Span::raw(" "),
            ];
            if overlay_spans.is_empty() {
                spans.push(Span::styled(message, style));
            } else {
                // Promote the whole message to an overlayed Line, then flatten
                // its spans into this row so the ListItem remains a single Line.
                let overlayed = render_line_with_diagnostics(&message, &overlay_spans, style);
                for s in overlayed.spans.into_iter() {
                    spans.push(Span::styled(s.content.into_owned(), s.style));
                }
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = format!("Issues ({})", view.len());
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(list, area);
}

fn render_lineage_panel(f: &mut Frame, state: &AppState, area: Rect, view: &[&VilIssue]) {
    let mut lines: Vec<Line> = Vec::new();

    let sel = state
        .vil
        .workbench_selected
        .min(view.len().saturating_sub(1));
    let current = view.get(sel).copied();

    if let Some(issue) = current {
        lines.push(Line::from(vec![
            Span::styled("Kind: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                issue.kind.label().to_string(),
                kind_style(&state.theme, issue.kind),
            ),
        ]));
        if let Some(file) = &issue.file {
            lines.push(Line::from(vec![
                Span::styled("Target: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(file.clone(), state.theme.style(StyleKey::Accent)),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("Source: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(issue.source.clone(), state.theme.style(StyleKey::Muted)),
            Span::styled(
                format!(" (ID: {})", issue.id),
                state.theme.style(StyleKey::Muted),
            ),
        ]));
        lines.push(Line::raw(""));
        if let Some(raw) = &issue.raw {
            for wrapped in textwrap_lines(raw, area.width.saturating_sub(2) as usize) {
                lines.push(Line::raw(wrapped));
            }
        } else {
            for wrapped in textwrap_lines(&issue.message, area.width.saturating_sub(2) as usize) {
                lines.push(Line::raw(wrapped));
            }
        }
        lines.push(Line::raw(""));

        // Semantic repair loop proposal
        if let Some(proposal) = &issue.repair_proposal {
            lines.push(Line::styled(
                "Semantic Repair Proposal:",
                state.theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::styled(
                format!("  {}", proposal),
                state.theme.style(StyleKey::Normal),
            ));
            lines.push(Line::raw(""));
        }

        // Lineage = IR-drift history: surface any `ir_metadata_files`
        // whose path mentions the target.
        let needle = issue.file.clone().unwrap_or_else(|| issue.message.clone());
        let related: Vec<&String> = state
            .vil
            .status
            .ir_metadata_files
            .iter()
            .filter(|f| {
                needle
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .filter(|tok| !tok.is_empty())
                    .any(|tok| f.contains(tok))
            })
            .collect();

        lines.push(Line::styled(
            format!("IR-drift history ({})", related.len()),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        if related.is_empty() {
            lines.push(Line::styled(
                "  (no matching IR metadata files)",
                state.theme.style(StyleKey::Muted),
            ));
        } else {
            for f in related.iter().take(8) {
                lines.push(Line::from(vec![
                    Span::styled("  • ", state.theme.style(StyleKey::Muted)),
                    Span::styled((*f).clone(), state.theme.style(StyleKey::Accent)),
                ]));
            }
            if related.len() > 8 {
                lines.push(Line::styled(
                    format!("  … and {} more", related.len() - 8),
                    state.theme.style(StyleKey::Muted),
                ));
            }
        }
    } else {
        lines.push(Line::styled(
            "No issue selected.",
            state.theme.style(StyleKey::Muted),
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "R: repair  A: audit  D: ir-diff  O: open in $EDITOR  ←/→: filter  B: batch campaign",
        state.theme.style(StyleKey::Muted),
    ));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Lineage"))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
}

/// R7 / PR-T15 — render the hover detail popup anchored near the clicked
/// issue row. The popup auto-flips horizontally when it would overflow the
/// tab area's right edge, and is clamped vertically so it never escapes the
/// tab area. The computed screen rect is recorded in
/// `state.hover_popup_region` for the next click dismissal test.
///
/// * `list_rect` — the issue list inner area; used as the horizontal anchor.
/// * `area` — the overall VIL tab area; used as the clamping box.
fn render_hover_popup(
    f: &mut Frame,
    state: &mut AppState,
    list_rect: Rect,
    area: Rect,
    detail: &HoverDetail,
) {
    if area.width < 10 || area.height < 6 {
        state.hover_popup_region = None;
        return;
    }

    // Popup size: 64 cols max, shrinks for narrow tabs. Height covers a
    // wrapped 3-line message plus header/footer and borders.
    let max_w: u16 = 64;
    let desired_w = max_w.min(area.width.saturating_sub(2));
    let width = desired_w.max(20);
    let height: u16 = 7;

    // Horizontal anchor: start 2 cols inside the list rect; auto-flip when
    // we'd overflow the tab area's right edge.
    let tab_right = area.x.saturating_add(area.width);
    let mut x = list_rect.x.saturating_add(2);
    if x.saturating_add(width) > tab_right {
        x = tab_right.saturating_sub(width).saturating_sub(1).max(area.x);
    }

    // Vertical anchor: below the list header (first row of list), clamped
    // into the tab area. We prefer mid-list so the popup doesn't cover the
    // clicked row *or* the list title.
    let tab_bottom = area.y.saturating_add(area.height);
    let mut y = list_rect.y.saturating_add(list_rect.height / 3);
    if y.saturating_add(height) > tab_bottom {
        y = tab_bottom.saturating_sub(height).max(area.y);
    }

    let popup = Rect::new(x, y, width, height);
    state.hover_popup_region = Some(popup);

    let (sev_label, sev_style) = match detail.severity {
        LspSeverity::Error => (
            "Error",
            state.theme.style(StyleKey::Error).add_modifier(Modifier::BOLD),
        ),
        LspSeverity::Warning => (
            "Warning",
            state.theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        ),
        LspSeverity::Information => (
            "Info",
            state.theme.style(StyleKey::Accent).add_modifier(Modifier::BOLD),
        ),
        LspSeverity::Hint => (
            "Hint",
            state.theme.style(StyleKey::Muted).add_modifier(Modifier::BOLD),
        ),
    };

    // Title line: "[Severity] code? source?" with the bits we have.
    let mut title_parts: Vec<Span> = vec![
        Span::styled(format!("[{}]", sev_label), sev_style),
    ];
    if let Some(code) = &detail.code {
        title_parts.push(Span::raw(" "));
        title_parts.push(Span::styled(
            code.clone(),
            state.theme.style(StyleKey::Accent),
        ));
    }
    if let Some(source) = &detail.source {
        title_parts.push(Span::raw(" · "));
        title_parts.push(Span::styled(
            source.clone(),
            state.theme.style(StyleKey::Muted),
        ));
    }

    let inner_w = width.saturating_sub(2) as usize;
    let mut body_lines: Vec<Line> = Vec::new();
    body_lines.push(Line::from(title_parts));
    body_lines.push(Line::raw(""));
    for wrapped in textwrap_lines(&detail.message, inner_w) {
        body_lines.push(Line::raw(wrapped));
    }
    body_lines.push(Line::raw(""));
    body_lines.push(Line::styled(
        "click outside to dismiss",
        state.theme.style(StyleKey::Muted).add_modifier(Modifier::ITALIC),
    ));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Diagnostic", sev_style))
        .border_style(sev_style);
    let widget = Paragraph::new(body_lines)
        .block(block)
        .wrap(Wrap { trim: true });

    // Clear the cells under the popup so underlying list rows don't bleed
    // through the borders (ratatui's default compositor overlays).
    f.render_widget(ratatui::widgets::Clear, popup);
    f.render_widget(widget, popup);
}

fn kind_style(theme: &Theme, kind: VilIssueKind) -> ratatui::style::Style {
    match kind {
        VilIssueKind::Semantic => theme.style(StyleKey::VilKindSemantic),
        VilIssueKind::ZeroCopy => theme.style(StyleKey::VilKindZeroCopy),
        VilIssueKind::Plumbing => theme.style(StyleKey::VilKindPlumbing),
        VilIssueKind::IrDrift => theme.style(StyleKey::VilKindIrDrift),
        VilIssueKind::CanonicalTerm => theme.style(StyleKey::VilKindCanonical),
        VilIssueKind::Other => theme.style(StyleKey::VilKindOther),
    }
}

fn textwrap_lines(text: &str, width: usize) -> Vec<String> {
    if width < 4 {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            out.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
