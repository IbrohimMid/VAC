//! Unified Commands, Shortcuts & Sessions Popup
//!
//! This module provides a unified popup with:
//! - Commands section: Searchable and triggerable command palette items
//! - Shortcuts section: Read-only keyboard shortcuts grouped by category
//! - Sessions section: List of previous sessions to resume

use crate::services::detect_term::ThemeColors;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};
use std::sync::OnceLock;

use crate::app::ShortcutsPopupMode;
use crate::constants::SCROLL_BUFFER_LINES;
use crate::services::commands::{Command, CommandAction};

/// Static UI-action entries that have no slash-command equivalent.
fn ui_action_commands() -> Vec<Command> {
    vec![
        Command {
            name: "Switch Profile".into(),
            description: "Change the active user profile".into(),
            shortcut: "Ctrl+F".into(),
            action: CommandAction::OpenProfileSwitcher,
        },
        Command {
            name: "Switch Rulebook".into(),
            description: "Change the active VIL rulebook".into(),
            shortcut: "".into(),
            action: CommandAction::OpenRulebookSwitcher,
        },
        Command {
            name: "Resume Session".into(),
            description: "View and resume previous sessions".into(),
            shortcut: "".into(),
            action: CommandAction::OpenSessions,
        },
        Command {
            name: "Keyboard Shortcuts".into(),
            description: "View all keyboard shortcuts".into(),
            shortcut: "Ctrl+S".into(),
            action: CommandAction::OpenShortcuts,
        },
        Command {
            name: "Clear Screen".into(),
            description: "Clear the terminal output".into(),
            shortcut: "".into(),
            action: CommandAction::ClearScreen,
        },
        Command {
            name: "Auto-Approve".into(),
            description: "Toggle tool auto-approval".into(),
            shortcut: "Ctrl+O".into(),
            action: CommandAction::ToggleAutoApprove,
        },
        Command {
            name: "Quit".into(),
            description: "Exit the application".into(),
            shortcut: "Ctrl+C".into(),
            action: CommandAction::Quit,
        },
    ]
}

/// Build the full command list from UI-action commands plus slash commands
/// derived from the canonical `vac_commands()` registry.
fn get_all_commands() -> Vec<Command> {
    let mut cmds = ui_action_commands();
    // Derive InsertSlashCommand entries from the single canonical registry so
    // the palette never lists phantom commands or drifts from the dispatcher.
    for spec in crate::services::helper_block::vac_commands() {
        // Skip commands marked as Hidden — they should not appear in operator surfaces.
        if spec.surface == crate::services::commands::CommandSurface::Hidden {
            continue;
        }
        let display_name = spec
            .command
            .strip_prefix('/')
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => s.to_string(),
                }
            })
            .unwrap_or_else(|| spec.command.clone());
        let shortcut = spec.shortcut.clone().unwrap_or_default();
        cmds.push(Command {
            name: display_name,
            description: spec.description.clone(),
            shortcut,
            action: CommandAction::InsertSlashCommand(spec.command.clone()),
        });
    }
    cmds
}

