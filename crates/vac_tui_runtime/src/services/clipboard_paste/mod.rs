pub mod path_extraction;
pub mod image_handling;

// Re-export public API
pub use path_extraction::{
    extract_file_paths_from_text, normalize_pasted_path, find_image_file_by_name,
};
pub use image_handling::{
    paste_image_as_png, paste_image_to_temp_png, PasteImageError, EncodedImageFormat,
    PastedImageInfo,
};

/// A single pasted attachment (long text or image) tracked in the input ledger.
/// The placeholder token is rendered in the input TextArea so the user sees a
/// compact representation; the full content is expanded at submit time.
#[derive(Debug, Clone)]
pub struct PastedItem {
    pub id: String,
    pub placeholder: String,
    pub kind: PastedKind,
}

#[derive(Debug, Clone)]
pub enum PastedKind {
    /// Long text content to be inlined at submit time.
    Text {
        content: String,
        line_count: usize,
        char_count: usize,
    },
    /// Image attachment. The actual ContentPart rides in `pending_image_parts`;
    /// this variant exists so the tray can show the image alongside text pastes
    /// and an index for removal.
    Image {
        width: u32,
        height: u32,
        byte_count: usize,
    },
}

/// Threshold above which a pasted text becomes a placeholder instead of
/// being inlined directly. Keeps the input readable while preserving the
/// full content for submission.
pub const LONG_PASTE_LINE_THRESHOLD: usize = 8;
pub const LONG_PASTE_CHAR_THRESHOLD: usize = 800;

pub fn is_long_paste(text: &str) -> bool {
    text.chars().count() >= LONG_PASTE_CHAR_THRESHOLD
        || text.chars().filter(|c| *c == '\n').count() + 1 >= LONG_PASTE_LINE_THRESHOLD
}

pub fn make_paste_id(counter: usize) -> String {
    format!("p{}", counter)
}

pub fn text_placeholder(id: &str, char_count: usize, line_count: usize) -> String {
    format!(
        "«pasted:{} +{} chars, {} lines»",
        id, char_count, line_count
    )
}

pub fn image_placeholder(id: &str, width: u32, height: u32) -> String {
    format!("«image:{} {}x{}»", id, width, height)
}

/// Copy text to the system clipboard.
#[cfg(not(target_os = "android"))]
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {}", e))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Failed to copy to clipboard: {}", e))
}

#[cfg(target_os = "android")]
pub fn copy_to_clipboard(_text: &str) -> Result<(), String> {
    Err("Clipboard is unsupported on Android".to_string())
}

// ===== Unit 5 (Wave 3.1) — Attachment tray preview & reorder helpers =====
//
// These helpers power the paste tray UI: per-paste kind badge, size, token
// estimate, preview, and the cursor/reorder/removal state transitions.
// Kept as pure functions over `PastedItem` / `Vec<PastedItem>` so they are
// trivially unit-testable without building a full `AppState`.

/// Fixed tile cost used to estimate image token budget (Anthropic default).
pub const IMAGE_TOKEN_ESTIMATE: usize = 85;

/// Heuristic: 4 characters ≈ 1 token. Used when the Unit-3 tokenizer is not
/// merged yet so this unit is independently shippable.
pub fn estimate_text_tokens(char_count: usize) -> usize {
    char_count.div_ceil(4)
}

/// Kind badge rendered in the tray card (`[TEXT]` / `[FILE]` / `[IMG]`).
pub fn kind_badge(kind: &PastedKind) -> &'static str {
    match kind {
        PastedKind::Text { .. } => "[TEXT]",
        PastedKind::Image { .. } => "[IMG]",
    }
}

/// Human-friendly size for the kind. `chars/bytes` for text, `WxH, KB` for img.
pub fn size_label(kind: &PastedKind) -> String {
    match kind {
        PastedKind::Text { char_count, .. } => {
            format!("{}c", char_count)
        }
        PastedKind::Image {
            width,
            height,
            byte_count,
        } => format!("{}x{} {}KB", width, height, byte_count / 1024),
    }
}

