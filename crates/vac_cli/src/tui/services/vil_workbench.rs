//! VIL Issue Workstation renderer and grouping logic (Wave 4.1, Unit 9).
//!
//! Consumes `AppState.vil_status.validation_issues` (Vec<String>), infers an
//! issue kind per entry via [`VilIssueKind::classify`], and renders a
//! tabs-within-tab UI with a scrollable issue list on the left and a lineage
//! panel on the right.
//!
//! Quick-actions (`R`/`A`/`D`/`O`) are dispatched via the sibling handler
//! module [`crate::tui::handlers::vil_workbench`].

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
};

use crate::tui::app::{AppState, VilIssueKind};

/// Ordered list of kinds surfaced in the top strip.
pub const KIND_ORDER: &[VilIssueKind] = &[
    VilIssueKind::Semantic,
    VilIssueKind::ZeroCopy,
    VilIssueKind::Plumbing,
    VilIssueKind::IrDrift,
    VilIssueKind::CanonicalTerm,
    VilIssueKind::Other,
];

/// Parsed validation-issue row ready for rendering / dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedIssue {
    pub raw: String,
    pub kind: VilIssueKind,
    /// Best-effort extracted file path (derived from the issue text).
    pub file: Option<String>,
    /// Best-effort line hint (always `None` today — `vil_validate` doesn't
    /// emit line numbers yet; reserved for future structured issues).
    pub line: Option<usize>,
    /// Compact one-liner shown in the list.
    pub short: String,
}

impl ClassifiedIssue {
    pub fn from_raw(raw: String) -> Self {
        let kind = VilIssueKind::classify(&raw);
        let file = extract_file_hint(&raw);
        let short = shorten(&raw);
        Self {
            raw,
            kind,
            file,
            line: None,
            short,
        }
    }
}

/// Try to extract a file name / handler name from the issue text.
///
/// `vil_validate` embeds identifiers in single quotes (e.g. `Handler
/// 'create_user'` or `Module 'user'`). We return the first quoted token.
fn extract_file_hint(text: &str) -> Option<String> {
    for (i, c) in text.char_indices() {
        if c == '\'' {
            let rest = &text[i + 1..];
            if let Some(end) = rest.find('\'') {
                let name = &rest[..end];
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn shorten(text: &str) -> String {
    // Collapse internal whitespace and trim to a single line for the list row.
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX: usize = 160;
    if collapsed.len() <= MAX {
        collapsed
    } else {
        // '…' is 3 bytes in UTF-8, so trim to MAX-3 to stay within budget.
        let mut t = collapsed;
        t.truncate(MAX.saturating_sub(3));
        t.push('…');
        t
    }
}

/// Group all issues from state into `ClassifiedIssue`s (in original order).
pub fn classify_issues(state: &AppState) -> Vec<ClassifiedIssue> {
    state
        .vil_status
        .validation_issues
        .iter()
        .cloned()
        .map(ClassifiedIssue::from_raw)
        .collect()
}

/// Count issues per kind.
pub fn group_counts(issues: &[ClassifiedIssue]) -> std::collections::HashMap<VilIssueKind, usize> {
    let mut m: std::collections::HashMap<VilIssueKind, usize> = std::collections::HashMap::new();
    for issue in issues {
        *m.entry(issue.kind).or_insert(0) += 1;
    }
    m
}

/// Apply the state's active filter to the classified issue list.
pub fn filtered<'a>(state: &AppState, issues: &'a [ClassifiedIssue]) -> Vec<&'a ClassifiedIssue> {
    match state.vil_workbench_group_filter {
        Some(kind) => issues.iter().filter(|i| i.kind == kind).collect(),
        None => issues.iter().collect(),
    }
}

/// Return the currently-selected issue, if any, honoring the active filter.
pub fn selected_issue(state: &AppState) -> Option<ClassifiedIssue> {
    let issues = classify_issues(state);
    let view = filtered(state, &issues);
    view.get(state.vil_workbench_selected).map(|i| (*i).clone())
}

/// Render the full VIL Issue Workstation tab body.
pub fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let issues = classify_issues(state);
    let counts = group_counts(&issues);

    // --- top strip: Semantic (N) │ ZeroCopy (N) │ ... │ All (N) ---
    let mut titles: Vec<String> = KIND_ORDER
        .iter()
        .map(|k| format!("{} ({})", k.label(), counts.get(k).copied().unwrap_or(0)))
        .collect();
    titles.push(format!("All ({})", issues.len()));

    let selected_tab_idx = match state.vil_workbench_group_filter {
        Some(kind) => KIND_ORDER
            .iter()
            .position(|k| *k == kind)
            .unwrap_or(KIND_ORDER.len()),
        None => KIND_ORDER.len(), // the "All" slot
    };

    let tabs = Tabs::new(titles)
        .select(selected_tab_idx)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "VIL Issues",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    // --- body: list (left) + lineage panel (right) ---
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[1]);

    let view = filtered(state, &issues);
    render_issue_list(f, state, body[0], &view);
    render_lineage_panel(f, state, body[1], &view);
}