pub fn filter_commands(query: &str, state: &crate::app::AppState) -> Vec<Command> {
    let mut cmds = if query.is_empty() {
        get_all_commands()
    } else {
        let query_lower = query.to_lowercase();
        get_all_commands()
            .into_iter()
            .filter(|cmd| {
                cmd.name.to_lowercase().contains(&query_lower)
                    || cmd.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    };

    cmds.sort_by_key(|cmd| {
        let cmd_id = match &cmd.action {
            CommandAction::InsertSlashCommand(s) => s.clone(),
            _ => cmd.name.clone(),
        };
        let freq = state
            .recent_commands
            .frequencies
            .get(&cmd_id)
            .copied()
            .unwrap_or(0);
        let recent_idx = state
            .recent_commands
            .history
            .iter()
            .position(|h| h == &cmd_id)
            .unwrap_or(usize::MAX);
        (std::cmp::Reverse(freq), recent_idx)
    });

    cmds
}

#[derive(Debug, Clone)]
pub struct Shortcut {
    pub key: String,
    pub description: String,
    pub category: String,
}

impl Shortcut {
    pub fn new(key: &str, description: &str, category: &str) -> Self {
        Self {
            key: key.to_string(),
            description: description.to_string(),
            category: category.to_string(),
        }
    }
}

pub fn get_all_shortcuts() -> Vec<Shortcut> {
    let mut shortcuts = vec![
        // Navigation
        Shortcut::new("↑/↓", "Navigate messages", "Navigation"),
        Shortcut::new("Page Up/Down", "Page through messages", "Navigation"),
        Shortcut::new("Ctrl+↑/↓", "Navigate dropdown/dialog", "Navigation"),
        Shortcut::new("Tab", "Complete command or select file", "Navigation"),
        Shortcut::new("Esc", "Close dialogs/popups", "Navigation"),
        // Text Input
        Shortcut::new("Ctrl+A", "Move cursor to start of line", "Text Input"),
        Shortcut::new("Ctrl+E", "Move cursor to end of line", "Text Input"),
        Shortcut::new("Ctrl+F", "Move cursor right", "Text Input"),
        Shortcut::new("Ctrl+B", "Move cursor left", "Text Input"),
        Shortcut::new("Alt+F", "Move cursor to next word", "Text Input"),
        Shortcut::new("Alt+B", "Move cursor to previous word", "Text Input"),
        Shortcut::new("Ctrl+U", "Delete to start of line", "Text Input"),
        Shortcut::new("Ctrl+W", "Delete previous word", "Text Input"),
        Shortcut::new("Ctrl+H", "Delete previous character", "Text Input"),
        Shortcut::new("Ctrl+J", "Insert newline", "Text Input"),
        Shortcut::new("Enter", "Submit input", "Text Input"),
        Shortcut::new("Backspace", "Delete previous character", "Text Input"),
        // Tool Management
        Shortcut::new("Ctrl+O", "Toggle auto-approve mode", "Tool Management"),
        Shortcut::new("Ctrl+Y", "Toggle side panel", "Tool Management"),
        Shortcut::new("Ctrl+R", "Retry last tool call", "Tool Management"),
        // UI Controls
        Shortcut::new("Ctrl+C", "Quit (double press)", "UI Controls"),
        Shortcut::new("Ctrl+L", "Toggle mouse capture", "UI Controls"),
        Shortcut::new("Ctrl+F", "Show profile switcher", "UI Controls"),
        Shortcut::new("Ctrl+P", "Show command palette", "UI Controls"),
        Shortcut::new("Ctrl+S", "Show shortcuts (this popup)", "UI Controls"),
        Shortcut::new("Ctrl+G", "Show file changes", "UI Controls"),
        Shortcut::new("Ctrl+X", "Copy session ID", "UI Controls"),
        // File Search
        Shortcut::new("@", "Trigger file search", "File Search"),
        Shortcut::new("$", "Enter interactive shell mode", "File Search"),
        Shortcut::new("Tab", "Select file from search", "File Search"),
        // Mouse
        Shortcut::new("Scroll Up/Down", "Scroll messages", "Mouse"),
        Shortcut::new("Click", "Interact with UI elements", "Mouse"),
    ];

    for spec in crate::services::helper_block::vac_commands() {
        if spec.surface == crate::services::commands::CommandSurface::Hidden {
            continue;
        }
        shortcuts.push(Shortcut::new(&spec.command, &spec.description, "Commands"));
    }

    shortcuts
}

// Cache the shortcuts content to prevent constant recreation
static SHORTCUTS_CACHE: OnceLock<Vec<Line<'static>>> = OnceLock::new();

pub fn get_cached_shortcuts_content(width: Option<usize>) -> &'static Vec<Line<'static>> {
    SHORTCUTS_CACHE.get_or_init(|| {
        let shortcuts = get_all_shortcuts();

        // Group shortcuts by category
        let mut categories: std::collections::HashMap<&str, Vec<&Shortcut>> =
            std::collections::HashMap::new();
        for shortcut in &shortcuts {
            categories
                .entry(&shortcut.category)
                .or_default()
                .push(shortcut);
        }

        // Define the EXACT order we want categories to appear
        let category_order = vec![
            "Navigation",
            "Text Input",
            "Tool Management",
            "UI Controls",
            "Commands",
            "File Search",
            "Mouse",
        ];

        // Create all lines for the popup
        let mut all_lines = Vec::new();
        // push empty line
        all_lines.push(Line::from(""));

        // Process categories in the EXACT order defined above
        for category_name in &category_order {
            if let Some(category_shortcuts) = categories.get(category_name) {
                // Add category header
                let category_style = Style::default()
                    .fg(ThemeColors::cyan())
                    .add_modifier(Modifier::BOLD);
                let category_width = width.unwrap_or(40).saturating_sub(category_name.len() + 5);
                all_lines.push(Line::from(vec![
                    Span::styled(format!(" {} ", category_name), category_style),
                    Span::styled(
                        "─".repeat(category_width).to_string(),
                        Style::default().fg(ThemeColors::dark_gray()),
                    ), // Fixed width to avoid recalculation
                ]));

                // Add shortcuts for this category - FIXED ALIGNMENT
                for shortcut in category_shortcuts {
                    // Use fixed-width formatting for perfect alignment
                    let key_formatted = format!(" {:<25}", shortcut.key); // Left-align in 25 chars
                    let description_formatted = format!("{:<40} ", shortcut.description); // Left-align in 40 chars

                    let spans = vec![
                        Span::styled(
                            key_formatted,
                            Style::default()
                                .fg(ThemeColors::green())
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            description_formatted,
                            Style::default().fg(ThemeColors::text()),
                        ),
                    ];

                    all_lines.push(Line::from(spans));
                }

                // Add empty line between categories
                all_lines.push(Line::from(""));
            }
        }

        all_lines
    })
}

/// Get the total count of actual shortcuts (green items only)
pub fn get_shortcuts_count() -> usize {
    get_all_shortcuts().len()
}

pub fn render_shortcuts_popup(f: &mut Frame, state: &mut crate::app::AppState) {
    // Calculate popup size (60% width, fit height to content)
    let area = centered_rect(60, 80, f.area());

    f.render_widget(ratatui::widgets::Clear, area);

    // Create the main block with border and background
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ThemeColors::cyan()));

    // Split area for title, tabs, and content - layout differs by mode
    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };

    // Render title inside the popup
    let title = " Command Palette";
    let title_style = Style::default()
        .fg(ThemeColors::title())
        .add_modifier(Modifier::BOLD);
    let title_line = Line::from(Span::styled(title, title_style));
    let title_paragraph = Paragraph::new(title_line);

    // Render tabs
    let tab_titles = vec![" Commands ", " Shortcuts ", " Sessions "];
    let selected_tab = match state.shortcuts_mode {
        ShortcutsPopupMode::Commands => 0,
        ShortcutsPopupMode::Shortcuts => 1,
        ShortcutsPopupMode::Sessions => 2,
    };
    let tabs = Tabs::new(tab_titles)
        .select(selected_tab)
        .style(Style::default().fg(ThemeColors::muted()))
        .highlight_style(
            Style::default()
                .fg(ThemeColors::accent())
                .add_modifier(Modifier::BOLD),
        )
        .divider(" | ");

    // Render content based on mode with mode-specific layouts
    match state.shortcuts_mode {
        ShortcutsPopupMode::Commands => {
            // Commands mode: Title, Tabs, Search, Content, Scroll, Help
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Title
                    Constraint::Length(1), // Tabs
                    Constraint::Length(3), // Search
                    Constraint::Min(3),    // Content
                    Constraint::Length(1), // Scroll indicators
                    Constraint::Length(1), // Help text
                ])
                .split(inner_area);

            f.render_widget(title_paragraph, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            render_commands_section(f, state, chunks[2], chunks[3], chunks[4], chunks[5], area);
        }
        ShortcutsPopupMode::Shortcuts => {
            // Shortcuts mode: Title, Tabs, Spacer, Search, Content, Scroll, Help
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Title
                    Constraint::Length(1), // Tabs
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Search (reduced from 3 if we remove border)
                    Constraint::Min(3),    // Content
                    Constraint::Length(1), // Scroll indicators
                    Constraint::Length(1), // Help text
                ])
                .split(inner_area);

            f.render_widget(title_paragraph, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            // spacer at chunks[2] is empty
            render_shortcuts_section(f, state, chunks[3], chunks[4], chunks[5], chunks[6], area);
        }
        ShortcutsPopupMode::Sessions => {
            // Sessions mode: Title, Tabs, Spacer, Search, Spacer2, Content, Scroll, Help
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Title
                    Constraint::Length(1), // Tabs
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Search
                    Constraint::Length(1), // Spacer between search and list
                    Constraint::Min(3),    // Content (sessions list)
                    Constraint::Length(1), // Scroll indicators
                    Constraint::Length(1), // Help text
                ])
                .split(inner_area);

            f.render_widget(title_paragraph, chunks[0]);
            f.render_widget(tabs, chunks[1]);
            // spacer at chunks[2] is empty
            // spacer at chunks[4] is empty (between search and list)
            render_sessions_section(f, state, chunks[3], chunks[5], chunks[6], chunks[7]);
        }
    }

    // Render the border with title last (so it's on top)
    f.render_widget(block, area);
}

