use std::time::Instant;

use super::super::MarkdownComponent;
use super::MarkdownStyle;

pub struct MarkdownRenderer {
    pub style: MarkdownStyle,
    pub content_width: Option<usize>,
}

mod parse;
mod render_lines;

impl MarkdownRenderer {
    pub fn new(style: MarkdownStyle) -> Self {
        Self {
            style,
            content_width: None,
        }
    }

    pub fn with_width(style: MarkdownStyle, width: usize) -> Self {
        Self {
            style,
            content_width: Some(width),
        }
    }

    // Improved parser with performance optimizations and limits
    pub fn parse_markdown(
        &self,
        input: &str,
    ) -> Result<Vec<MarkdownComponent>, Box<dyn std::error::Error>> {
        let start = Instant::now();

        // Input validation and limits
        if input.is_empty() {
            return Ok(vec![]);
        }

        // Reduce max size for better performance
        if input.len() > 100_000 {
            return Err("Markdown content too large (max 100KB)".into());
        }

        // Pre-process problematic patterns
        let cleaned_input = self.preprocess_input(input);

        let mut components = Vec::new();
        let lines: Vec<&str> = cleaned_input.lines().collect();

        // Limit number of lines to prevent infinite processing
        let max_lines = 500; // Reduced from 2000 for better performance
        let process_lines = if lines.len() > max_lines {
            &lines[..max_lines]
        } else {
            &lines
        };

        let mut i = 0;
        while i < process_lines.len() {
            let original_line = process_lines[i];

            // Strip line numbers if present (e.g., "1: # Header" -> "# Header")
            let stripped_line = self.strip_line_number(original_line);
            let line = stripped_line.trim();

            // Skip empty lines but add them for spacing
            if line.is_empty() {
                components.push(MarkdownComponent::EmptyLine);
                i += 1;
                continue;
            }

            // Parse different markdown elements with early returns
            if let Some(component) = self.parse_line_optimized(line, process_lines, &mut i) {
                components.push(component);
            }

            i += 1;

            // Yield control every 25 lines to keep UI responsive
            if i % 25 == 0 {
                std::thread::yield_now();

                // Timeout protection - reduced from 5s to 2s
                if start.elapsed().as_secs() > 2 {
                    components.push(MarkdownComponent::Paragraph(
                        "... (content truncated due to timeout)".to_string(),
                    ));
                    break;
                }
            }
        }

        // Performance monitoring removed

        Ok(components)
    }
}
