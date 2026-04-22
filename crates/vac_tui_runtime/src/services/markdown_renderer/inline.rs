use super::MarkdownStyle;
use ratatui::text::Span;

/// Parse image syntax: ![alt](url)
pub fn parse_image(text: &str) -> Option<(String, String)> {
    if text.len() > 500 {
        // Limit to prevent DoS
        return None;
    }

    if let Some(start) = text.find("![")
        && let Some(middle) = text[start..].find("](")
        && start + middle < text.len()
    {
        let alt_part = &text[start + 2..start + middle];
        let url_start = start + middle + 2;
        if let Some(end) = text[url_start..].find(')')
            && url_start + end <= text.len()
        {
            let url_part = &text[url_start..url_start + end];
            return Some((alt_part.to_string(), url_part.to_string()));
        }
    }
    None
}

/// Parse link syntax: [text](url)
pub fn parse_link(text: &str) -> Option<(String, String)> {
    if text.len() > 500 {
        // Limit to prevent DoS
        return None;
    }

    if let Some(start) = text.find('[')
        && let Some(middle) = text[start..].find("](")
        && start + middle < text.len()
    {
        let text_part = &text[start + 1..start + middle];
        let url_start = start + middle + 2;
        if let Some(end) = text[url_start..].find(')')
            && url_start + end <= text.len()
        {
            let url_part = &text[url_start..url_start + end];
            return Some((text_part.to_string(), url_part.to_string()));
        }
    }
    None
}

/// Parse inline formatting (bold, inline code) with proper styling
pub fn parse_inline_formatting(line: &str, style: &MarkdownStyle) -> Vec<Span<'static>> {
    if line.len() > 2000 {
        return vec![Span::styled(line.to_string(), style.text_style)];
    }

    let mut spans = Vec::new();
    let mut remaining = line.to_string();

    // Handle bold first (greedy matching)
    while let Some(start) = remaining.find("**") {
        // Add text before bold
        if start > 0 {
            spans.push(Span::styled(
                remaining[..start].to_string(),
                style.text_style,
            ));
        }

        // Find closing **
        if let Some(end) = remaining[start + 2..].find("**") {
            let bold_text = &remaining[start + 2..start + 2 + end];
            spans.push(Span::styled(bold_text.to_string(), style.bold_style));
            remaining = remaining[start + 2 + end + 2..].to_string();
        } else {
            // No closing **, treat as regular text
            spans.push(Span::styled(remaining.clone(), style.text_style));
            break;
        }
    }

    // Handle inline code (backticks)
    let mut remaining_for_code = remaining.clone();
    while let Some(start) = remaining_for_code.find('`') {
        // Add text before code
        if start > 0 {
            spans.push(Span::styled(
                remaining_for_code[..start].to_string(),
                style.text_style,
            ));
        }

        // Find closing backtick
        if let Some(end) = remaining_for_code[start + 1..].find('`') {
            let code_text = &remaining_for_code[start + 1..start + 1 + end];
            spans.push(Span::styled(code_text.to_string(), style.code_style));
            remaining_for_code = remaining_for_code[start + 1 + end + 1..].to_string();
        } else {
            // No closing backtick, treat as regular text
            spans.push(Span::styled(remaining_for_code.clone(), style.text_style));
            break;
        }
    }

    // Add any remaining text
    if !remaining_for_code.is_empty() {
        spans.push(Span::styled(remaining_for_code.clone(), style.text_style));
    }

    spans
}
