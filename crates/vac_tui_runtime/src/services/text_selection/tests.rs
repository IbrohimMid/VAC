use super::*;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Helper: create a selection covering all given lines fully
fn full_selection(start_line: usize, end_line: usize, end_col: u16) -> SelectionState {
    SelectionState {
        active: true,
        start_line: Some(start_line),
        start_col: Some(0),
        end_line: Some(end_line),
        end_col: Some(end_col),
    }
}

// -- decorative_prefix_width tests --

#[test]
fn test_decorative_prefix_width_user_message() {
    // User message line: "┃ Hello world"
    let line = Line::from(vec![
        Span::styled("┃ ", Style::default()),
        Span::raw("Hello world"),
    ]);
    assert_eq!(decorative_prefix_width(&line), 2);
}

#[test]
fn test_decorative_prefix_width_quote() {
    // Quote line: "│ quoted text"
    let line = Line::from(vec![
        Span::styled("│ ", Style::default()),
        Span::raw("quoted text"),
    ]);
    assert_eq!(decorative_prefix_width(&line), 2);
}

#[test]
fn test_decorative_prefix_width_code_block_no_indent() {
    // Code block line with no indentation: "fn main() {"
    let line = Line::from(vec![Span::raw("fn main() {")]);
    assert_eq!(decorative_prefix_width(&line), 0);
}

#[test]
fn test_decorative_prefix_width_code_block_with_indent() {
    // Code block line with indentation: "    let x = 1;"
    let line = Line::from(vec![Span::raw("    let x = 1;")]);
    assert_eq!(decorative_prefix_width(&line), 0);
}

#[test]
fn test_decorative_prefix_width_empty_line() {
    let line = Line::from(vec![Span::raw("")]);
    assert_eq!(decorative_prefix_width(&line), 0);
}

#[test]
fn test_decorative_prefix_width_border_only() {
    // Line that is just a border char (e.g. part of a box top)
    let line = Line::from(vec![Span::raw("┃")]);
    assert_eq!(decorative_prefix_width(&line), 1);
}

// -- strip_decorative_prefix_residue tests --

#[test]
fn test_strip_residue_no_prefix() {
    // Code block: no decorative prefix, preserve indentation
    let line = Line::from(vec![Span::raw("    name: Stakpak Dev")]);
    let extracted = " name: Stakpak Dev"; // after border filter (no borders here, but simulating)
    let result = strip_decorative_prefix_residue(extracted, 0, &line);
    assert_eq!(result, " name: Stakpak Dev");
}

#[test]
fn test_strip_residue_user_message() {
    // User message: "┃ Hello" → after border removal → " Hello"
    let line = Line::from(vec![
        Span::styled("┃ ", Style::default()),
        Span::raw("Hello"),
    ]);
    let extracted = " Hello"; // border char removed, space remains
    let result = strip_decorative_prefix_residue(extracted, 2, &line);
    assert_eq!(result, "Hello");
}

#[test]
fn test_strip_residue_preserves_content_indent_after_prefix() {
    // User message with indented content: "┃   indented" → " indented" after border removal
    // prefix_width=2 (border+space), border_char_cols=1, residual=1
    // So strip 1 space → "  indented" (2 spaces of content indent remain)
    let line = Line::from(vec![
        Span::styled("┃ ", Style::default()),
        Span::raw("  indented"),
    ]);
    let extracted = "   indented"; // 3 spaces: 1 from prefix + 2 content
    let result = strip_decorative_prefix_residue(extracted, 2, &line);
    assert_eq!(result, "  indented");
}

// -- extract_selected_text_from_lines integration tests --

#[test]
fn test_extract_code_block_preserves_indentation() {
    // Simulate a YAML code block with indentation (no decorative prefix)
    let lines = vec![
        Line::from(vec![Span::raw("name: Stakpak Dev")]),
        Line::from(vec![Span::raw("  description: AI agent")]),
        Line::from(vec![Span::raw("  features:")]),
        Line::from(vec![Span::raw("    - infrastructure")]),
        Line::from(vec![Span::raw("    - kubernetes")]),
    ];
    let selection = full_selection(0, 4, 50);
    let result = extract_selected_text_from_lines(&selection, &lines);
    assert_eq!(
        result,
        "name: Stakpak Dev\n  description: AI agent\n  features:\n    - infrastructure\n    - kubernetes"
    );
}

#[test]
fn test_extract_user_message_strips_prefix() {
    // User message with "┃ " prefix
    let lines = vec![
        Line::from(vec![
            Span::styled("┃ ", Style::default()),
            Span::raw("Hello world"),
        ]),
        Line::from(vec![
            Span::styled("┃ ", Style::default()),
            Span::raw("How are you?"),
        ]),
    ];
    let selection = full_selection(0, 1, 50);
    let result = extract_selected_text_from_lines(&selection, &lines);
    assert_eq!(result, "Hello world\nHow are you?");
}

#[test]
fn test_extract_mixed_code_and_messages() {
    // Simulates a mix: user message line followed by code block lines
    let lines = vec![
        Line::from(vec![
            Span::styled("┃ ", Style::default()),
            Span::raw("Here is my config:"),
        ]),
        Line::from(vec![Span::raw("server:")]),
        Line::from(vec![Span::raw("  port: 8080")]),
        Line::from(vec![Span::raw("  host: localhost")]),
    ];
    let selection = full_selection(0, 3, 50);
    let result = extract_selected_text_from_lines(&selection, &lines);
    assert_eq!(
        result,
        "Here is my config:\nserver:\n  port: 8080\n  host: localhost"
    );
}

#[test]
fn test_extract_quote_strips_prefix() {
    // Quote line with "│ " prefix
    let lines = vec![Line::from(vec![
        Span::styled("│ ", Style::default()),
        Span::raw("This is a quote"),
    ])];
    let selection = full_selection(0, 0, 50);
    let result = extract_selected_text_from_lines(&selection, &lines);
    assert_eq!(result, "This is a quote");
}

#[test]
fn test_extract_preserves_empty_lines_in_code() {
    let lines = vec![
        Line::from(vec![Span::raw("fn main() {")]),
        Line::from(vec![Span::raw("")]),
        Line::from(vec![Span::raw("    println!(\"hello\");")]),
        Line::from(vec![Span::raw("}")]),
    ];
    let selection = full_selection(0, 3, 50);
    let result = extract_selected_text_from_lines(&selection, &lines);
    assert_eq!(result, "fn main() {\n\n    println!(\"hello\");\n}");
}

#[test]
fn test_extract_spacing_marker_treated_as_empty() {
    let lines = vec![
        Line::from(vec![Span::raw("some text")]),
        Line::from(vec![Span::raw("SPACING_MARKER")]),
        Line::from(vec![Span::raw("more text")]),
    ];
    let selection = full_selection(0, 2, 50);
    let result = extract_selected_text_from_lines(&selection, &lines);
    assert_eq!(result, "some text\n\nmore text");
}

#[test]
fn test_extract_python_indentation_preserved() {
    // Python code block — indentation is critical
    let lines = vec![
        Line::from(vec![Span::raw("def foo():")]),
        Line::from(vec![Span::raw("    if True:")]),
        Line::from(vec![Span::raw("        return 1")]),
        Line::from(vec![Span::raw("    return 0")]),
    ];
    let selection = full_selection(0, 3, 50);
    let result = extract_selected_text_from_lines(&selection, &lines);
    assert_eq!(
        result,
        "def foo():\n    if True:\n        return 1\n    return 0"
    );
}
