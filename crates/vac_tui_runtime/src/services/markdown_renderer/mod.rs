// Module organization:
// - style.rs: MarkdownStyle struct + theme implementations
// - renderer.rs: MarkdownRenderer struct + rendering logic
// - layout.rs: Text layout, wrapping, and display width calculations
// - inline.rs: Inline formatting parsing (bold, code, links, images)

pub mod style;
pub mod renderer;
pub mod layout;
pub mod inline;

// Re-export public API
pub use style::MarkdownStyle;
pub use renderer::MarkdownRenderer;

use ratatui::text::{Line, Span};
use regex::Regex;

// Simplified component enum with all the variants you mentioned
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MarkdownComponent {
    H1(String),
    H2(String),
    H3(String),
    H4(String),
    H5(String),
    H6(String),
    Bold(String),
    Italic(String),
    BoldItalic(String),
    Strikethrough(String),
    Code(String),
    Link {
        text: String,
        url: String,
    },
    Image {
        alt: String,
        url: String,
    },
    UnorderedList(Vec<MarkdownComponent>),
    OrderedList(Vec<MarkdownComponent>),
    ListItem(String),
    Paragraph(String),
    CodeBlock {
        language: Option<String>,
        content: String,
    },
    Quote(String),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Important(String),
    Note(String),
    Tip(String),
    Warning(String),
    Caution(String),
    TaskOpen(String),
    TaskComplete(String),
    HorizontalSeparator,
    PlainText(String),
    Word(String),
    EmptyLine,
    MixedContent(Vec<Span<'static>>),
}

// Simple public function for easy use with performance monitoring
pub fn render_markdown_to_lines(
    markdown_content: &str,
) -> Result<Vec<Line<'static>>, Box<dyn std::error::Error>> {
    let parsed_content = xml_tags_to_markdown_headers(markdown_content);

    let style = MarkdownStyle::adaptive(); // Use adaptive styling
    let renderer = MarkdownRenderer::new(style);
    let components = renderer.parse_markdown(parsed_content.as_str())?;
    let lines = renderer.render_to_lines(components);
    Ok(lines)
}

/// Render markdown with an explicit content width for proper table sizing.
/// Use this when the display area width is known (e.g., when side panel is open).
pub fn render_markdown_to_lines_with_width(
    markdown_content: &str,
    width: usize,
) -> Result<Vec<Line<'static>>, Box<dyn std::error::Error>> {
    let parsed_content = xml_tags_to_markdown_headers(markdown_content);

    let style = MarkdownStyle::adaptive();
    let renderer = MarkdownRenderer::with_width(style, width);
    let components = renderer.parse_markdown(parsed_content.as_str())?;
    let lines = renderer.render_to_lines(components);
    Ok(lines)
}

fn xml_tags_to_markdown_headers(input: &str) -> String {
    // Use match to handle regex compilation errors gracefully
    let tag_regex = match Regex::new(r"<([a-zA-Z_][a-zA-Z0-9_-]*)[^>]*>") {
        Ok(regex) => regex,
        Err(_) => return input.to_string(), // Return original input if regex fails
    };

    let closing_tag_regex = match Regex::new(r"</([a-zA-Z_][a-zA-Z0-9_-]*)>") {
        Ok(regex) => regex,
        Err(_) => return input.to_string(), // Return original input if regex fails
    };

    let mut result = input.to_string();

    // Replace opening tags with markdown headers (skip checkpoint tags)
    result = tag_regex
        .replace_all(&result, |caps: &regex::Captures| {
            let tag_name = &caps[1];

            // Skip checkpoint tags - leave them untouched
            if tag_name == "checkpoint_id" || tag_name == "img" {
                caps[0].to_string() // Return the original tag unchanged
            } else {
                let formatted_name = format_header_name(tag_name);
                if formatted_name == "Scratchpad" {
                    format!("## {}\n", formatted_name) // Makes it a level 3 markdown header
                } else {
                    format!("#### {}\n", formatted_name) // Makes it a level 3 markdown header
                }
            }
        })
        .to_string();

    // Remove closing tags (except checkpoint)
    result = closing_tag_regex
        .replace_all(&result, |caps: &regex::Captures| {
            let tag_name = &caps[1];
            // Skip checkpoint closing tags - leave them untouched
            if tag_name == "checkpoint_id" {
                caps[0].to_string() // Return the original closing tag unchanged
            } else {
                String::new() // Just remove other closing tags
            }
        })
        .to_string();

    result
}

fn format_header_name(name: &str) -> String {
    name.split('_') // Split on underscores
        .filter(|s| !s.is_empty()) // Remove empty strings
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.as_str().to_lowercase().chars())
                    .collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ") // Join with spaces instead of underscores
}

