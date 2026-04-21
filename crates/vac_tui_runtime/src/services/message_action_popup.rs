use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageAction {
    CopyMessage,
    CopyCode,
    Regenerate,
    RevertToMessage,
    RepairVilContract,
    ExplainPlumbing,
    AuditZeroCopy,
    DiffIrChange,
}

impl MessageAction {
    pub fn all() -> Vec<Self> {
        vec![
            Self::CopyMessage,
            Self::CopyCode,
            Self::Regenerate,
            Self::RevertToMessage,
            Self::RepairVilContract,
            Self::ExplainPlumbing,
            Self::AuditZeroCopy,
            Self::DiffIrChange,
        ]
    }
}

pub fn render_message_action_popup(f: &mut Frame, state: &AppState) {
    if !state
        .overlay_manager
        .is_active(crate::overlay::OverlayId::MessageAction)
    {
        return;
    }

    let popup_width: u16 = 50;
    let popup_height: u16 = 11;

    let terminal_area = f.area();
    let x = (terminal_area.width.saturating_sub(popup_width)) / 2;
    let y = (terminal_area.height.saturating_sub(popup_height)) / 2;

    let area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(state.theme.style(StyleKey::BorderFocused));

    f.render_widget(block, area);

    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(2),
        ])
        .split(inner_area);

    let title = Paragraph::new(Line::from(vec![Span::styled(
        " Message Action",
        state.theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
    )]));
    f.render_widget(title, chunks[0]);

    let actions = MessageAction::all();
    let mut item_lines: Vec<Line> = Vec::new();

    for (idx, action) in actions.iter().enumerate() {
        let is_selected = idx == state.message_action_popup_selected;

        let (highlight_word, rest_text) = match action {
            MessageAction::CopyMessage => ("Copy", " message text to clipboard"),
            MessageAction::CopyCode => ("Copy Code", " extract code blocks to clipboard"),
            MessageAction::Regenerate => ("Regenerate", " discard this and retry"),
            MessageAction::RevertToMessage => ("Revert", " undo messages and file changes"),
            MessageAction::RepairVilContract => {
                ("Repair VIL Contract", " auto-generate fixes for VIL rules")
            }
            MessageAction::ExplainPlumbing => {
                ("Explain Plumbing", " explain generated VIL plumbing")
            }
            MessageAction::AuditZeroCopy => {
                ("Audit Zero-Copy", " detect zero-copy risks in handler")
            }
            MessageAction::DiffIrChange => ("Diff IR Change", " view semantic IR-significant diff"),
        };

        let available_width = (inner_area.width as usize).saturating_sub(2);
        let text_len = 2 + highlight_word.len() + rest_text.len();
        let padding = available_width.saturating_sub(text_len);

        let line = if is_selected {
            Line::from(vec![
                Span::styled("  ", state.theme.style(StyleKey::ToastInfo)),
                Span::styled(
                    highlight_word,
                    state
                        .theme
                        .style(StyleKey::OverlaySelected)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest_text, state.theme.style(StyleKey::ToastInfo)),
                Span::styled(
                    " ".repeat(padding),
                    state.theme.style(StyleKey::ToastInfo),
                ),
            ])
        } else {
            Line::from(vec![
                Span::raw("  "),
                Span::styled(highlight_word, Style::default()),
                Span::styled(rest_text, state.theme.style(StyleKey::Muted)),
            ])
        };

        item_lines.push(line);
    }

    let items = Paragraph::new(item_lines);
    f.render_widget(items, chunks[2]);
}

pub fn get_selected_action(state: &AppState) -> Option<MessageAction> {
    let actions = MessageAction::all();
    actions.get(state.message_action_popup_selected).copied()
}
