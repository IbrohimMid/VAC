//! Render functions and helpers for VIL Issue Workstation.

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

    let issues = super::classify_issues(state);
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

fn render_issue_list(f: &mut Frame, state: &AppState, area: Rect, view: &[&VilIssue]) {
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
            ListItem::new(Line::from(vec![
                Span::styled(kind_tag, kind_style),
                Span::styled(locator, state.theme.style(StyleKey::Accent)),
                Span::raw(" "),
                Span::styled(issue.message.clone(), style),
            ]))
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