/// Token estimate label for the tray (`~N tok`).
pub fn token_estimate(kind: &PastedKind) -> usize {
    match kind {
        PastedKind::Text { char_count, .. } => estimate_text_tokens(*char_count),
        PastedKind::Image { .. } => IMAGE_TOKEN_ESTIMATE,
    }
}

/// Short preview: first 80 chars for text; "image WxH" for image.
pub fn preview_text(kind: &PastedKind) -> String {
    match kind {
        PastedKind::Text { content, .. } => {
            let one_line: String = content
                .chars()
                .map(|c| if c == '\n' { ' ' } else { c })
                .collect();
            let trimmed = one_line.trim();
            let preview: String = trimmed.chars().take(80).collect();
            if trimmed.chars().count() > 80 {
                format!("{}…", preview)
            } else {
                preview
            }
        }
        PastedKind::Image { width, height, .. } => format!("image {}x{}", width, height),
    }
}

/// Clamp a selected index into `pending_pastes`. Returns 0 when empty.
pub fn clamp_selected(selected: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        selected.min(len.saturating_sub(1))
    }
}

/// Move cursor to the next paste (wrapping).
pub fn select_next(selected: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (selected + 1) % len }
}

/// Move cursor to the previous paste (wrapping).
pub fn select_prev(selected: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else if selected == 0 {
        len - 1
    } else {
        selected - 1
    }
}

/// Remove the paste at `selected`. Returns the new clamped cursor.
pub fn remove_at(pastes: &mut Vec<PastedItem>, selected: usize) -> usize {
    if pastes.is_empty() || selected >= pastes.len() {
        return 0;
    }
    pastes.remove(selected);
    clamp_selected(selected, pastes.len())
}

/// Swap the paste at `selected` with its successor. Returns the new cursor
/// (tracking the moved paste). No-op if at the last index.
pub fn swap_with_next(pastes: &mut [PastedItem], selected: usize) -> usize {
    if pastes.is_empty() || selected + 1 >= pastes.len() {
        return selected.min(pastes.len().saturating_sub(1));
    }
    pastes.swap(selected, selected + 1);
    selected + 1
}