fn render_commands_section(
    f: &mut Frame,
    state: &crate::app::AppState,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
    area: Rect,
) {
    // Render search input
    let search_prompt = ">";
    let cursor = "|";
    let placeholder = "Type to filter";

    let search_spans = if state.command_palette_input.is_empty() {
        vec![
            Span::raw(" "), // Small space before
            Span::styled(search_prompt, Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(cursor, Style::default().fg(ThemeColors::cyan())),
            Span::styled(placeholder, Style::default().fg(ThemeColors::dark_gray())),
            Span::raw(" "), // Small space after
        ]
    } else {
        vec![
            Span::raw(" "), // Small space before
            Span::styled(search_prompt, Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(
                &state.command_palette_input,
                Style::default()
                    .fg(ThemeColors::text())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::default().fg(ThemeColors::cyan())),
            Span::raw(" "), // Small space after
        ]
    };

    let search_text = ratatui::text::Text::from(vec![
        Line::from(""), // Empty line above
        Line::from(search_spans),
        Line::from(""), // Empty line below
    ]);
    let search_paragraph = Paragraph::new(search_text);
    f.render_widget(search_paragraph, search_area);

    // Get filtered commands
    let filtered_commands = filter_commands(&state.command_palette_input, state);
    let total_commands = filtered_commands.len();
    let height = content_area.height as usize;

    // Calculate scroll position
    let max_scroll = total_commands.saturating_sub(height.saturating_sub(SCROLL_BUFFER_LINES));
    let scroll = if state.command_palette_scroll > max_scroll {
        max_scroll
    } else {
        state.command_palette_scroll
    };

    // Add top arrow indicator if there are hidden items above
    let mut visible_lines = Vec::new();
    let has_content_above = scroll > 0;
    if has_content_above {
        visible_lines.push(Line::from(vec![Span::styled(
            " ▲",
            Style::default().fg(Color::Reset),
        )]));
    }

    // Create visible lines
    for i in 0..height {
        let line_index = scroll + i;
        if line_index < total_commands {
            let command = &filtered_commands[line_index];
            let available_width = area.width as usize - 2; // Account for borders
            let is_selected = line_index == state.command_palette_selected;
            let bg_color = if is_selected {
                ThemeColors::highlight_bg()
            } else {
                Color::Reset
            };
            let text_color = if is_selected {
                ThemeColors::highlight_fg()
            } else {
                ThemeColors::text()
            };

            // Create a single line with name on left and shortcut on right
            let name_formatted = format!(
                " {:<width$}",
                command.name,
                width = available_width.saturating_sub(command.shortcut.len() + 2)
            );
            let shortcut_formatted = format!("{} ", command.shortcut);

            let spans = vec![
                Span::styled(name_formatted, Style::default().fg(text_color).bg(bg_color)),
                Span::styled(
                    shortcut_formatted,
                    Style::default()
                        .fg(if is_selected {
                            ThemeColors::highlight_fg()
                        } else {
                            ThemeColors::dark_gray()
                        })
                        .bg(bg_color),
                ),
            ];

            visible_lines.push(Line::from(spans));
        } else {
            visible_lines.push(Line::from(""));
        }
    }

    // Render content
    let content_paragraph = Paragraph::new(visible_lines)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .style(Style::default().bg(Color::Reset).fg(ThemeColors::text()));

    f.render_widget(content_paragraph, content_area);

    // Scroll indicators
    let has_content_below = scroll < max_scroll;

    if has_content_above || has_content_below {
        let mut indicator_spans = vec![];

        // Show cumulative commands counter
        let cumulative = (scroll + height).min(total_commands);
        indicator_spans.push(Span::styled(
            format!(" ({}/{})", cumulative, total_commands),
            Style::default().fg(Color::Reset),
        ));

        if has_content_below {
            indicator_spans.push(Span::styled(
                " ▼",
                Style::default().fg(ThemeColors::dark_gray()),
            ));
        }

        let indicator_paragraph = Paragraph::new(Line::from(indicator_spans));
        f.render_widget(indicator_paragraph, scroll_area);
    } else {
        // Empty line when no scroll indicators
        f.render_widget(Paragraph::new(""), scroll_area);
    }

    // Help text
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑/↓", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" navigate", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("enter", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" select", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("tab", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" switch", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("esc", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" close", Style::default().fg(ThemeColors::cyan())),
    ]));

    f.render_widget(help, help_area);
}

