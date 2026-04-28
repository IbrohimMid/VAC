use crate::history_cell::VacHistoryCell;
use crate::services::message::{render_assistant_message_with_width, render_user_message};
use crate::services::theme::{StyleKey, Theme};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

pub fn render_history_cell(cell: &VacHistoryCell, width: usize) -> Vec<Line<'static>> {
    let theme = Theme::default();
    match cell {
        VacHistoryCell::UserMessage { content } => render_user_message(content, width),
        VacHistoryCell::AssistantMarkdown { content } => {
            render_assistant_message_with_width(content, width)
        }
        VacHistoryCell::AssistantStream { chunk } => {
            vec![Line::from(vec![
                Span::styled(
                    "VAC streaming: ",
                    theme.style(StyleKey::Success).add_modifier(Modifier::BOLD),
                ),
                Span::raw(chunk.clone()),
            ])]
        }
        VacHistoryCell::ToolCall {
            name, arguments, ..
        } => {
            let arg_str = match arguments {
                serde_json::Value::Object(obj) => {
                    if obj.is_empty() {
                        "{}".to_string()
                    } else {
                        let mut values = obj
                            .iter()
                            .map(|(k, v)| format!("{}={}", k, v))
                            .collect::<Vec<_>>();
                        values.sort();
                        values.join(", ")
                    }
                }
                _ => arguments.to_string(),
            };

            // Truncate arg_str to fit width, rough estimation
            let max_arg_len = width.saturating_sub(name.len() + 15);
            let trunc_args = if arg_str.chars().count() > max_arg_len {
                format!(
                    "{}…",
                    arg_str
                        .chars()
                        .take(max_arg_len.saturating_sub(1))
                        .collect::<String>()
                )
            } else {
                arg_str
            };

            vec![Line::from(vec![
                Span::raw("  "),
                Span::styled("○ ", theme.style(StyleKey::Muted)),
                Span::styled(format!("{:<14}", name), theme.style(StyleKey::Accent)),
                Span::raw(" "),
                Span::styled(trunc_args, theme.style(StyleKey::Muted)),
            ])]
        }
        VacHistoryCell::ToolResult {
            name,
            success,
            content,
            ..
        } => {
            let (dot, dot_key) = if *success {
                ("●", StyleKey::Success)
            } else {
                ("●", StyleKey::Error)
            };

            let n_lines = content.lines().count();
            let summary = format!("{} line{}", n_lines, if n_lines == 1 { "" } else { "s" });

            let mut lines = vec![Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} ", dot), theme.style(dot_key)),
                Span::styled(format!("{:<14}", name), theme.style(StyleKey::Accent)),
                Span::raw(" "),
                Span::styled(summary, theme.style(StyleKey::Muted)),
                Span::raw("  "),
                Span::styled("done", theme.style(dot_key)),
            ])];

            if !*success {
                let first = content.lines().next().unwrap_or("").trim();
                if !first.is_empty() {
                    let trunc_first = if first.chars().count() > 100 {
                        format!("{}…", first.chars().take(99).collect::<String>())
                    } else {
                        first.to_string()
                    };
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(trunc_first, theme.style(StyleKey::Error)),
                    ]));
                }
            }
            lines
        }
        VacHistoryCell::ApprovalPrompt {
            tool_name,
            explanation,
            ..
        } => {
            let expl = explanation.as_deref().unwrap_or("requires approval");
            vec![Line::from(vec![
                Span::styled("⚠️ ", theme.style(StyleKey::Warning)),
                Span::styled(
                    format!("Approval needed for {}: {}", tool_name, expl),
                    theme.style(StyleKey::Warning),
                ),
            ])]
        }
        VacHistoryCell::ProposedPlan { plan } => {
            vec![Line::from(vec![
                Span::styled("📋 Plan: ", theme.style(StyleKey::Accent)),
                Span::raw(plan.clone()),
            ])]
        }
        VacHistoryCell::TodoList { items } => {
            let mut lines = vec![Line::from(Span::styled(
                "☑ Tasks:",
                theme.style(StyleKey::Accent),
            ))];
            for item in items {
                lines.push(Line::from(format!("  - {}", item)));
            }
            lines
        }
        VacHistoryCell::Error { message } => {
            vec![Line::from(vec![
                Span::styled("ERROR: ", theme.style(StyleKey::Error)),
                Span::styled(message.clone(), theme.style(StyleKey::Error)),
            ])]
        }
        VacHistoryCell::Info { message } => {
            vec![Line::from(vec![
                Span::styled("ℹ ", theme.style(StyleKey::Muted)),
                Span::styled(message.clone(), theme.style(StyleKey::Muted)),
            ])]
        }
        VacHistoryCell::VilDiagnostic { diagnostic } => {
            vec![Line::from(vec![
                Span::styled("VIL Diagnostic: ", theme.style(StyleKey::Warning)),
                Span::styled(diagnostic.clone(), theme.style(StyleKey::Muted)),
            ])]
        }
    }
}
