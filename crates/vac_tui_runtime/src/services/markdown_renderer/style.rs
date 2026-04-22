use crate::services::theme::{StyleKey, Theme, ThemePreset};
use ratatui::style::{Modifier, Style};

#[derive(Clone)]
pub struct MarkdownStyle {
    pub h1_style: Style,
    pub h2_style: Style,
    pub h3_style: Style,
    pub h4_style: Style,
    pub h5_style: Style,
    pub h6_style: Style,
    pub bold_style: Style,
    pub italic_style: Style,
    pub bold_italic_style: Style,
    pub strikethrough_style: Style,
    pub code_style: Style,
    pub code_block_style: Style,
    pub link_style: Style,
    pub quote_style: Style,
    pub list_bullet_style: Style,
    pub task_open_style: Style,
    pub task_complete_style: Style,
    pub important_style: Style,
    pub note_style: Style,
    pub tip_style: Style,
    pub warning_style: Style,
    pub caution_style: Style,
    pub text_style: Style,
    pub separator_style: Style,
    pub table_header_style: Style,
    pub table_cell_style: Style,
}

impl Default for MarkdownStyle {
    fn default() -> Self {
        Self::adaptive()
    }
}

impl MarkdownStyle {
    /// Construct from a `Theme` reference, mapping each field to a `StyleKey`.
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            h1_style: theme.style(StyleKey::MarkdownH1),
            h2_style: theme.style(StyleKey::MarkdownH2),
            h3_style: theme.style(StyleKey::MarkdownH3),
            h4_style: theme.style(StyleKey::MarkdownH4),
            h5_style: theme.style(StyleKey::MarkdownH5),
            h6_style: theme.style(StyleKey::MarkdownH6),
            bold_style: theme.style(StyleKey::MarkdownBold),
            italic_style: theme.style(StyleKey::MarkdownItalic),
            bold_italic_style: theme
                .style(StyleKey::MarkdownBold)
                .add_modifier(Modifier::ITALIC),
            strikethrough_style: theme.style(StyleKey::MarkdownStrikethrough),
            code_style: theme
                .style(StyleKey::MarkdownCodeInline)
                .patch(theme.style(StyleKey::MarkdownCodeInlineBg)),
            code_block_style: theme
                .style(StyleKey::MarkdownCodeBlock)
                .patch(theme.style(StyleKey::MarkdownCodeBlockBg)),
            link_style: theme.style(StyleKey::MarkdownLink),
            quote_style: theme.style(StyleKey::MarkdownQuote),
            list_bullet_style: theme.style(StyleKey::MarkdownListBullet),
            task_open_style: theme.style(StyleKey::MarkdownTaskOpen),
            task_complete_style: theme.style(StyleKey::MarkdownTaskDone),
            important_style: theme.style(StyleKey::MarkdownImportant),
            note_style: theme.style(StyleKey::MarkdownNote),
            tip_style: theme.style(StyleKey::MarkdownTip),
            warning_style: theme.style(StyleKey::MarkdownCaution),
            caution_style: theme.style(StyleKey::MarkdownCaution),
            text_style: theme.style(StyleKey::Text),
            separator_style: theme.style(StyleKey::MarkdownSeparator),
            table_header_style: theme.style(StyleKey::MarkdownTableHeader),
            table_cell_style: theme.style(StyleKey::MarkdownTableCell),
        }
    }

    /// Create an adaptive style that works well on both dark and light backgrounds.
    pub fn adaptive() -> Self {
        let is_light = crate::services::detect_term::is_light_mode();
        let is_rgb_supported = crate::services::detect_term::should_use_rgb_colors();

        if is_light {
            Self::from_theme(&Theme::new(ThemePreset::Light))
        } else if is_rgb_supported {
            Self::from_theme(&Theme::new(ThemePreset::Dark))
        } else {
            Self::from_theme(&Theme::new(ThemePreset::HighContrast))
        }
    }

    /// Light theme optimized for light terminal backgrounds.
    pub fn light_theme() -> Self {
        Self::from_theme(&Theme::new(ThemePreset::Light))
    }

    /// Dark theme optimized for RGB-capable terminals.
    pub fn dark_theme() -> Self {
        Self::from_theme(&Theme::new(ThemePreset::Dark))
    }

    /// High contrast theme for dark terminals without RGB support.
    pub fn high_contrast_theme() -> Self {
        Self::from_theme(&Theme::new(ThemePreset::HighContrast))
    }
}