fn render_shortcuts_section(
    f: &mut Frame,
    state: &mut crate::app::AppState,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
    area: Rect,
) {
    // Render search input
    let search_term = &state.command_palette_input;
    let search_prompt = ">";
    let cursor = "|";
    let placeholder = "Type to filter (e.g. 'ctrl+')";

    let search_spans = if search_term.is_empty() {
        vec![
            Span::raw(" "), // Small space before
            Span::styled(search_prompt, Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(cursor, Style::default().fg(ThemeColors::cyan())),
            Span::styled(placeholder, Style::default().fg(ThemeColors::dark_gray())),
            Span::raw(" "), // Small space after
        ]
    } else {
        vec![
            Span::raw(" "), // Small space before
            Span::styled(search_prompt, Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(search_term, Style::default().fg(ThemeColors::text())),
            Span::styled(cursor, Style::default().fg(ThemeColors::cyan())),
        ]
    };

    f.render_widget(
        Paragraph::new(Line::from(search_spans))
            .block(Block::default().border_style(Style::default().fg(ThemeColors::dark_gray()))),
        search_area,
    );

    // Get shortcuts content (filtered or cached)
    let search_lower = search_term.to_lowercase();
    let all_lines = if search_term.is_empty() {
        get_cached_shortcuts_content(Some(area.width as usize)).clone()
    } else {
        // Dynamic filtering
        let all_shortcuts = get_all_shortcuts();
        let filtered: Vec<&Shortcut> = all_shortcuts
            .iter()
            .filter(|s| {
                s.key.to_lowercase().contains(&search_lower)
                    || s.description.to_lowercase().contains(&search_lower)
                    || s.category.to_lowercase().contains(&search_lower)
            })
            .collect();

        // Rebuild lines (reusing logic from get_cached_shortcuts_content slightly simplified)
        let mut lines = Vec::new();
        lines.push(Line::from(""));

        let mut categories: std::collections::HashMap<&str, Vec<&Shortcut>> =
            std::collections::HashMap::new();
        for s in filtered {
            categories.entry(&s.category).or_default().push(s);
        }

        let category_order = vec![
            "Navigation",
            "Text Input",
            "Tool Management",
            "UI Controls",
            "Commands",
            "File Search",
            "Mouse",
        ];

        for category_name in &category_order {
            if let Some(category_shortcuts) = categories.get(category_name) {
                // Add category header
                let category_style = Style::default()
                    .fg(ThemeColors::cyan())
                    .add_modifier(Modifier::BOLD);
                let category_width =
                    area.width.saturating_sub(category_name.len() as u16 + 5) as usize;
                lines.push(Line::from(vec![
                    Span::styled(format!(" {} ", category_name), category_style),
                    Span::styled(
                        "─".repeat(category_width).to_string(),
                        Style::default().fg(ThemeColors::dark_gray()),
                    ),
                ]));

                for shortcut in category_shortcuts {
                    let key_formatted = format!(" {:<25}", shortcut.key);
                    let description_formatted = format!("{:<40} ", shortcut.description);

                    lines.push(Line::from(vec![
                        Span::styled(
                            key_formatted,
                            Style::default()
                                .fg(ThemeColors::green())
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            description_formatted,
                            Style::default().fg(ThemeColors::text()),
                        ),
                    ]));
                }
                lines.push(Line::from(""));
            }
        }
        lines
    };

    let total_lines = all_lines.len();
    let height = content_area.height as usize;
    let shortcuts_count = if search_term.is_empty() {
        get_shortcuts_count()
    } else {
        // Count filtered shortcuts
        let mut count = 0;
        for line in &all_lines {
            for span in &line.spans {
                if span.style.fg == Some(ThemeColors::green()) {
                    count += 1;
                    break;
                }
            }
        }
        count
    };

    // Calculate scroll position (similar to collapsed messages)
    let max_scroll = total_lines.saturating_sub(height.saturating_sub(SCROLL_BUFFER_LINES));

    state.shortcuts_scroll = state.shortcuts_scroll.min(max_scroll);
    let scroll = state.shortcuts_scroll;

    // Add top arrow indicator if there are hidden items above
    let mut visible_lines = Vec::new();
    let has_content_above = scroll > 0;
    if has_content_above {
        visible_lines.push(Line::from(vec![Span::styled(
            " ▲",
            Style::default().fg(Color::Reset),
        )]));
    }

    // Create visible lines (similar to collapsed messages)
    for i in 0..height {
        let line_index = scroll + i;
        if line_index < all_lines.len() {
            visible_lines.push(all_lines[line_index].clone());
        } else {
            visible_lines.push(Line::from(""));
        }
    }

    // Render as paragraph with static lines
    let content_paragraph = Paragraph::new(visible_lines)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .style(Style::default().bg(Color::Reset).fg(ThemeColors::text()));

    f.render_widget(content_paragraph, content_area);

    // Calculate cumulative shortcuts count (including scrolled past ones)
    let mut cumulative_shortcuts_count = 0;

    // Count shortcuts from the beginning up to the current scroll position + visible area
    for line_index in 0..=(scroll + height).min(all_lines.len().saturating_sub(1)) {
        if line_index < all_lines.len() {
            let line = &all_lines[line_index];
            // Check if this line contains a shortcut (green text)
            for span in &line.spans {
                if span.style.fg == Some(ThemeColors::green())
                    && span.style.add_modifier.contains(Modifier::BOLD)
                {
                    cumulative_shortcuts_count += 1;
                    break; // Count each line only once
                }
            }
        }
    }

    // Scroll indicators (above help line)
    let has_content_above = scroll > 0;
    let has_content_below = scroll < max_scroll;

    if has_content_above || has_content_below {
        let mut indicator_spans = vec![];

        // Show cumulative shortcuts counter and down arrow on the left
        indicator_spans.push(Span::styled(
            format!(" ({}/{})", cumulative_shortcuts_count, shortcuts_count),
            Style::default().fg(Color::Reset),
        ));

        if has_content_below {
            indicator_spans.push(Span::styled(
                " ▼",
                Style::default().fg(ThemeColors::dark_gray()),
            ));
        }

        let indicator_paragraph = Paragraph::new(Line::from(indicator_spans));
        f.render_widget(indicator_paragraph, scroll_area);
    } else {
        // Empty line when no scroll indicators
        f.render_widget(Paragraph::new(""), scroll_area);
    }

    // Help text
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑/↓", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" scroll", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("tab", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" switch", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("esc", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" close", Style::default().fg(ThemeColors::cyan())),
    ]));

    f.render_widget(help, help_area);
}