// Enhanced function with timeout protection
#[allow(dead_code)]
pub fn render_markdown_to_lines_safe(
    markdown_content: &str,
) -> Result<Vec<Line<'static>>, Box<dyn std::error::Error>> {
    // Quick validation
    if markdown_content.is_empty() {
        return Ok(vec![]);
    }

    if markdown_content.len() > 2_000_000 {
        return Err("Markdown content too large (max 2MB)".into());
    }

    // Use a thread with timeout for very large content
    if markdown_content.len() > 100_000 {
        return render_with_timeout(markdown_content);
    }

    render_markdown_to_lines(markdown_content)
}

fn render_with_timeout(
    markdown_content: &str,
) -> Result<Vec<Line<'static>>, Box<dyn std::error::Error>> {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use crate::services::theme::{StyleKey, Theme};

    let (tx, rx) = mpsc::channel();
    let content = markdown_content.to_string();

    thread::spawn(move || {
        let result = render_markdown_to_lines(&content);
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });

    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(result) => match result {
            Ok(lines) => Ok(lines),
            Err(e) => Err(e.into()),
        },
        Err(_) => {
            // Timeout - return a simple error message
            let theme = Theme::default();
            Ok(vec![
                Line::from(vec![Span::styled(
                    "⚠️ Markdown rendering timed out",
                    theme.style(StyleKey::Warning),
                )]),
                Line::from(vec![Span::styled(
                    "Content too complex to render safely",
                    theme.style(StyleKey::Muted),
                )]),
            ])
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_adaptive_style_creation() {
        // Test that adaptive style can be created without panicking
        let style = MarkdownStyle::adaptive();

        // Verify that the style has proper colors set
        assert!(style.text_style.fg.is_some());
        assert!(style.h1_style.fg.is_some());
        assert!(style.code_style.fg.is_some());
    }

    #[test]
    fn test_dark_theme_creation() {
        // Test that dark theme can be created
        let style = MarkdownStyle::dark_theme();

        // Verify RGB colors are used
        match style.h1_style.fg {
            Some(Color::Rgb(_, _, _)) => {
                // Expected for RGB theme
            }
            _ => panic!("Dark theme should use RGB colors"),
        }
    }

    #[test]
    fn test_high_contrast_theme_creation() {
        // Test that high contrast theme can be created
        let style = MarkdownStyle::high_contrast_theme();

        // Verify reset colors are used for better compatibility
        match style.text_style.fg {
            Some(Color::Reset) => {
                // Expected for high contrast theme
            }
            _ => panic!("High contrast theme should use reset colors"),
        }

        // Verify no backgrounds are used for code blocks
        assert!(
            style.code_style.bg.is_none(),
            "Code style should not have background"
        );
        assert!(
            style.code_block_style.bg.is_none(),
            "Code block style should not have background"
        );

        // Verify cyan color is used for code blocks
        match style.code_block_style.fg {
            Some(Color::Cyan) => {
                // Expected for high contrast theme
            }
            _ => panic!("Code block style should use cyan color"),
        }
    }

    #[test]
    fn test_markdown_rendering_with_adaptive_style() {
        // Test that markdown rendering works with adaptive styling
        let markdown = "# Test Header\n\nThis is **bold** text with `code`.";

        let result = render_markdown_to_lines(markdown);
        assert!(result.is_ok());

        let lines = result.unwrap();
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_display_width_with_emojis() {
        // Test that emoji width calculation works correctly using Unicode width properties
        let style = MarkdownStyle::adaptive();
        let renderer = MarkdownRenderer::new(style);

        // Test emoji width calculation - these should be determined by Unicode width properties
        assert_eq!(renderer.display_width("🔴"), 2); // Wide emoji
        assert_eq!(renderer.display_width("🟡"), 2); // Wide emoji
        assert_eq!(renderer.display_width("✓"), 1); // Narrow symbol
        assert_eq!(renderer.display_width("▲"), 1); // Narrow symbol
        assert_eq!(renderer.display_width("🔴 Critical"), 11); // 2 + 1 space + 8 chars
        assert_eq!(renderer.display_width("🟡 Medium"), 9); // 2 + 1 space + 6 chars
        assert_eq!(renderer.display_width("✓ Keep as-is"), 12); // 1 + 1 space + 10 chars
        assert_eq!(renderer.display_width("Hello"), 5);
        assert_eq!(renderer.display_width("Hello 🔴"), 8); // 5 + 1 space + 2
    }

    #[test]
    fn test_table_with_emojis() {
        // Test that tables with emojis render correctly
        let markdown = r#"| Category | Severity | Issue Count |
|----------|----------|-------------|
| Security | 🔴 Critical | 8 |
| Reliability | 🟡 Medium | 3 |"#;

        let result = render_markdown_to_lines(markdown);
        assert!(result.is_ok());

        let lines = result.unwrap();
        assert!(!lines.is_empty());

        // Should not panic and should produce some output
        assert!(!lines.is_empty());
    }
}
