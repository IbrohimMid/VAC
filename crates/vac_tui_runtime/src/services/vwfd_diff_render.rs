//! VWFD semantic diff renderer (PR-T13).
//!
//! Renders a [`vac_changeset::formats::vwfd::VwfdDiff`] into a vertical list
//! of ratatui lines grouped by section (added / removed / modified steps,
//! expression-level deltas). The caller owns the state and viewport; this
//! module is stateless and only exposes pure `build_*` helpers plus a
//! `render` convenience that draws into a `Rect` using a `Paragraph`.
//!
//! Styling flows entirely through the workspace `Theme::style(StyleKey::…)`
//! contract — no raw ratatui `Color::*` is used, so dark / light / Dracula
//! / Solarized presets all re-colour correctly.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use vac_changeset::formats::vwfd::{
    ExpressionDelta, ExpressionField, ModifiedStep, VwfdDiff,
};

use crate::services::theme::{StyleKey, Theme};

// ── public rendering entry points ────────────────────────────────────────

/// Render the full diff into `area`. No-op when `diff.is_empty()` beyond a
/// single muted "(no changes)" line.
pub fn render(f: &mut Frame, diff: &VwfdDiff, theme: &Theme, area: Rect) {
    let lines = build_lines(diff, theme);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.style(StyleKey::Muted))
        .title(Span::styled(
            format!(" VWFD diff  ({}) ", summary_badge(diff)),
            theme.style(StyleKey::Warning),
        ));
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

/// Build the flat list of styled lines for the given diff. Useful in tests
/// and when the caller wants to compose the diff into a larger paragraph.
pub fn build_lines<'a>(diff: &'a VwfdDiff, theme: &Theme) -> Vec<Line<'a>> {
    let mut out: Vec<Line<'a>> = Vec::new();

    if diff.is_empty() {
        out.push(Line::from(Span::styled(
            "(no changes)",
            theme.style(StyleKey::Muted),
        )));
        return out;
    }

    push_added(&mut out, diff, theme);
    push_removed(&mut out, diff, theme);
    push_modified(&mut out, diff, theme);
    push_expressions(&mut out, diff, theme);

    // Trim trailing blank line if any section appended one.
    if matches!(out.last(), Some(l) if l.spans.is_empty()) {
        out.pop();
    }

    out
}

/// One-line human-readable summary suitable for toast/banner use.
pub fn summary_badge(diff: &VwfdDiff) -> String {
    if diff.is_empty() {
        return "no changes".to_string();
    }
    format!(
        "+{} steps · -{} steps · ~{} steps · {} expr",
        diff.added_steps.len(),
        diff.removed_steps.len(),
        diff.modified_steps.len(),
        diff.modified_expressions.len(),
    )
}

// ── section builders ──────────────────────────────────────────────────────

fn push_added<'a>(out: &mut Vec<Line<'a>>, diff: &'a VwfdDiff, theme: &Theme) {
    if diff.added_steps.is_empty() {
        return;
    }
    push_section_header(out, "Added steps", diff.added_steps.len(), theme);
    for id in &diff.added_steps {
        out.push(Line::from(vec![
            Span::styled("  + ", theme.style(StyleKey::Success)),
            Span::styled(id.as_str(), theme.style(StyleKey::Success)),
        ]));
    }
    out.push(Line::from(""));
}

fn push_removed<'a>(out: &mut Vec<Line<'a>>, diff: &'a VwfdDiff, theme: &Theme) {
    if diff.removed_steps.is_empty() {
        return;
    }
    push_section_header(out, "Removed steps", diff.removed_steps.len(), theme);
    for id in &diff.removed_steps {
        out.push(Line::from(vec![
            Span::styled("  - ", theme.style(StyleKey::Error)),
            Span::styled(id.as_str(), theme.style(StyleKey::Error)),
        ]));
    }
    out.push(Line::from(""));
}

fn push_modified<'a>(out: &mut Vec<Line<'a>>, diff: &'a VwfdDiff, theme: &Theme) {
    if diff.modified_steps.is_empty() {
        return;
    }
    push_section_header(out, "Modified steps", diff.modified_steps.len(), theme);
    for step in &diff.modified_steps {
        push_modified_step(out, step, theme);
    }
    out.push(Line::from(""));
}

fn push_expressions<'a>(out: &mut Vec<Line<'a>>, diff: &'a VwfdDiff, theme: &Theme) {
    if diff.modified_expressions.is_empty() {
        return;
    }
    push_section_header(
        out,
        "Expression deltas",
        diff.modified_expressions.len(),
        theme,
    );
    for delta in &diff.modified_expressions {
        push_expression_delta(out, delta, theme);
    }
    out.push(Line::from(""));
}

// ── line primitives ───────────────────────────────────────────────────────

fn push_section_header<'a>(out: &mut Vec<Line<'a>>, label: &'a str, count: usize, theme: &Theme) {
    out.push(Line::from(vec![
        Span::styled(label, theme.style(StyleKey::Warning)),
        Span::styled(
            format!("  ({count})"),
            theme.style(StyleKey::Muted),
        ),
    ]));
}

fn push_modified_step<'a>(out: &mut Vec<Line<'a>>, step: &'a ModifiedStep, theme: &Theme) {
    out.push(Line::from(vec![
        Span::styled("  ~ ", theme.style(StyleKey::Accent)),
        Span::styled(step.id.as_str(), theme.style(StyleKey::Accent)),
    ]));

    if let Some((before, after)) = &step.handler_changed {
        out.push(indent_two_field_change(
            "handler",
            Some(before.as_str()),
            Some(after.as_str()),
            theme,
        ));
    }
    if let Some((before, after)) = &step.condition_changed {
        out.push(indent_two_field_change(
            "condition",
            before.as_deref(),
            after.as_deref(),
            theme,
        ));
    }
    if let Some((before, after)) = &step.on_error_changed {
        out.push(indent_two_field_change(
            "on_error",
            before.as_deref(),
            after.as_deref(),
            theme,
        ));
    }
    if step.inputs_changed {
        out.push(indent_flag("inputs", theme));
    }
    if step.outputs_changed {
        out.push(indent_flag("outputs", theme));
    }
}

