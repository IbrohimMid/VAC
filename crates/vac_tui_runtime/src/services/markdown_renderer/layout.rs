/// Get display width accounting for Unicode characters and emojis
pub fn display_width(text: &str) -> usize {
    text.chars().map(|c| char_display_width(c)).sum()
}

/// Get the actual display width of a single character using Unicode width properties
pub fn char_display_width(c: char) -> usize {
    use unicode_width::UnicodeWidthChar;
    
    if c.is_ascii() {
        return 1;
    }

    // Use Unicode East Asian Width property to determine width
    // This automatically handles most emojis and symbols correctly
    match unicode_width::UnicodeWidthChar::width(c) {
        Some(1) => 1, // Narrow characters (like ✓, ▲, etc.)
        Some(2) => 2, // Wide characters (like 🔴, 🟡, etc.)
        _ => 2,       // Default to wide for unknown characters
    }
}

/// Wrap text to fit within a specified width, breaking long words if necessary
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if display_width(text) <= width {
        return vec![text.to_string()];
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in words {
        let word_width = display_width(word);

        // If the word itself is longer than the available width, break it
        if word_width > width {
            // First, add any current line content
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
            }

            // Break the long word into chunks
            let word_chunks = break_long_word(word, width);
            for chunk in word_chunks {
                if current_line.is_empty() {
                    current_line = chunk;
                } else {
                    lines.push(current_line);
                    current_line = chunk;
                }
            }
        } else if current_line.is_empty() {
            current_line = word.to_string();
        } else if display_width(&current_line) + 1 + word_width <= width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Break a long word into chunks that fit within max_width
pub fn break_long_word(word: &str, max_width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut current_width = 0;

    for ch in word.chars() {
        let char_width = char_display_width(ch);

        if current_width + char_width > max_width && !current_chunk.is_empty() {
            chunks.push(current_chunk);
            current_chunk = String::new();
            current_width = 0;
        }

        current_chunk.push(ch);
        current_width += char_width;
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

/// Truncate text to fit within max_width, adding ellipsis if needed
pub fn truncate_text(text: &str, max_width: usize) -> String {
    if display_width(text) <= max_width {
        return text.to_string();
    }

    let mut result = String::new();
    let mut current_width = 0;

    for ch in text.chars() {
        let char_width = char_display_width(ch);
        if current_width + char_width > max_width {
            break;
        }
        result.push(ch);
        current_width += char_width;
    }

    // Add ellipsis if we truncated
    if result.len() < text.len() && current_width < max_width {
        result.push('…');
    }

    result
}

/// Strip markdown syntax (backticks, bold markers) from text for table cells
pub fn strip_markdown_for_table(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '`' {
            // Skip backtick, include content until next backtick
            while let Some(&next) = chars.peek() {
                if next == '`' {
                    chars.next(); // consume closing backtick
                    break;
                }
                result.push(chars.next().unwrap_or(' '));
            }
        } else if c == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consume second *
            // Include content until next **
            while let Some(&next) = chars.peek() {
                if next == '*' {
                    chars.next();
                    if chars.peek() == Some(&'*') {
                        chars.next(); // consume closing **
                        break;
                    }
                    result.push('*');
                } else {
                    result.push(chars.next().unwrap_or(' '));
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}
