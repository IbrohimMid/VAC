//! Banner Service
//!
//! Persistent/dismissible banner strip rendered above the main workspace.
//! Supports three use cases: time-limited messages, persistent notices,
//! and click-through CTAs (entire banner acts as one action region, or
//! individual slash-commands inside the text become clickable regions).

use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

const BANNER_MESSAGE_DURATION: Duration = Duration::from_secs(60);

pub const BANNER_VISIBLE_HEIGHT: u16 = 3;

// ========== Unit 8 (Wave 3.6) — Banner maturity ==========

/// Severity controls auto-expiry and dismissibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BannerSeverity {
    /// Must be explicitly dismissed; blocks input until acknowledged.
    Blocking,
    /// Auto-expires after 30s, can be dismissed early.
    Suggested,
    /// Auto-expires after 10s, lowest priority.
    Informative,
}

impl BannerSeverity {
    pub fn auto_expiry(self) -> Option<Duration> {
        match self {
            Self::Blocking => None,
            Self::Suggested => Some(Duration::from_secs(30)),
            Self::Informative => Some(Duration::from_secs(10)),
        }
    }
}

/// An inline button embedded in the banner strip.
#[derive(Debug, Clone)]
pub struct BannerAction {
    pub label: String,
    pub command: String,
    pub keybind_hint: Option<String>,
}

/// Stable identifier for deduplication and dismissed-memory.
pub type BannerId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerStyle {
    Warning,
    Error,
    Info,
    Success,
}