fn push_expression_delta<'a>(
    out: &mut Vec<Line<'a>>,
    delta: &'a ExpressionDelta,
    theme: &Theme,
) {
    let field_label: &'static str = match delta.field {
        ExpressionField::Condition => "condition",
        ExpressionField::OnError => "on_error",
        ExpressionField::Handler => "handler",
        ExpressionField::Inputs => "inputs",
        ExpressionField::Outputs => "outputs",
    };

    out.push(Line::from(vec![
        Span::styled("  ~ ", theme.style(StyleKey::Accent)),
        Span::styled(delta.step_id.as_str(), theme.style(StyleKey::Accent)),
        Span::styled(" · ", theme.style(StyleKey::Muted)),
        Span::styled(field_label, theme.style(StyleKey::Muted)),
    ]));
    out.push(Line::from(vec![
        Span::styled("      before: ", theme.style(StyleKey::Muted)),
        Span::styled(
            delta.before.as_deref().unwrap_or("∅"),
            theme.style(StyleKey::Error),
        ),
    ]));
    out.push(Line::from(vec![
        Span::styled("      after:  ", theme.style(StyleKey::Muted)),
        Span::styled(
            delta.after.as_deref().unwrap_or("∅"),
            theme.style(StyleKey::Success),
        ),
    ]));
}

fn indent_two_field_change(
    field: &str,
    before: Option<&str>,
    after: Option<&str>,
    theme: &Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled("      ", theme.style(StyleKey::Muted)),
        Span::styled(
            field.to_string(),
            theme.style(StyleKey::Muted),
        ),
        Span::styled(": ", theme.style(StyleKey::Muted)),
        Span::styled(
            before.unwrap_or("∅").to_string(),
            theme.style(StyleKey::Error),
        ),
        Span::styled(" → ", theme.style(StyleKey::Muted)),
        Span::styled(
            after.unwrap_or("∅").to_string(),
            theme.style(StyleKey::Success),
        ),
    ])
}

fn indent_flag(field: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled("      ", theme.style(StyleKey::Muted)),
        Span::styled(
            field.to_string(),
            theme.style(StyleKey::Muted),
        ),
        Span::styled(": changed", theme.style(StyleKey::Warning)),
    ])
}

// ── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::default()
    }

    #[test]
    fn empty_diff_renders_no_changes_marker() {
        let diff = VwfdDiff::default();
        let lines = build_lines(&diff, &theme());
        assert_eq!(lines.len(), 1);
        // Single muted line with the literal marker.
        let text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(text, "(no changes)");
        assert_eq!(summary_badge(&diff), "no changes");
    }

    #[test]
    fn full_diff_sections_appear_in_order() {
        let diff = VwfdDiff {
            added_steps: vec!["wf1/s-new".to_string()],
            removed_steps: vec!["wf1/s-old".to_string()],
            modified_steps: vec![ModifiedStep {
                id: "wf1/s-keep".to_string(),
                handler_changed: Some(("h-old".into(), "h-new".into())),
                on_error_changed: None,
                condition_changed: Some((Some("always".into()), None)),
                inputs_changed: true,
                outputs_changed: false,
            }],
            modified_expressions: vec![ExpressionDelta {
                step_id: "wf1/s-keep".to_string(),
                field: ExpressionField::Condition,
                before: Some("always".to_string()),
                after: None,
            }],
        };

        let lines = build_lines(&diff, &theme());
        // Flatten to string for ordering assertions.
        let flat: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect();

        let joined = flat.join("\n");
        assert!(joined.contains("Added steps"));
        assert!(joined.contains("Removed steps"));
        assert!(joined.contains("Modified steps"));
        assert!(joined.contains("Expression deltas"));

        // Added must come before Removed.
        let added_idx = flat.iter().position(|l| l.contains("Added steps")).unwrap();
        let removed_idx = flat
            .iter()
            .position(|l| l.contains("Removed steps"))
            .unwrap();
        let modified_idx = flat
            .iter()
            .position(|l| l.contains("Modified steps"))
            .unwrap();
        let expr_idx = flat
            .iter()
            .position(|l| l.contains("Expression deltas"))
            .unwrap();
        assert!(added_idx < removed_idx);
        assert!(removed_idx < modified_idx);
        assert!(modified_idx < expr_idx);

        // Modified-step fields surface per-field indented lines.
        assert!(flat.iter().any(|l| l.contains("handler") && l.contains("h-old") && l.contains("h-new")));
        assert!(flat.iter().any(|l| l.contains("condition") && l.contains("always")));
        assert!(flat.iter().any(|l| l.contains("inputs") && l.contains("changed")));
        assert!(!flat.iter().any(|l| l.contains("outputs: changed")));
    }

    #[test]
    fn summary_badge_counts() {
        let diff = VwfdDiff {
            added_steps: vec!["a".into(), "b".into()],
            removed_steps: vec![],
            modified_steps: vec![ModifiedStep {
                id: "x".into(),
                handler_changed: None,
                on_error_changed: None,
                condition_changed: None,
                inputs_changed: false,
                outputs_changed: true,
            }],
            modified_expressions: vec![],
        };
        assert_eq!(
            summary_badge(&diff),
            "+2 steps · -0 steps · ~1 steps · 0 expr"
        );
    }
}