/// Swap the paste at `selected` with its predecessor. Returns the new cursor.
pub fn swap_with_prev(pastes: &mut [PastedItem], selected: usize) -> usize {
    if pastes.is_empty() || selected == 0 {
        return 0;
    }
    pastes.swap(selected, selected - 1);
    selected - 1
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn text_item(id: &str, content: &str) -> PastedItem {
        let chars = content.chars().count();
        let lines = content.chars().filter(|c| *c == '\n').count() + 1;
        PastedItem {
            id: id.to_string(),
            placeholder: format!("«{}»", id),
            kind: PastedKind::Text {
                content: content.to_string(),
                line_count: lines,
                char_count: chars,
            },
        }
    }

    fn image_item(id: &str, w: u32, h: u32) -> PastedItem {
        PastedItem {
            id: id.to_string(),
            placeholder: format!("«{}»", id),
            kind: PastedKind::Image {
                width: w,
                height: h,
                byte_count: 4096,
            },
        }
    }

    #[test]
    fn kind_badge_labels() {
        assert_eq!(kind_badge(&text_item("p1", "hi").kind), "[TEXT]");
        assert_eq!(kind_badge(&image_item("p2", 100, 50).kind), "[IMG]");
    }

    #[test]
    fn text_token_heuristic_rounds_up() {
        assert_eq!(estimate_text_tokens(0), 0);
        assert_eq!(estimate_text_tokens(1), 1);
        assert_eq!(estimate_text_tokens(3), 1);
        assert_eq!(estimate_text_tokens(4), 1);
        assert_eq!(estimate_text_tokens(5), 2);
        assert_eq!(estimate_text_tokens(400), 100);
    }

    #[test]
    fn image_token_estimate_is_fixed_tile() {
        assert_eq!(token_estimate(&image_item("p1", 100, 200).kind), 85);
    }

    #[test]
    fn preview_text_flattens_newlines_and_truncates() {
        let item = text_item("p1", "line one\nline two");
        assert_eq!(preview_text(&item.kind), "line one line two");

        let long: String = "x".repeat(200);
        let item = text_item("p1", &long);
        let preview = preview_text(&item.kind);
        // 80 chars + ellipsis
        assert_eq!(preview.chars().count(), 81);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn select_next_prev_wrap() {
        assert_eq!(select_next(0, 3), 1);
        assert_eq!(select_next(2, 3), 0); // wrap
        assert_eq!(select_prev(0, 3), 2); // wrap
        assert_eq!(select_prev(1, 3), 0);
        // empty slice
        assert_eq!(select_next(0, 0), 0);
        assert_eq!(select_prev(0, 0), 0);
    }

    #[test]
    fn remove_selected_shrinks_vec_and_clamps_cursor() {
        // Pre-populate with 3 items
        let mut pastes = vec![
            text_item("p1", "one"),
            text_item("p2", "two"),
            text_item("p3", "three"),
        ];
        // Remove middle
        let new_sel = remove_at(&mut pastes, 1);
        assert_eq!(pastes.len(), 2);
        assert_eq!(pastes[0].id, "p1");
        assert_eq!(pastes[1].id, "p3");
        assert_eq!(new_sel, 1); // still in range
        // Remove last → cursor clamps down to 0 (only 1 item left)
        let new_sel = remove_at(&mut pastes, 1);
        assert_eq!(pastes.len(), 1);
        assert_eq!(new_sel, 0);
        // Remove the last one → empty + cursor 0
        let new_sel = remove_at(&mut pastes, 0);
        assert!(pastes.is_empty());
        assert_eq!(new_sel, 0);
    }

    #[test]
    fn remove_at_out_of_range_is_noop() {
        let mut pastes = vec![text_item("p1", "one")];
        let new_sel = remove_at(&mut pastes, 5);
        assert_eq!(pastes.len(), 1);
        assert_eq!(new_sel, 0);
    }

    #[test]
    fn reorder_j_twice_matches_expected_order() {
        // Pre-populate with 3 items
        let mut pastes = vec![
            text_item("p1", "one"),
            text_item("p2", "two"),
            text_item("p3", "three"),
        ];
        // Simulate selected=0, press "J" twice (swap with next)
        let sel = swap_with_next(&mut pastes, 0); // [p2, p1, p3], cursor=1
        assert_eq!(
            pastes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["p2", "p1", "p3"]
        );
        assert_eq!(sel, 1);
        let sel = swap_with_next(&mut pastes, sel); // [p2, p3, p1], cursor=2
        assert_eq!(
            pastes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["p2", "p3", "p1"]
        );
        assert_eq!(sel, 2);
    }

    #[test]
    fn reorder_k_moves_up() {
        let mut pastes = vec![
            text_item("p1", "one"),
            text_item("p2", "two"),
            text_item("p3", "three"),
        ];
        let sel = swap_with_prev(&mut pastes, 2); // [p1, p3, p2], cursor=1
        assert_eq!(
            pastes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["p1", "p3", "p2"]
        );
        assert_eq!(sel, 1);
    }

    #[test]
    fn swap_noops_at_boundaries() {
        let mut pastes = vec![text_item("p1", "one"), text_item("p2", "two")];
        // Can't go up from 0
        let sel = swap_with_prev(&mut pastes, 0);
        assert_eq!(sel, 0);
        assert_eq!(pastes[0].id, "p1");
        // Can't go down from last
        let sel = swap_with_next(&mut pastes, 1);
        assert_eq!(sel, 1);
        assert_eq!(pastes[1].id, "p2");
    }

    #[test]
    fn size_label_text_vs_image() {
        let t = text_item("p1", "hello");
        assert_eq!(size_label(&t.kind), "5c");
        let i = image_item("p2", 200, 100);
        // 4096 / 1024 = 4
        assert_eq!(size_label(&i.kind), "200x100 4KB");
    }
}