fn render_issue_list(f: &mut Frame, state: &AppState, area: Rect, view: &[&ClassifiedIssue]) {
    let empty_msg = if state.vil_status.validation_issues.is_empty() {
        "No validation issues. Run /vil-status or edit a watched file."
    } else {
        "No issues in the selected group."
    };

    if view.is_empty() {
        let widget = Paragraph::new(Line::styled(
            empty_msg,
            Style::default().fg(Color::DarkGray),
        ))
        .block(Block::default().borders(Borders::ALL).title("Issues"))
        .wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    let sel = state
        .vil_workbench_selected
        .min(view.len().saturating_sub(1));
    let items: Vec<ListItem> = view
        .iter()
        .enumerate()
        .map(|(idx, issue)| {
            let selected = idx == sel;
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let kind_color = kind_color(issue.kind);
            let kind_tag = format!("[{}]", issue.kind.label());
            let locator = match (&issue.file, issue.line) {
                (Some(f), Some(l)) => format!(" {f}:{l}"),
                (Some(f), None) => format!(" {f}"),
                _ => String::new(),
            };
            ListItem::new(Line::from(vec![
                Span::styled(kind_tag, Style::default().fg(kind_color)),
                Span::styled(locator, Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled(issue.short.clone(), style),
            ]))
        })
        .collect();

    let title = format!("Issues ({})", view.len());
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(list, area);
}

fn render_lineage_panel(f: &mut Frame, state: &AppState, area: Rect, view: &[&ClassifiedIssue]) {
    let mut lines: Vec<Line> = Vec::new();

    let sel = state
        .vil_workbench_selected
        .min(view.len().saturating_sub(1));
    let current = view.get(sel).copied();

    if let Some(issue) = current {
        lines.push(Line::from(vec![
            Span::styled("Kind: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                issue.kind.label().to_string(),
                Style::default().fg(kind_color(issue.kind)),
            ),
        ]));
        if let Some(file) = &issue.file {
            lines.push(Line::from(vec![
                Span::styled("Target: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(file.clone(), Style::default().fg(Color::Cyan)),
            ]));
        }
        lines.push(Line::raw(""));
        for wrapped in textwrap_lines(&issue.raw, area.width.saturating_sub(2) as usize) {
            lines.push(Line::raw(wrapped));
        }
        lines.push(Line::raw(""));

        // Lineage = IR-drift history: surface any `ir_metadata_files`
        // whose path mentions the target.
        let needle = issue.file.clone().unwrap_or_else(|| issue.raw.clone());
        let related: Vec<&String> = state
            .vil_status
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
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            for f in related.iter().take(8) {
                lines.push(Line::from(vec![
                    Span::styled("  • ", Style::default().fg(Color::DarkGray)),
                    Span::styled((*f).clone(), Style::default().fg(Color::Cyan)),
                ]));
            }
            if related.len() > 8 {
                lines.push(Line::styled(
                    format!("  … and {} more", related.len() - 8),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
    } else {
        lines.push(Line::styled(
            "No issue selected.",
            Style::default().fg(Color::DarkGray),
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "R: repair  A: audit  D: ir-diff  O: open in $EDITOR  ←/→: filter",
        Style::default().fg(Color::DarkGray),
    ));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Lineage"))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
}

fn kind_color(kind: VilIssueKind) -> Color {
    match kind {
        VilIssueKind::Semantic => Color::Magenta,
        VilIssueKind::ZeroCopy => Color::Yellow,
        VilIssueKind::Plumbing => Color::Cyan,
        VilIssueKind::IrDrift => Color::LightRed,
        VilIssueKind::CanonicalTerm => Color::Green,
        VilIssueKind::Other => Color::DarkGray,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{AppState, AppStateOptions};

    fn fresh_state(issues: Vec<String>) -> AppState {
        let mut s = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap_or_default(),
        });
        s.vil_status.validation_issues = issues;
        s
    }

    #[test]
    fn classify_keyword_matrix() {
        assert_eq!(
            VilIssueKind::classify("Handler 'a' param 'b' contains owned-bytes type 'Vec<u8>'"),
            VilIssueKind::ZeroCopy
        );
        assert_eq!(
            VilIssueKind::classify(
                "Struct 'X' manually implements 'VilMessage' (medium — trait impl only)."
            ),
            VilIssueKind::Plumbing
        );
        assert_eq!(
            VilIssueKind::classify(
                "Struct 'Foo' looks like a semantic message type but has no VIL role macro. Add #[vil_state]"
            ),
            VilIssueKind::Semantic
        );
        assert_eq!(
            VilIssueKind::classify("IR metadata drift detected on src/lib.rs"),
            VilIssueKind::IrDrift
        );
        assert_eq!(
            VilIssueKind::classify("Canonical term violation: use 'changeset' not 'diff-set'"),
            VilIssueKind::CanonicalTerm
        );
        assert_eq!(
            VilIssueKind::classify("Something completely unrelated."),
            VilIssueKind::Other
        );
    }

    #[test]
    fn grouping_counts_five_mock_issues() {
        let state = fresh_state(vec![
            "Handler 'a' zero-copy violation on Network boundary".into(),
            "Struct 'B' manually implements 'VilMessage' — remove plumbing".into(),
            "Struct 'C' has no VIL role macro — add #[vil_state]".into(),
            "IR drift detected between HEAD and working tree".into(),
            "Some other advisory from a future pass.".into(),
        ]);

        let issues = classify_issues(&state);
        assert_eq!(issues.len(), 5);
        let counts = group_counts(&issues);
        assert_eq!(counts.get(&VilIssueKind::ZeroCopy).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&VilIssueKind::Plumbing).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&VilIssueKind::Semantic).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&VilIssueKind::IrDrift).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&VilIssueKind::Other).copied().unwrap_or(0), 1);
    }

    #[test]
    fn filtered_respects_group_filter() {
        let mut state = fresh_state(vec![
            "Handler 'a' zero-copy violation".into(),
            "Struct 'B' manually implements 'VilMessage'".into(),
            "Handler 'c' owned-bytes type 'Vec<u8>'".into(),
        ]);
        let all = classify_issues(&state);
        assert_eq!(filtered(&state, &all).len(), 3);

        state.vil_workbench_group_filter = Some(VilIssueKind::ZeroCopy);
        let view = filtered(&state, &all);
        assert_eq!(view.len(), 2);
        assert!(view.iter().all(|i| i.kind == VilIssueKind::ZeroCopy));
    }

    #[test]
    fn extract_file_hint_grabs_quoted_identifier() {
        let issue = "Handler 'create_user' is on Network boundary but is not zero-copy eligible.";
        let c = ClassifiedIssue::from_raw(issue.to_string());
        assert_eq!(c.file.as_deref(), Some("create_user"));
        assert_eq!(c.kind, VilIssueKind::ZeroCopy);
    }

    #[test]
    fn short_collapses_whitespace_and_truncates() {
        let long = "a ".repeat(200);
        let short = shorten(&long);
        assert!(short.len() <= 160);
    }
}
