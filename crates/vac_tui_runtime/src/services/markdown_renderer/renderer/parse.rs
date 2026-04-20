use crate::services::syntax_highlighter;
use crossterm;

use super::super::super::MarkdownComponent;
use super::super::inline;
use super::super::layout;
use super::super::MarkdownStyle;
use super::MarkdownRenderer;

impl MarkdownRenderer {
    // Pre-process input to handle problematic patterns
    pub(super) fn preprocess_input(&self, input: &str) -> String {
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
    pub(super) fn strip_line_number(&self, line: &str) -> String {
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

    pub(super) fn parse_line_optimized(
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

    pub(super) fn get_terminal_width(&self) -> Option<usize> {
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

    pub(super) fn wrap_text(&self, text: &str, width: usize) -> Vec<String> {
        layout::wrap_text(text, width)
    }

    pub(super) fn truncate_text(&self, text: &str, max_width: usize) -> String {
        layout::truncate_text(text, max_width)
    }

    pub(super) fn strip_markdown_for_table(&self, text: &str) -> String {
        layout::strip_markdown_for_table(text)
    }

    // Calculate display width for Unicode text with accurate emoji detection
    pub fn display_width(&self, text: &str) -> usize {
        layout::display_width(text)
    }

    pub(super) fn parse_table_safe(
        &self,
        all_lines: &[&str],
        index: &mut usize,
    ) -> Option<MarkdownComponent> {
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

    pub(super) fn try_syntax_highlighting(
        &self,
        content: &str,
        language: &Option<String>,
    ) -> Result<Vec<ratatui::text::Line<'static>>, Box<dyn std::error::Error>> {
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
