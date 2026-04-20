use crate::services::syntax_highlighter;
use crossterm;
use ratatui::text::{Line, Span};
use std::time::Instant;

use super::super::MarkdownComponent;
use super::MarkdownStyle;
use super::layout;
use super::inline;

pub struct MarkdownRenderer {
    pub style: MarkdownStyle,
    pub content_width: Option<usize>,
}

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

    // Pre-process input to handle problematic patterns
    fn preprocess_input(&self, input: &str) -> String {
        input
            // Fix complex badge patterns like [![text](img)](link)
            .replace("[![", "🔗 [")
            // Simplify image shields/badges
            .replace("](https://img.shields.io", "](shield")
            // Clean up excessive formatting
            .replace("****", "**")
            // Normalize line endings
            .replace("\r\n", "\n")
            .replace('\r', "\n")
    }

    // Strip line numbers like "1: ", "22: ", "78:", "80:", etc.
    fn strip_line_number(&self, line: &str) -> String {
        let trimmed = line.trim();

        // Look for pattern like "number:" at the beginning (with or without space after colon)
        if let Some(colon_pos) = trimmed.find(':')
            && colon_pos < 10
        {
            // Reasonable limit for line numbers
            let prefix = &trimmed[..colon_pos];
            if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
                // Remove the colon and any following whitespace
                let after_colon = &trimmed[colon_pos + 1..];
                return after_colon.trim_start().to_string();
            }
        }

        line.to_string()
    }

    fn parse_line_optimized(
        &self,
        line: &str,
        all_lines: &[&str],
        index: &mut usize,
    ) -> Option<MarkdownComponent> {
        // Early returns for performance
        if line.len() > 10000 {
            return Some(MarkdownComponent::Paragraph(
                "(Line too long, truncated)".to_string(),
            ));
        }

        // Code blocks - handle first to avoid conflicts
        if line.starts_with("```") {
            return self.parse_code_block_safe(all_lines, index);
        }

        // Headings - check BEFORE other patterns to avoid conflicts
        if line.starts_with('#') {
            return self.parse_heading_fast(line);
        }

        // Handle complex badges/links early
        if line.contains("🔗 [") && line.contains("](") {
            return self.parse_simplified_link(line);
        }

        // Images - handle ![alt](url) syntax
        if line.contains("![")
            && line.contains("](")
            && let Some((alt, url)) = self.parse_image_safe(line)
        {
            return Some(MarkdownComponent::Image { alt, url });
        }

        // Links - handle [text](url) syntax
        if line.contains('[')
            && line.contains("](")
            && line.contains(')')
            && let Some((text, url)) = self.parse_link_safe(line)
        {
            return Some(MarkdownComponent::Link { text, url });
        }

        // Tasks
        if line.starts_with("- [x]") || line.starts_with("- [X]") {
            return Some(MarkdownComponent::TaskComplete(
                line[5..].trim().to_string(),
            ));
        }
        if let Some(stripped) = line.strip_prefix("- [ ]") {
            return Some(MarkdownComponent::TaskOpen(stripped.trim().to_string()));
        }

        // Lists
        if line.starts_with("- ") || line.starts_with("* ") {
            let content = line[2..].trim().to_string();
            return self.parse_list_item_safe(&content);
        }

        // Numbered lists
        if let Some(captures) = self.parse_numbered_list(line) {
            return self.parse_list_item_safe(&captures);
        }

        // Quotes
        if line.starts_with("> ") {
            return self.parse_quote_safe(line, all_lines, index);
        }

        // Horizontal separator
        if line.starts_with("---") || line.starts_with("***") || line.starts_with("___") {
            return Some(MarkdownComponent::HorizontalSeparator);
        }

        // Callouts
        if let Some(callout) = self.parse_callout(line) {
            return Some(callout);
        }

        // Tables - check for table pattern (|...|...|)
        if line.contains('|')
            && line.starts_with('|')
            && let Some(table) = self.parse_table_safe(all_lines, index)
        {
            return Some(table);
        }

        // Check for inline formatting (optimized)
        if self.has_inline_formatting_fast(line) {
            return Some(self.parse_inline_formatting_safe(line));
        }

        // Default to paragraph
        Some(MarkdownComponent::Paragraph(line.to_string()))
    }

    fn parse_heading_fast(&self, line: &str) -> Option<MarkdownComponent> {
        let hash_count = line.chars().take_while(|&c| c == '#').count();
        if hash_count == 0 || hash_count > 6 {
            return None;
        }

        let content = if line.len() > hash_count {
            let after_hashes = &line[hash_count..];
            if let Some(stripped) = after_hashes.strip_prefix(' ') {
                stripped.trim()
            } else {
                after_hashes.trim()
            }
        } else {
            ""
        };

        match hash_count {
            1 => Some(MarkdownComponent::H1(content.to_string())),
            2 => Some(MarkdownComponent::H2(content.to_string())),
            3 => Some(MarkdownComponent::H3(content.to_string())),
            4 => Some(MarkdownComponent::H4(content.to_string())),
            5 => Some(MarkdownComponent::H5(content.to_string())),
            6 => Some(MarkdownComponent::H6(content.to_string())),
            _ => None,
        }
    }

    fn parse_simplified_link(&self, line: &str) -> Option<MarkdownComponent> {
        // Handle simplified links from badges
        if let Some(start) = line.find("🔗 [")
            && let Some(middle) = line[start..].find("](")
            && let Some(end) = line[start + middle + 2..].find(')')
        {
            let text_part = &line[start + 4..start + middle];
            let url_start = start + middle + 2;
            let url_part = &line[url_start..url_start + end];
            return Some(MarkdownComponent::Link {
                text: text_part.to_string(),
                url: url_part.to_string(),
            });
        }
        None
    }

    fn parse_code_block_safe(
        &self,
        all_lines: &[&str],
        index: &mut usize,
    ) -> Option<MarkdownComponent> {
        let stripped_start_line = self.strip_line_number(all_lines[*index]);
        let start_line = stripped_start_line.trim();
        let language = if start_line.len() > 3 {
            let lang = start_line[3..].trim();
            if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            }
        } else {
            None
        };

        let mut code_lines = Vec::new();
        let mut j = *index + 1;
        let max_code_lines = 500; // Limit code block size

        // Collect lines until closing ``` or limit reached
        while j < all_lines.len() && code_lines.len() < max_code_lines {
            let code_line = self.strip_line_number(all_lines[j]);
            if code_line.trim().starts_with("```") {
                break;
            }
            code_lines.push(code_line);
            j += 1;
        }

        *index = j; // Skip to closing ```

        let content = if code_lines.len() >= max_code_lines {
            let mut truncated = code_lines.join("\n");
            truncated.push_str("\n... (code block truncated)");
            truncated
        } else {
            code_lines.join("\n")
        };

        Some(MarkdownComponent::CodeBlock { language, content })
    }

    fn parse_list_item_safe(&self, text: &str) -> Option<MarkdownComponent> {
        if text.is_empty() {
            return Some(MarkdownComponent::ListItem("".to_string()));
        }

        // Check for nested formatting
        if text.contains("**") || text.contains('`') || text.contains('[') {
            // Parse as inline formatted content
            Some(self.parse_inline_formatting_safe(text))
        } else {
            Some(MarkdownComponent::ListItem(text.to_string()))
        }
    }

    fn parse_quote_safe(
        &self,
        line: &str,
        all_lines: &[&str],
        index: &mut usize,
    ) -> Option<MarkdownComponent> {
        let mut quote_lines = vec![line.strip_prefix("> ")?.to_string()];

        let mut j = *index + 1;
        let max_quote_lines = 50; // Limit for safety

        // Collect consecutive quote lines
        while j < all_lines.len() && quote_lines.len() < max_quote_lines {
            let stripped = self.strip_line_number(all_lines[j]);
            let next_line = stripped.trim();

            if let Some(quote_content) = next_line.strip_prefix("> ") {
                quote_lines.push(quote_content.to_string());
                j += 1;
            } else if next_line.is_empty() {
                // Empty lines can continue quotes
                quote_lines.push(String::new());
                j += 1;
            } else {
                // Non-quote line, stop collecting
                break;
            }
        }

        *index = j - 1; // Adjust index to skip collected lines

        Some(MarkdownComponent::Quote(quote_lines.join("\n")))
    }

    fn parse_callout(&self, line: &str) -> Option<MarkdownComponent> {
        let lower_line = line.to_lowercase();

        if lower_line.contains("[!important]") {
            return Some(MarkdownComponent::Important(
                line.replace("[!important]", "")
                    .replace("[!IMPORTANT]", "")
                    .trim()
                    .to_string(),
            ));
        }
        if lower_line.contains("[!note]") {
            return Some(MarkdownComponent::Note(
                line.replace("[!note]", "")
                    .replace("[!NOTE]", "")
                    .trim()
                    .to_string(),
            ));
        }
        if lower_line.contains("[!tip]") {
            return Some(MarkdownComponent::Tip(
                line.replace("[!tip]", "")
                    .replace("[!TIP]", "")
                    .trim()
                    .to_string(),
            ));
        }
        if lower_line.contains("[!warning]") {
            return Some(MarkdownComponent::Warning(
                line.replace("[!warning]", "")
                    .replace("[!WARNING]", "")
                    .trim()
                    .to_string(),
            ));
        }
        if lower_line.contains("[!caution]") {
            return Some(MarkdownComponent::Caution(
                line.replace("[!caution]", "")
                    .replace("[!CAUTION]", "")
                    .trim()
                    .to_string(),
            ));
        }
        None
    }

    fn parse_numbered_list(&self, line: &str) -> Option<String> {
        // Match patterns like "1. ", "2. ", etc.
        let trimmed = line.trim();
        if let Some(dot_pos) = trimmed.find(". ")
            && dot_pos < 5
        {
            // Reasonable limit for list numbers
            let prefix = &trimmed[..dot_pos];
            if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
                return Some(trimmed[dot_pos + 2..].to_string());
            }
        }
        None
    }

    // Optimized inline formatting detection
    fn has_inline_formatting_fast(&self, line: &str) -> bool {
        if line.len() < 3 || line.len() > 5000 {
            return false;
        }

        // Quick pattern matching without complex loops
        let has_bold = line.contains("**") && line.matches("**").count() >= 2;
        let has_italic =
            line.contains('*') && !line.contains("**") && line.matches('*').count() >= 2;
        let has_code =
            line.contains('`') && line.matches('`').count() >= 2 && line.matches('`').count() <= 10;
        let has_strikethrough = line.contains("~~") && line.matches("~~").count() >= 2;
        let has_links = line.contains('[') && line.contains("](");

        has_bold || has_italic || has_code || has_strikethrough || has_links
    }

    // Simplified check for basic formatting
    fn has_simple_formatting(&self, line: &str) -> bool {
        line.len() < 1000 && (line.contains("**") || line.contains('`') || line.contains('['))
    }

    fn parse_inline_formatting_safe(&self, line: &str) -> MarkdownComponent {
        let spans = inline::parse_inline_formatting(line, &self.style);
        
        match spans.len().cmp(&1) {
            std::cmp::Ordering::Greater => MarkdownComponent::MixedContent(spans),
            std::cmp::Ordering::Equal => MarkdownComponent::Paragraph(spans[0].content.to_string()),
            std::cmp::Ordering::Less => MarkdownComponent::Paragraph(line.to_string()),
        }
    }

    fn parse_image_safe(&self, text: &str) -> Option<(String, String)> {
        inline::parse_image(text)
    }

    fn parse_link_safe(&self, text: &str) -> Option<(String, String)> {
        inline::parse_link(text)
    }

    fn get_terminal_width(&self) -> Option<usize> {
        // Use the explicitly set content width if available
        if let Some(width) = self.content_width {
            return Some(width.saturating_sub(6));
        }
        // Fallback to terminal size
        if let Ok((width, _)) = crossterm::terminal::size() {
            Some(width.saturating_sub(6) as usize)
        } else {
            None
        }
    }

    fn wrap_text(&self, text: &str, width: usize) -> Vec<String> {
        layout::wrap_text(text, width)
    }

    fn truncate_text(&self, text: &str, max_width: usize) -> String {
        layout::truncate_text(text, max_width)
    }

    fn strip_markdown_for_table(&self, text: &str) -> String {
        layout::strip_markdown_for_table(text)
    }

    // Calculate display width for Unicode text with accurate emoji detection
    pub fn display_width(&self, text: &str) -> usize {
        layout::display_width(text)
    }

    fn parse_table_safe(&self, all_lines: &[&str], index: &mut usize) -> Option<MarkdownComponent> {
        let stripped_start_line = self.strip_line_number(all_lines[*index]);
        let start_line = stripped_start_line.trim();

        // Check if this is a valid table header line (starts and ends with |)
        if !start_line.starts_with('|') || !start_line.ends_with('|') {
            return None;
        }

        // Parse headers from the first line, stripping markdown syntax
        let headers: Vec<String> = start_line
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(|s| self.strip_markdown_for_table(s.trim()))
            .collect();

        if headers.is_empty() {
            return None;
        }

        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut j = *index + 1;
        let max_table_rows = 100; // Limit table size

        // Check if the next line is a separator line (|----|----| etc.)
        if j < all_lines.len() {
            let stripped_separator_line = self.strip_line_number(all_lines[j]);
            let separator_line = stripped_separator_line.trim();

            // Validate separator line
            if separator_line.starts_with('|')
                && separator_line.ends_with('|')
                && separator_line.contains('-')
            {
                // Skip the separator line
                j += 1;
            }
        }

        // Parse table rows (even if there's no separator - for streaming compatibility)
        // Handle wrapped/broken rows: if a line starts with | but doesn't end with |,
        // concatenate subsequent lines until we find one ending with |
        while j < all_lines.len() && rows.len() < max_table_rows {
            let stripped_row_line = self.strip_line_number(all_lines[j]);
            let mut row_line = stripped_row_line.trim().to_string();

            // Check if this looks like a table row start
            if !row_line.starts_with('|') {
                break;
            }

            // If line starts with | but doesn't end with |, it might be a wrapped row
            // Try to reassemble it by concatenating subsequent lines
            let mut lookahead = j + 1;
            while !row_line.ends_with('|') && lookahead < all_lines.len() {
                let next_stripped = self.strip_line_number(all_lines[lookahead]);
                let next_line = next_stripped.trim();

                // If next line starts with |, this is a new row, not a continuation
                if next_line.starts_with('|') {
                    break;
                }

                // Append continuation line (preserving space)
                row_line.push_str(next_line);
                lookahead += 1;

                // Safety limit to prevent infinite loops
                if lookahead - j > 10 {
                    break;
                }
            }

            // Update j to skip any continuation lines we consumed
            j = lookahead - 1;

            // Now check if we have a valid complete row
            if !row_line.ends_with('|') {
                // Still not a valid row after reassembly, stop parsing
                break;
            }

            // Parse the row cells, stripping markdown syntax
            let cells: Vec<String> = row_line
                .trim_start_matches('|')
                .trim_end_matches('|')
                .split('|')
                .map(|s| self.strip_markdown_for_table(s.trim()))
                .collect();

            if !cells.is_empty() {
                rows.push(cells);
            }

            j += 1;
        }

        *index = j - 1; // Adjust index to skip processed lines

        // Return table even if incomplete (for streaming)
        Some(MarkdownComponent::Table { headers, rows })
    }

    // Convert components to styled lines for ratatui with limits
    pub fn render_to_lines(&self, components: Vec<MarkdownComponent>) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(components.len() * 2);
        let max_lines = 5000; // Prevent memory issues

        for (i, component) in components.into_iter().enumerate() {
            if lines.len() >= max_lines {
                lines.push(Line::from(vec![Span::styled(
                    "... (content truncated for performance)",
                    self.style.text_style,
                )]));
                break;
            }

            lines.extend(self.component_to_lines(component));

            // Yield control periodically
            if i % 100 == 0 {
                std::thread::yield_now();
            }
        }

        lines
    }

    fn component_to_lines(&self, component: MarkdownComponent) -> Vec<Line<'static>> {
        match component {
            MarkdownComponent::H1(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.h1_style)])]
            }
            MarkdownComponent::H2(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.h2_style)])]
            }
            MarkdownComponent::H3(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.h3_style)])]
            }
            MarkdownComponent::H4(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.h4_style)])]
            }
            MarkdownComponent::H5(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.h5_style)])]
            }
            MarkdownComponent::H6(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.h6_style)])]
            }

            MarkdownComponent::Bold(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.bold_style)])]
            }
            MarkdownComponent::Italic(text) => vec![Line::from(vec![Span::styled(
                text,
                self.style.italic_style,
            )])],
            MarkdownComponent::BoldItalic(text) => vec![Line::from(vec![Span::styled(
                text,
                self.style.bold_italic_style,
            )])],
            MarkdownComponent::Strikethrough(text) => vec![Line::from(vec![Span::styled(
                text,
                self.style.strikethrough_style,
            )])],
            MarkdownComponent::Code(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.code_style)])]
            }

            MarkdownComponent::Link { text, url: _ } => vec![Line::from(vec![Span::styled(
                text.to_string(),
                self.style.link_style,
            )])],

            MarkdownComponent::Image { alt, url: _ } => vec![Line::from(vec![Span::styled(
                format!("🖼️ {}", alt),
                self.style.link_style,
            )])],

            MarkdownComponent::UnorderedList(items) => {
                let mut list_lines = Vec::new();
                for item in items.into_iter().take(100) {
                    // Limit list items
                    let item_lines = self.component_to_lines(item);
                    for (i, line) in item_lines.into_iter().enumerate() {
                        if i == 0 {
                            let mut spans = vec![Span::styled("• ", self.style.list_bullet_style)];
                            spans.extend(line.spans);
                            list_lines.push(Line::from(spans));
                        } else {
                            let mut spans = vec![Span::styled("  ", self.style.text_style)];
                            spans.extend(line.spans);
                            list_lines.push(Line::from(spans));
                        }
                    }
                }
                list_lines
            }

            MarkdownComponent::OrderedList(items) => {
                let mut list_lines = Vec::new();
                for (index, item) in items.into_iter().enumerate().take(100) {
                    // Limit list items
                    let item_lines = self.component_to_lines(item);
                    for (i, line) in item_lines.into_iter().enumerate() {
                        if i == 0 {
                            let mut spans = vec![Span::styled(
                                format!("{}. ", index + 1),
                                self.style.list_bullet_style,
                            )];
                            spans.extend(line.spans);
                            list_lines.push(Line::from(spans));
                        } else {
                            let mut spans = vec![Span::styled("   ", self.style.text_style)];
                            spans.extend(line.spans);
                            list_lines.push(Line::from(spans));
                        }
                    }
                }
                list_lines
            }

            MarkdownComponent::ListItem(text) => {
                vec![Line::from(vec![
                    Span::styled("• ", self.style.list_bullet_style),
                    Span::styled(text, self.style.text_style),
                ])]
            }

            MarkdownComponent::CodeBlock { language, content } => {
                // Special handling for markdown code blocks - render as markdown instead of code
                if let Some(lang) = &language
                    && (lang.to_lowercase() == "markdown" || lang.to_lowercase() == "md")
                {
                    // Parse the content as markdown and render it
                    match self.parse_markdown(&content) {
                        Ok(markdown_components) => {
                            let mut rendered_lines = Vec::new();
                            for component in markdown_components {
                                rendered_lines.extend(self.component_to_lines(component));
                            }
                            return rendered_lines;
                        }
                        Err(_) => {
                            // If parsing fails, fall back to treating as code
                        }
                    }
                }

                let mut code_lines = Vec::new();

                // Limit code block size for performance
                let lines: Vec<&str> = content.lines().take(200).collect();

                // Try to use syntax highlighting if available
                if let Ok(highlighted_lines) = self.try_syntax_highlighting(&content, &language) {
                    code_lines.extend(highlighted_lines);
                } else {
                    // Fallback to simple styling
                    for line in lines {
                        code_lines.push(Line::from(vec![Span::styled(
                            line.to_string(),
                            self.style.code_block_style, // Use only the style, no additional background
                        )]));
                    }
                }

                if content.lines().count() > 200 {
                    code_lines.push(Line::from(vec![Span::styled(
                        "... (code block truncated)",
                        self.style.code_block_style,
                    )]));
                }

                code_lines
            }

            MarkdownComponent::Quote(text) => {
                let mut quote_lines = Vec::new();
                for line in text.lines().take(50) {
                    // Limit quote lines
                    quote_lines.push(Line::from(vec![
                        Span::styled("│ ", self.style.quote_style),
                        Span::styled(line.to_string(), self.style.quote_style),
                    ]));
                }
                quote_lines
            }

            MarkdownComponent::Table { headers, rows } => {
                let mut table_lines = Vec::new();

                if headers.is_empty() {
                    return table_lines;
                }

                // Limit columns and rows for performance
                let max_columns = 10;
                let max_rows = 50;
                let limited_headers: Vec<String> =
                    headers.iter().take(max_columns).cloned().collect();
                let limited_rows: Vec<Vec<String>> = rows
                    .into_iter()
                    .take(max_rows)
                    .map(|row| row.iter().take(max_columns).cloned().collect())
                    .collect();

                // Calculate natural width for each column based on content
                let mut natural_column_widths: Vec<usize> = limited_headers
                    .iter()
                    .map(|h| self.display_width(h))
                    .collect();

                for row in &limited_rows {
                    for (col_idx, cell) in row.iter().enumerate() {
                        if col_idx < natural_column_widths.len() {
                            natural_column_widths[col_idx] =
                                natural_column_widths[col_idx].max(self.display_width(cell));
                        }
                    }
                }

                // Calculate total natural width
                // Each column has: width + 2 spaces padding + 1 separator (│)
                // Plus 1 for the starting │
                let natural_table_width: usize = natural_column_widths.iter().sum::<usize>()
                    + (natural_column_widths.len() * 3)
                    + 1;

                // Get terminal width, default to 80 if unavailable
                let terminal_width = self.get_terminal_width().unwrap_or(80);

                // If natural width fits in terminal, use natural widths
                // Otherwise, proportionally scale down all columns
                let column_widths: Vec<usize> = if natural_table_width <= terminal_width {
                    natural_column_widths
                } else {
                    // Calculate available width for content (excluding separators and padding)
                    let available_width =
                        terminal_width.saturating_sub(natural_column_widths.len() * 3 + 1);
                    let total_natural_width: usize = natural_column_widths.iter().sum();

                    if total_natural_width == 0 {
                        // Fallback if all columns are empty
                        vec![1; natural_column_widths.len()]
                    } else {
                        // Proportionally scale down each column
                        natural_column_widths
                            .iter()
                            .map(|&natural_width| {
                                let scaled_width =
                                    (natural_width * available_width) / total_natural_width;
                                scaled_width.max(1) // Ensure minimum width of 1
                            })
                            .collect()
                    }
                };

                // Table width is now guaranteed to fit within terminal width

                // Otherwise, render with beautiful box-drawing characters

                // Create top border
                let top_border = format!(
                    "┌{}┐",
                    column_widths
                        .iter()
                        .map(|w| "─".repeat(w + 2))
                        .collect::<Vec<_>>()
                        .join("┬")
                );
                table_lines.push(Line::from(vec![Span::styled(
                    top_border,
                    self.style.table_header_style,
                )]));

                // Render headers with proper alignment and wrapping
                let mut header_wrapped_cells: Vec<Vec<String>> = Vec::new();
                let mut header_max_lines = 1;

                for (col_idx, header) in limited_headers.iter().enumerate() {
                    if col_idx < column_widths.len() {
                        let width = column_widths[col_idx];
                        let wrapped = self.wrap_text(header, width);
                        header_max_lines = header_max_lines.max(wrapped.len());
                        header_wrapped_cells.push(wrapped);
                    }
                }

                // Fill in missing header cells
                while header_wrapped_cells.len() < column_widths.len() {
                    let width = column_widths[header_wrapped_cells.len()];
                    header_wrapped_cells.push(vec![" ".repeat(width)]);
                }

                // Render each line of the header
                for line_idx in 0..header_max_lines {
                    let mut padded_cells: Vec<String> = Vec::new();

                    for (col_idx, cell_lines) in header_wrapped_cells.iter().enumerate() {
                        let width = column_widths[col_idx];
                        let cell_content = if line_idx < cell_lines.len() {
                            cell_lines[line_idx].as_str()
                        } else {
                            ""
                        };

                        // Truncate content if it's still too long for the column
                        let truncated_content = if self.display_width(cell_content) > width {
                            self.truncate_text(cell_content, width)
                        } else {
                            cell_content.to_string()
                        };

                        // Pad with spaces to match column width
                        let display_width = self.display_width(&truncated_content);
                        let padding_needed = if width > display_width {
                            width.saturating_sub(display_width)
                        } else {
                            0
                        };

                        padded_cells.push(format!(
                            "{}{}",
                            truncated_content,
                            " ".repeat(padding_needed)
                        ));
                    }

                    let header_line = format!("│ {} │", padded_cells.join(" │ "));
                    table_lines.push(Line::from(vec![Span::styled(
                        header_line,
                        self.style.table_header_style,
                    )]));
                }

                // Header separator
                let separator = format!(
                    "├{}┤",
                    column_widths
                        .iter()
                        .map(|w| "─".repeat(w + 2))
                        .collect::<Vec<_>>()
                        .join("┼")
                );
                table_lines.push(Line::from(vec![Span::styled(
                    separator,
                    self.style.table_header_style,
                )]));

                // Render rows with proper alignment and text wrapping
                for row in limited_rows {
                    // Wrap each cell content to fit its column width independently
                    let mut wrapped_cells: Vec<Vec<String>> = Vec::new();
                    let mut max_lines = 1;

                    for (col_idx, cell) in row.iter().enumerate() {
                        if col_idx < column_widths.len() {
                            let width = column_widths[col_idx];
                            let wrapped = self.wrap_text(cell, width);
                            max_lines = max_lines.max(wrapped.len());
                            wrapped_cells.push(wrapped);
                        }
                    }

                    // Fill in missing cells with empty spaces
                    while wrapped_cells.len() < column_widths.len() {
                        let width = column_widths[wrapped_cells.len()];
                        wrapped_cells.push(vec![" ".repeat(width)]);
                    }

                    // Render each line of the row - each cell wraps independently
                    for line_idx in 0..max_lines {
                        let mut padded_cells: Vec<String> = Vec::new();

                        for (col_idx, cell_lines) in wrapped_cells.iter().enumerate() {
                            let width = column_widths[col_idx];
                            let cell_content = if line_idx < cell_lines.len() {
                                cell_lines[line_idx].as_str()
                            } else {
                                ""
                            };

                            // Truncate content if it's still too long for the column
                            let truncated_content = if self.display_width(cell_content) > width {
                                self.truncate_text(cell_content, width)
                            } else {
                                cell_content.to_string()
                            };

                            // Pad with spaces to match column width
                            let display_width = self.display_width(&truncated_content);
                            let padding_needed = if width > display_width {
                                width.saturating_sub(display_width)
                            } else {
                                0
                            };

                            padded_cells.push(format!(
                                "{}{}",
                                truncated_content,
                                " ".repeat(padding_needed)
                            ));
                        }

                        let row_line = format!("│ {} │", padded_cells.join(" │ "));
                        table_lines.push(Line::from(vec![Span::styled(
                            row_line,
                            self.style.table_cell_style,
                        )]));
                    }
                }

                // Create bottom border
                let bottom_border = format!(
                    "└{}┘",
                    column_widths
                        .iter()
                        .map(|w| "─".repeat(w + 2))
                        .collect::<Vec<_>>()
                        .join("┴")
                );
                table_lines.push(Line::from(vec![Span::styled(
                    bottom_border,
                    self.style.table_header_style,
                )]));

                table_lines
            }

            MarkdownComponent::TaskOpen(text) => vec![Line::from(vec![
                Span::styled("☐ ", self.style.task_open_style),
                Span::styled(text, self.style.text_style),
            ])],

            MarkdownComponent::TaskComplete(text) => vec![Line::from(vec![
                Span::styled("☑ ", self.style.task_complete_style),
                Span::styled(text, self.style.text_style),
            ])],

            MarkdownComponent::Important(text) => vec![
                Line::from(vec![Span::styled(
                    "🔒 IMPORTANT",
                    self.style.important_style,
                )]),
                Line::from(vec![Span::styled(text, self.style.text_style)]),
            ],

            MarkdownComponent::Note(text) => vec![
                Line::from(vec![Span::styled("📝 NOTE", self.style.note_style)]),
                Line::from(vec![Span::styled(text, self.style.text_style)]),
            ],

            MarkdownComponent::Tip(text) => vec![
                Line::from(vec![Span::styled("💡 TIP", self.style.tip_style)]),
                Line::from(vec![Span::styled(text, self.style.text_style)]),
            ],

            MarkdownComponent::Warning(text) => vec![
                Line::from(vec![Span::styled("⚠️  WARNING", self.style.warning_style)]),
                Line::from(vec![Span::styled(text, self.style.text_style)]),
            ],

            MarkdownComponent::Caution(text) => vec![
                Line::from(vec![Span::styled("⚡ CAUTION", self.style.caution_style)]),
                Line::from(vec![Span::styled(text, self.style.text_style)]),
            ],

            MarkdownComponent::HorizontalSeparator => vec![Line::from(vec![Span::styled(
                "─".repeat(50),
                self.style.separator_style,
            )])],

            MarkdownComponent::Paragraph(text) => {
                // Wrap text to content width if available
                if let Some(width) = self.content_width {
                    let wrapped = self.wrap_text(&text, width);
                    wrapped
                        .into_iter()
                        .map(|line| Line::from(vec![Span::styled(line, self.style.text_style)]))
                        .collect()
                } else {
                    vec![Line::from(vec![Span::styled(text, self.style.text_style)])]
                }
            }

            MarkdownComponent::PlainText(text) => {
                // Wrap text to content width if available
                if let Some(width) = self.content_width {
                    let wrapped = self.wrap_text(&text, width);
                    wrapped
                        .into_iter()
                        .map(|line| Line::from(vec![Span::styled(line, self.style.text_style)]))
                        .collect()
                } else {
                    vec![Line::from(vec![Span::styled(text, self.style.text_style)])]
                }
            }

            MarkdownComponent::Word(text) => {
                vec![Line::from(vec![Span::styled(text, self.style.text_style)])]
            }

            MarkdownComponent::EmptyLine => vec![Line::from("")],

            MarkdownComponent::MixedContent(spans) => {
                // Wrap mixed content (text with inline formatting like bold/code) to content width
                if let Some(width) = self.content_width {
                    self.wrap_mixed_content(spans, width)
                } else {
                    vec![Line::from(spans)]
                }
            }
        }
    }

    /// Wrap mixed content (spans with different styles) to fit within width
    fn wrap_mixed_content(&self, spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
        let mut result_lines: Vec<Line<'static>> = Vec::new();
        let mut current_line_spans: Vec<Span<'static>> = Vec::new();
        let mut current_line_width = 0usize;

        for span in spans {
            let span_text = span.content.to_string();
            let span_style = span.style;

            // Split span text into words
            let words: Vec<&str> = span_text.split_inclusive(char::is_whitespace).collect();

            for word in words {
                let word_width = self.display_width(word);

                if current_line_width + word_width > width && current_line_width > 0 {
                    // Start a new line
                    if !current_line_spans.is_empty() {
                        result_lines.push(Line::from(current_line_spans));
                        current_line_spans = Vec::new();
                        current_line_width = 0;
                    }
                }

                // Handle words longer than width by breaking them
                if word_width > width {
                    let broken = layout::break_long_word(word, width);
                    for (i, chunk) in broken.into_iter().enumerate() {
                        if i > 0 && !current_line_spans.is_empty() {
                            result_lines.push(Line::from(current_line_spans));
                            current_line_spans = Vec::new();
                            current_line_width = 0;
                        }
                        let chunk_width = self.display_width(&chunk);
                        current_line_spans.push(Span::styled(chunk, span_style));
                        current_line_width += chunk_width;
                    }
                } else {
                    current_line_spans.push(Span::styled(word.to_string(), span_style));
                    current_line_width += word_width;
                }
            }
        }

        // Don't forget the last line
        if !current_line_spans.is_empty() {
            result_lines.push(Line::from(current_line_spans));
        }

        if result_lines.is_empty() {
            result_lines.push(Line::from(""));
        }

        result_lines
    }

    // Try to apply syntax highlighting if the feature is available
    fn try_syntax_highlighting(
        &self,
        content: &str,
        language: &Option<String>,
    ) -> Result<Vec<Line<'static>>, Box<dyn std::error::Error>> {
        // This is a placeholder - you can implement syntax highlighting here
        // For now, we'll use a simple fallback

        // If you have the syntax_highlighter module, uncomment this:
        let extension = language
            .as_ref()
            .map(|lang| match lang.to_lowercase().as_str() {
                "rust" | "rs" => "rs",
                "javascript" | "js" => "js",
                "typescript" | "ts" => "ts",
                "python" | "py" => "py",
                "php" => "php",
                "bash" | "sh" | "shell" => "sh",
                "go" => "go",
                "java" => "java",
                "cpp" | "c++" => "cpp",
                "c" => "c",
                "html" => "html",
                "css" => "css",
                "sql" => "sql",
                "yaml" | "yml" => "yml",
                "toml" => "toml",
                "json" => "json",
                "markdown" | "md" => "md",
                _ => "txt",
            });

        Ok(syntax_highlighter::apply_syntax_highlighting(
            content, extension,
        ))
    }
}
