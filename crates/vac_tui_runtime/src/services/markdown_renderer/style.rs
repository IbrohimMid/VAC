use crate::services::detect_term::{AdaptiveColors, ThemeColors};
use ratatui::style::{Color, Modifier, Style};

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
    /// Create an adaptive style that works well on both dark and light backgrounds
    pub fn adaptive() -> Self {
        let is_light = crate::services::detect_term::is_light_mode();
        let is_rgb_supported = crate::services::detect_term::should_use_rgb_colors();

        if is_light {
            // Light theme with dark colors for good contrast
            Self::light_theme()
        } else if is_rgb_supported {
            // Use RGB colors for supported terminals (dark theme optimized)
            Self::dark_theme()
        } else {
            // Use high-contrast colors for unsupported terminals (works on both light and dark)
            Self::high_contrast_theme()
        }
    }

    /// Light theme optimized for light terminal backgrounds
    pub fn light_theme() -> Self {
        Self {
            h1_style: Style::default()
                .fg(Color::Indexed(25)) // Dark blue
                .add_modifier(Modifier::BOLD),
            h2_style: Style::default()
                .fg(Color::Indexed(30)) // Dark cyan/teal
                .add_modifier(Modifier::BOLD),
            h3_style: Style::default()
                .fg(Color::Indexed(28)) // Dark green
                .add_modifier(Modifier::BOLD),
            h4_style: Style::default()
                .fg(Color::Indexed(127)) // Dark magenta
                .add_modifier(Modifier::BOLD),
            h5_style: Style::default()
                .fg(Color::Indexed(130)) // Dark orange/brown
                .add_modifier(Modifier::BOLD),
            h6_style: Style::default()
                .fg(Color::Indexed(124)) // Dark red
                .add_modifier(Modifier::BOLD),
            bold_style: Style::default()
                .fg(Color::Indexed(232)) // Near-black for bold on light backgrounds
                .add_modifier(Modifier::BOLD),
            italic_style: Style::default()
                .fg(ThemeColors::text())
                .add_modifier(Modifier::ITALIC),
            bold_italic_style: Style::default()
                .fg(Color::Indexed(232)) // Near-black
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
            strikethrough_style: Style::default()
                .fg(ThemeColors::muted())
                .add_modifier(Modifier::CROSSED_OUT),
            code_style: Style::default()
                .fg(Color::Indexed(124)) // Dark red
                .bg(Color::Indexed(254)), // Very light gray background
            code_block_style: Style::default()
                .fg(Color::Indexed(235)) // Very dark gray
                .bg(Color::Indexed(254)), // Very light gray background
            link_style: Style::default()
                .fg(Color::Indexed(25)) // Dark blue
                .add_modifier(Modifier::UNDERLINED),
            quote_style: Style::default().fg(Color::Indexed(241)), // Medium gray
            list_bullet_style: Style::default().fg(Color::Indexed(240)), // Medium gray
            task_open_style: Style::default().fg(Color::Indexed(130)), // Dark orange
            task_complete_style: Style::default().fg(Color::Indexed(28)), // Dark green
            important_style: Style::default()
                .fg(Color::Indexed(160)) // Dark red
                .add_modifier(Modifier::BOLD),
            note_style: Style::default()
                .fg(Color::Indexed(25)) // Dark blue
                .add_modifier(Modifier::BOLD),
            tip_style: Style::default()
                .fg(Color::Indexed(28)) // Dark green
                .add_modifier(Modifier::BOLD),
            warning_style: Style::default()
                .fg(Color::Indexed(130)) // Dark orange
                .add_modifier(Modifier::BOLD),
            caution_style: Style::default()
                .fg(Color::Indexed(160)) // Dark red
                .add_modifier(Modifier::BOLD),
            text_style: Style::default().fg(ThemeColors::text()),
            separator_style: Style::default().fg(ThemeColors::muted()),
            table_header_style: Style::default()
                .fg(ThemeColors::text())
                .add_modifier(Modifier::BOLD),
            table_cell_style: Style::default().fg(ThemeColors::text()),
        }
    }

    /// Dark theme optimized for RGB-capable terminals
    pub fn dark_theme() -> Self {
        Self {
            h1_style: Style::default()
                .fg(Color::Rgb(100, 150, 255)) // Bright blue
                .add_modifier(Modifier::BOLD),
            h2_style: Style::default()
                .fg(Color::Rgb(100, 255, 255)) // Bright cyan
                .add_modifier(Modifier::BOLD),
            h3_style: Style::default()
                .fg(Color::Rgb(100, 255, 100)) // Bright green
                .add_modifier(Modifier::BOLD),
            h4_style: Style::default()
                .fg(Color::Rgb(255, 100, 255)) // Bright magenta
                .add_modifier(Modifier::BOLD),
            h5_style: Style::default()
                .fg(Color::Indexed(136)) // Dark yellow/gold - visible on both
                .add_modifier(Modifier::BOLD),
            h6_style: Style::default()
                .fg(Color::Rgb(255, 100, 100)) // Bright red
                .add_modifier(Modifier::BOLD),
            bold_style: Style::default().add_modifier(Modifier::BOLD),
            italic_style: Style::default().add_modifier(Modifier::ITALIC),
            bold_italic_style: Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC),
            strikethrough_style: Style::default().add_modifier(Modifier::CROSSED_OUT),
            code_style: Style::default()
                .fg(Color::Rgb(255, 150, 100)) // Orange-red for inline code
                .bg(AdaptiveColors::code_bg()),
            code_block_style: Style::default()
                .fg(Color::Rgb(150, 220, 150)) // Soft green for code blocks
                .bg(AdaptiveColors::code_block_bg()),
            link_style: Style::default()
                .fg(Color::Rgb(100, 150, 255)) // Bright blue for links
                .add_modifier(Modifier::UNDERLINED),
            quote_style: Style::default().fg(ThemeColors::muted()),
            list_bullet_style: Style::default().fg(ThemeColors::muted()),
            task_open_style: Style::default().fg(Color::Rgb(255, 200, 50)), // Bright yellow/gold
            task_complete_style: Style::default().fg(Color::Rgb(100, 255, 100)), // Bright green
            important_style: Style::default()
                .fg(Color::Rgb(255, 100, 100)) // Bright red
                .add_modifier(Modifier::BOLD),
            note_style: Style::default()
                .fg(Color::Rgb(100, 150, 255)) // Bright blue
                .add_modifier(Modifier::BOLD),
            tip_style: Style::default()
                .fg(Color::Rgb(100, 255, 100)) // Bright green
                .add_modifier(Modifier::BOLD),
            warning_style: Style::default()
                .fg(Color::Rgb(255, 200, 50)) // Bright yellow/gold
                .add_modifier(Modifier::BOLD),
            caution_style: Style::default()
                .fg(Color::Rgb(255, 100, 100)) // Bright red
                .add_modifier(Modifier::BOLD),
            text_style: Style::default().fg(ThemeColors::text()),
            separator_style: Style::default().fg(ThemeColors::muted()),
            table_header_style: Style::default()
                .fg(ThemeColors::text())
                .add_modifier(Modifier::BOLD),
            table_cell_style: Style::default().fg(ThemeColors::text()),
        }
    }

    /// High contrast theme for dark terminals without RGB support
    pub fn high_contrast_theme() -> Self {
        Self {
            h1_style: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            h2_style: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            h3_style: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            h4_style: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            h5_style: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            h6_style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            bold_style: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            italic_style: Style::default().add_modifier(Modifier::ITALIC),
            bold_italic_style: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
            strikethrough_style: Style::default().add_modifier(Modifier::CROSSED_OUT),
            code_style: Style::default().fg(Color::Red), // Red text only - no background for better compatibility
            code_block_style: Style::default().fg(Color::Cyan), // Cyan text only - no background for better compatibility
            link_style: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
            quote_style: Style::default().fg(Color::DarkGray), // Dark gray for better contrast
            list_bullet_style: Style::default().fg(Color::Reset), // Reset to terminal default for better compatibility
            task_open_style: Style::default().fg(Color::Yellow),
            task_complete_style: Style::default().fg(Color::Green),
            important_style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            note_style: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            tip_style: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            warning_style: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            caution_style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            text_style: Style::default().fg(Color::Reset), // Reset to terminal default for better compatibility
            separator_style: Style::default().fg(Color::DarkGray), // Dark gray separators
            table_header_style: Style::default()
                .fg(Color::Reset) // Reset to terminal default
                .add_modifier(Modifier::BOLD),
            table_cell_style: Style::default().fg(Color::Reset), // Reset to terminal default
        }
    }
}
