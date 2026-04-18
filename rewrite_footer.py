import re

with open("crates/vac_cli/src/tui/view.rs", "r") as f:
    content = f.read()

start = content.find("fn render_footer(f: &mut Frame, state: &mut AppState, area: Rect) {")
end = content.find("fn render_command_palette", start)

# We want to replace the hardcoded match state.focus { ... } with ActionRegistry calls.
new_footer = """fn render_footer(f: &mut Frame, state: &mut AppState, area: Rect) {
    // Reject reason prompt takes priority
    if let Some(reason) = &state.reject_reason_input {
        let hints = vec![
            Span::styled(
                "REJECT REASON ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(reason.as_str()),
            Span::styled("█", Style::default().fg(Color::Yellow)),
            Span::styled(
                "  Enter: confirm  Esc: skip",
                Style::default().fg(Color::DarkGray),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    if state.at_trigger_active {
        let hints = vec![
            Span::styled(
                "@ FILE ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(&state.at_query, Style::default().fg(Color::White)),
            Span::styled(
                "  ↑↓: select  Enter: insert  Esc: cancel",
                Style::default().fg(Color::DarkGray),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    if state.shell_popup_visible && state.active_shell_command.is_some() {
        let hints = vec![
            Span::styled(
                "SHELL ",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  Ctrl+Z: background  Esc: close  Ctrl+C: kill",
                Style::default().fg(Color::DarkGray),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    let registry = crate::tui::action_registry::ActionRegistry::new();
    let ctx = crate::tui::action_registry::ActionContext::from_app_state(state);
    let actions = registry.get_actions_for_context(ctx);

    let mut hints = Vec::new();
    for action in actions {
        if !hints.is_empty() {
            hints.push(Span::raw("  "));
        }
        let key_str = action.keys.join("/");
        hints.push(Span::styled(key_str, Style::default().fg(Color::Cyan)));
        hints.push(Span::styled(
            format!(": {}  ", action.description.to_lowercase()),
            Style::default().fg(Color::DarkGray),
        ));
    }

    let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}
"""

content = content[:start] + new_footer + "\n" + content[end:]

with open("crates/vac_cli/src/tui/view.rs", "w") as f:
    f.write(content)
