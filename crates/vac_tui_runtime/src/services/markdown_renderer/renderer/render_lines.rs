use ratatui::text::{Line, Span};

use super::super::super::MarkdownComponent;
use super::MarkdownRenderer;

impl MarkdownRenderer {
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
                    let broken = super::super::layout::break_long_word(word, width);
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
}
