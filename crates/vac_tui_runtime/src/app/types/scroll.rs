/// Grouped message/activity scroll + cursor position.
#[derive(Debug, Clone, Default)]
pub struct ScrollState {
    /// Character offset within the input field.
    pub cursor_position: usize,
    /// Scrollback offset for the message list (0 = bottom).
    pub messages: usize,
    /// Scrollback offset for the activity pane.
    pub activity: usize,
}