fn render_sessions_section(
    f: &mut Frame,
    state: &mut crate::app::AppState,
    search_area: Rect,
    content_area: Rect,
    scroll_area: Rect,
    help_area: Rect,
) {
    // Render search input (reuse command_palette_input since tabs are mutually exclusive)
    let search_term = &state.command_palette_input;
    let search_prompt = ">";
    let cursor = "|";
    let placeholder = "Type to filter sessions";

    let search_spans = if search_term.is_empty() {
        vec![
            Span::raw(" "),
            Span::styled(search_prompt, Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(cursor, Style::default().fg(ThemeColors::cyan())),
            Span::styled(placeholder, Style::default().fg(ThemeColors::dark_gray())),
        ]
    } else {
        vec![
            Span::raw(" "),
            Span::styled(search_prompt, Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(
                search_term.clone(),
                Style::default()
                    .fg(ThemeColors::text())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::default().fg(ThemeColors::cyan())),
        ]
    };

    f.render_widget(Paragraph::new(Line::from(search_spans)), search_area);

    // Filter sessions by search term
    let search_lower = search_term.to_lowercase();
    let filtered_sessions: Vec<(usize, &crate::app::SessionInfo)> = state
        .sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| search_term.is_empty() || s.title.to_lowercase().contains(&search_lower))
        .collect();

    let total_filtered = filtered_sessions.len();
    let height = content_area.height as usize;

    if filtered_sessions.is_empty() {
        // Show empty state message
        let empty_message = if state.sessions.is_empty() {
            " No sessions available"
        } else {
            " No sessions match your search"
        };
        let empty_widget = Paragraph::new(Line::from(vec![Span::styled(
            empty_message,
            Style::default().fg(ThemeColors::dark_gray()),
        )]));
        f.render_widget(empty_widget, content_area);
        f.render_widget(Paragraph::new(""), scroll_area);
    } else {
        // Ensure sessions_selected_idx is within bounds of filtered list
        let selected_in_filtered = state
            .sessions_selected_idx
            .min(total_filtered.saturating_sub(1));

        // Calculate scroll position based on selected item
        let max_scroll = total_filtered.saturating_sub(height);
        let scroll = if selected_in_filtered >= height {
            (selected_in_filtered - height + 1).min(max_scroll)
        } else {
            0
        };

        // Determine which sessions to show
        let visible_end = (scroll + height).min(total_filtered);

        let has_content_above = scroll > 0;
        let has_content_below = visible_end < total_filtered;

        // Build visible lines
        let mut visible_lines: Vec<Line> = Vec::new();

        // Add top arrow if there's content above
        if has_content_above {
            visible_lines.push(Line::from(vec![Span::styled(
                " ▲",
                Style::default().fg(ThemeColors::dark_gray()),
            )]));
        }

        // Create session items for visible range
        for (_filtered_idx, (original_idx, session)) in filtered_sessions
            .iter()
            .enumerate()
            .skip(scroll)
            .take(height)
        {
            let formatted_datetime = if let Ok(dt) =
                chrono::DateTime::parse_from_rfc3339(&session.updated_at.replace(" UTC", "+00:00"))
            {
                dt.format("%Y-%m-%d %H:%M").to_string()
            } else {
                let parts = session.updated_at.split('T').collect::<Vec<_>>();
                let date = parts.first().unwrap_or(&"");
                let time = parts.get(1).and_then(|t| t.split('.').next()).unwrap_or("");
                format!("{} {}", date, time)
            };

            let text = format!(" {} . {}", formatted_datetime, session.title);
            let is_selected = *original_idx == state.sessions_selected_idx;

            let (fg, bg) = if is_selected {
                (ThemeColors::highlight_fg(), ThemeColors::highlight_bg())
            } else {
                (ThemeColors::text(), Color::Reset)
            };

            let style = if is_selected {
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(fg).bg(bg)
            };

            visible_lines.push(Line::from(vec![Span::styled(text, style)]));
        }

        let content_paragraph = Paragraph::new(visible_lines);
        f.render_widget(content_paragraph, content_area);

        // Scroll indicators - only show when there are hidden items
        if has_content_above || has_content_below {
            let mut indicator_spans = vec![];

            // Show cumulative count: how many items visible from top up to bottom of visible area
            let cumulative_count = visible_end;

            indicator_spans.push(Span::styled(
                format!(" ({}/{})", cumulative_count, total_filtered),
                Style::default().fg(Color::Reset),
            ));

            if has_content_below {
                indicator_spans.push(Span::styled(
                    " ▼",
                    Style::default().fg(ThemeColors::dark_gray()),
                ));
            }

            let indicator_paragraph = Paragraph::new(Line::from(indicator_spans));
            f.render_widget(indicator_paragraph, scroll_area);
        }
    }

    // Help text
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑/↓", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" navigate", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("enter", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" select", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("tab", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" switch", Style::default().fg(ThemeColors::cyan())),
        Span::raw("  "),
        Span::styled("esc", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled(" close", Style::default().fg(ThemeColors::cyan())),
    ]));

    f.render_widget(help, help_area);
}

/// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