impl BannerStyle {
    pub fn style_key(&self) -> StyleKey {
        match self {
            BannerStyle::Warning => StyleKey::Warning,
            BannerStyle::Error => StyleKey::Error,
            BannerStyle::Info => StyleKey::Accent,
            BannerStyle::Success => StyleKey::Success,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BannerMessage {
    pub id: BannerId,
    pub text: String,
    pub created_at: Instant,
    pub style: BannerStyle,
    pub persistent: bool,
    pub action: Option<String>,
    // Unit 8 extensions
    pub severity: BannerSeverity,
    pub actions: Vec<BannerAction>,
}

impl BannerMessage {
    pub fn new(text: impl Into<String>, style: BannerStyle) -> Self {
        let text = text.into();
        let id = format!("banner-{:x}", fxhash(&text));
        Self {
            id,
            text,
            created_at: Instant::now(),
            style,
            persistent: false,
            action: None,
            severity: BannerSeverity::Informative,
            actions: Vec::new(),
        }
    }

    pub fn persistent(text: impl Into<String>, style: BannerStyle) -> Self {
        let text = text.into();
        let id = format!("banner-{:x}", fxhash(&text));
        Self {
            id,
            text,
            created_at: Instant::now(),
            style,
            persistent: true,
            action: None,
            severity: BannerSeverity::Blocking,
            actions: Vec::new(),
        }
    }

    pub fn persistent_with_action(
        text: impl Into<String>,
        style: BannerStyle,
        action: impl Into<String>,
    ) -> Self {
        let text = text.into();
        let id = format!("banner-{:x}", fxhash(&text));
        Self {
            id,
            text,
            created_at: Instant::now(),
            style,
            persistent: true,
            action: Some(action.into()),
            severity: BannerSeverity::Blocking,
            actions: Vec::new(),
        }
    }

    pub fn with_severity(mut self, severity: BannerSeverity) -> Self {
        self.severity = severity;
        if severity == BannerSeverity::Blocking {
            self.persistent = true;
        }
        self
    }

    pub fn with_actions(mut self, actions: Vec<BannerAction>) -> Self {
        self.actions = actions;
        self
    }

    pub fn is_expired(&self) -> bool {
        if self.persistent {
            return false;
        }
        let expiry = self
            .severity
            .auto_expiry()
            .unwrap_or(BANNER_MESSAGE_DURATION);
        self.created_at.elapsed() > expiry
    }
}

/// Stacked banner queue. At most one banner renders at a time; `Ctrl+B` cycles.
#[derive(Debug, Clone, Default)]
pub struct BannerQueue {
    pub queue: VecDeque<BannerMessage>,
    pub dismissed: HashSet<BannerId>,
}

impl BannerQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a banner; deduplicates by id and skips dismissed banners.
    pub fn push(&mut self, msg: BannerMessage) {
        if self.dismissed.contains(&msg.id) {
            return;
        }
        if self.queue.iter().any(|m| m.id == msg.id) {
            return;
        }
        self.queue.push_back(msg);
    }

    /// Remove expired and dismissed entries, return the front (visible) banner.
    pub fn current(&mut self) -> Option<&BannerMessage> {
        // Drain expired/dismissed from front
        while let Some(front) = self.queue.front() {
            if front.is_expired() || self.dismissed.contains(&front.id) {
                self.queue.pop_front();
            } else {
                break;
            }
        }
        self.queue.front()
    }

    /// Dismiss the current banner (move to next in queue).
    pub fn dismiss_current(&mut self) {
        if let Some(msg) = self.queue.pop_front() {
            self.dismissed.insert(msg.id);
        }
    }

    /// Cycle: move the front banner to the back.
    pub fn cycle(&mut self) {
        if let Some(msg) = self.queue.pop_front() {
            self.queue.push_back(msg);
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// Cheap non-cryptographic hash for banner id generation.
fn fxhash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn banner_height(state: &AppState) -> u16 {
    match &state.banner_message {
        Some(msg) if !msg.is_expired() => BANNER_VISIBLE_HEIGHT,
        _ => 0,
    }
}

fn find_slash_commands(text: &str) -> Vec<(usize, String)> {
    let mut commands = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '/' {
            let is_word_start = i == 0
                || text[..i]
                    .chars()
                    .last()
                    .is_some_and(|prev| prev.is_whitespace());

            if is_word_start {
                let start = i;
                let mut end = i + 1;
                while let Some(&(j, ch)) = chars.peek() {
                    if ch.is_whitespace() {
                        break;
                    }
                    end = j + ch.len_utf8();
                    chars.next();
                }
                let cmd = text[start..end].to_string();

                if cmd.len() > 1 {
                    commands.push((start, cmd));
                }
            }
        }
    }
    commands
}

pub fn render_banner(f: &mut Frame, area: Rect, state: &mut AppState) {
    if let Some(msg) = &state.banner_message
        && msg.is_expired()
    {
        state.banner_message = None;
    }

    let Some(msg) = &state.banner_message else {
        return;
    };

    let border_style = state.theme.style(msg.style.style_key());
    let accent_style = state.theme.style(StyleKey::Accent);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .padding(ratatui::widgets::Padding::horizontal(1));

    let commands = find_slash_commands(&msg.text);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut per_cmd_regions: Vec<(String, Rect)> = Vec::new();

    let mut char_x: u16 = 2;
    let mut byte_offset: usize = 0;

    if commands.is_empty() {
        spans.push(Span::raw(msg.text.clone()));
    } else {
        for (cmd_start, cmd) in &commands {
            if *cmd_start > byte_offset {
                let plain = &msg.text[byte_offset..*cmd_start];
                char_x += plain.chars().count() as u16;
                spans.push(Span::raw(plain.to_string()));
            }

            let cmd_width = cmd.chars().count() as u16;
            let cmd_rect = Rect::new(
                area.x.saturating_add(char_x),
                area.y.saturating_add(1),
                cmd_width,
                1,
            );
            per_cmd_regions.push((cmd.clone(), cmd_rect));

            let styled = Span::styled(
                cmd.clone(),
                accent_style.add_modifier(Modifier::UNDERLINED),
            );
            spans.push(styled);

            char_x += cmd_width;
            byte_offset = *cmd_start + cmd.len();
        }

        if byte_offset < msg.text.len() {
            spans.push(Span::raw(msg.text[byte_offset..].to_string()));
        }
    }

    let click_regions = if let Some(action) = &msg.action {
        vec![(action.clone(), area)]
    } else {
        per_cmd_regions
    };

    let content_width = area.width.saturating_sub(4) as usize;
    let text_width: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let dismiss_label = " x ";
    let pad = content_width.saturating_sub(text_width + dismiss_label.len());
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
    }
    spans.push(Span::styled(
        dismiss_label.to_string(),
        state
            .theme
            .style(msg.style.style_key())
            .add_modifier(Modifier::DIM),
    ));

    let dismiss_width: u16 = 5;
    let dismiss_x = area.x + area.width.saturating_sub(2 + dismiss_width);
    let dismiss_y = area.y;
    state.banner_dismiss_region = Some(Rect::new(
        dismiss_x,
        dismiss_y,
        dismiss_width + 2,
        area.height,
    ));

    let paragraph = Paragraph::new(Line::from(spans))
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
    state.banner_click_regions = click_regions;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn queue_deduplicates_by_id() {
        let mut q = BannerQueue::new();
        let msg1 = BannerMessage::new("hello", BannerStyle::Info);
        let msg2 = BannerMessage::new("hello", BannerStyle::Warning); // same text = same id
        q.push(msg1);
        q.push(msg2);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn queue_dismiss_skips_to_next() {
        let mut q = BannerQueue::new();
        q.push(BannerMessage::new("first", BannerStyle::Info));
        q.push(BannerMessage::new("second", BannerStyle::Warning));
        assert_eq!(q.current().unwrap().text, "first");
        q.dismiss_current();
        assert_eq!(q.current().unwrap().text, "second");
    }

    #[test]
    fn queue_dismissed_banners_not_re_added() {
        let mut q = BannerQueue::new();
        let msg = BannerMessage::new("once", BannerStyle::Info);
        let id = msg.id.clone();
        q.push(msg);
        q.dismiss_current();
        assert!(q.dismissed.contains(&id));
        // Try to re-add
        q.push(BannerMessage::new("once", BannerStyle::Info));
        assert!(q.current().is_none());
    }

    #[test]
    fn queue_cycle_moves_front_to_back() {
        let mut q = BannerQueue::new();
        q.push(BannerMessage::new("a", BannerStyle::Info));
        q.push(BannerMessage::new("b", BannerStyle::Info));
        assert_eq!(q.current().unwrap().text, "a");
        q.cycle();
        assert_eq!(q.current().unwrap().text, "b");
        q.cycle();
        assert_eq!(q.current().unwrap().text, "a");
    }

    #[test]
    fn severity_expiry_durations() {
        assert_eq!(BannerSeverity::Blocking.auto_expiry(), None);
        assert_eq!(
            BannerSeverity::Suggested.auto_expiry(),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            BannerSeverity::Informative.auto_expiry(),
            Some(Duration::from_secs(10))
        );
    }

    #[test]
    fn blocking_banner_never_expires() {
        let msg = BannerMessage::persistent("important", BannerStyle::Error)
            .with_severity(BannerSeverity::Blocking);
        assert!(!msg.is_expired());
        assert!(msg.persistent);
    }

    #[test]
    fn banner_with_actions() {
        let msg = BannerMessage::new("upgrade available", BannerStyle::Info)
            .with_severity(BannerSeverity::Suggested)
            .with_actions(vec![BannerAction {
                label: "Upgrade".into(),
                command: "/upgrade".into(),
                keybind_hint: Some("u".into()),
            }]);
        assert_eq!(msg.actions.len(), 1);
        assert_eq!(msg.actions[0].command, "/upgrade");
    }
}
