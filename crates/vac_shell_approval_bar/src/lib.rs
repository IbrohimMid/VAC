//! Step 2 slice 4 — approval bar widget extracted from Stakpak.
//!
//! # Provenance
//!
//! Source: `vendor/stakpak/tui/src/services/approval_bar.rs`
//! at commit `2e75bd56970d114ab41653aed045300b9a3257a7` (Apache 2.0).
//!
//! # Decoupling moves
//!
//! Donor's `ApprovalBar` struct conflates three responsibilities:
//! the queue itself, the operator's per-row decisions, and the
//! widget render path. Per reviewer guidance the queue + decisions
//! belong to a host crate; this crate is the pure render + key
//! handler over a caller-owned view.
//!
//! Concrete differences from the donor:
//!
//! * The donor stores `Vec<ApprovalAction>` (carrying `ToolCall`
//!   from `stakpak_shared`) inside the widget. We replace that with
//!   a flat [`ApprovalActionView`] (`id`, `label`, `status`) so the
//!   widget never sees a tool-call type.
//! * Mutators (`toggle_selected`, `reject_all`, `select_next`, …)
//!   are kept here only as helpers that mutate `ApprovalBarViewState`.
//!   Decision *policy* — what happens when the operator presses
//!   Enter — lives in `vac_shell_host_approval` behind the
//!   `ApprovalController` trait declared in `vac_shell_bridge`.
//! * `add_action` is gone; the host populates the view from the
//!   queue snapshot before each render.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalStatus {
    #[default]
    Approved,
    Rejected,
}

/// One row in the approval bar's view. Identifier is opaque — the
/// host maps it back to whatever VAC type owns the actual decision.
#[derive(Debug, Clone)]
pub struct ApprovalActionView {
    pub id: String,
    pub label: String,
    pub status: ApprovalStatus,
}

/// View state. Caller owns this; the host rebuilds the `actions`
/// slice from its queue snapshot on each tick.
#[derive(Debug, Clone, Default)]
pub struct ApprovalBarViewState {
    pub actions: Vec<ApprovalActionView>,
    pub selected_index: usize,
    pub visible: bool,
    /// Set by the operator's first Esc; the host clears it after
    /// the second Esc submits a `RejectAll`.
    pub esc_pending: bool,
}

impl ApprovalBarViewState {
    pub fn is_visible(&self) -> bool {
        self.visible && !self.actions.is_empty()
    }

    pub fn select_prev(&mut self) {
        if self.actions.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.actions.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    pub fn select_next(&mut self) {
        if self.actions.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.actions.len();
    }

    pub fn selected(&self) -> Option<&ApprovalActionView> {
        self.actions.get(self.selected_index)
    }
}

/// Logical key event the widget reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalBarKey {
    Left,
    Right,
    Space,
    Enter,
    Escape,
}

/// Outcome of a key dispatch. The host applies the resulting intent
/// against its queue via the `ApprovalController` trait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalBarEvent {
    /// Operator wants to toggle the row identified by `id`.
    Toggle(String),
    /// Operator pressed Enter — commit current decisions.
    SubmitAll,
    /// Second-Esc reject-all confirmation.
    RejectAll,
    /// First Esc — host should record `esc_pending = true` so the
    /// next Esc converts to RejectAll.
    EscPrimed,
    /// Key was handled but did not produce a host-visible event.
    Consumed,
    /// Key was not handled — caller may forward elsewhere.
    Ignored,
}

/// Stateless key handler. Mutates view (selection, esc_pending) and
/// returns the host-visible intent.
pub fn on_key(view: &mut ApprovalBarViewState, key: ApprovalBarKey) -> ApprovalBarEvent {
    if !view.is_visible() {
        return ApprovalBarEvent::Ignored;
    }
    match key {
        ApprovalBarKey::Left => {
            view.select_prev();
            ApprovalBarEvent::Consumed
        }
        ApprovalBarKey::Right => {
            view.select_next();
            ApprovalBarEvent::Consumed
        }
        ApprovalBarKey::Space => view
            .selected()
            .map(|a| ApprovalBarEvent::Toggle(a.id.clone()))
            .unwrap_or(ApprovalBarEvent::Consumed),
        ApprovalBarKey::Enter => ApprovalBarEvent::SubmitAll,
        ApprovalBarKey::Escape => {
            if view.esc_pending {
                view.esc_pending = false;
                ApprovalBarEvent::RejectAll
            } else {
                view.esc_pending = true;
                ApprovalBarEvent::EscPrimed
            }
        }
    }
}

/// Sizing helper — same shape as the donor's `calculate_height`,
/// adapted to the view-only struct. Returns 0 when not visible.
pub fn calculate_height(view: &ApprovalBarViewState, terminal_width: u16) -> u16 {
    if !view.is_visible() {
        return 0;
    }
    let inner_width = terminal_width.saturating_sub(4) as usize;
    let tab_width = inner_width.saturating_sub(2);

    let mut num_lines = 1usize;
    let mut current_width = 0usize;
    for action in &view.actions {
        let button = format!(" ✓ {} ", action.label);
        let bw = button.chars().count();
        let sep = if current_width == 0 { 0 } else { 1 };
        let need = bw + sep;
        if current_width > 0 && current_width + need > tab_width {
            num_lines += 1;
            current_width = bw;
        } else {
            current_width += need;
        }
    }
    let content_height = num_lines + num_lines.saturating_sub(1);
    let total = 1 + content_height + 1 + 1 + 1;
    (total as u16).min(15)
}

/// Convert a tool name (`run_command`, `dynamic_subagent_task`, …)
/// into a humanised label for the row. Pure helper, no donor types.
pub fn format_tool_label(tool_name: &str) -> String {
    tool_name
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render the approval bar. Donor layout preserved (top border with
/// title, wrapped buttons, footer controls, bottom border) minus the
/// donor theme service.
pub fn render_approval_bar(f: &mut Frame, view: &ApprovalBarViewState, area: Rect) {
    if !view.is_visible() || area.height < 4 {
        return;
    }
    f.render_widget(Clear, area);

    let border_color = Color::DarkGray;
    let title_color = Color::Cyan;
    let success = Color::Green;
    let danger = Color::Red;
    let highlight_fg = Color::Black;
    let highlight_bg = Color::Cyan;
    let unselected_fg = Color::Gray;
    let unselected_bg = Color::Reset;
    let muted = Color::DarkGray;
    let accent = Color::Cyan;

    let inner_width = area.width.saturating_sub(2) as usize;
    let tab_width = inner_width.saturating_sub(2);

    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut current: Vec<Span> = Vec::new();
    let mut current_width = 0usize;

    for (idx, action) in view.actions.iter().enumerate() {
        let is_selected = idx == view.selected_index;
        let (indicator, indicator_color) = match action.status {
            ApprovalStatus::Approved => ("✓", success),
            ApprovalStatus::Rejected => ("✗", danger),
        };
        let button = format!(" {} {} ", indicator, action.label);
        let bw = button.chars().count();
        let sep = if current.is_empty() { 0 } else { 1 };
        let need = bw + sep;
        if !current.is_empty() && current_width + need > tab_width {
            lines.push(current);
            current = Vec::new();
            current_width = 0;
        }
        if !current.is_empty() {
            current.push(Span::raw(" "));
            current_width += 1;
        }
        if is_selected {
            current.push(Span::styled(
                " ",
                Style::default().fg(highlight_fg).bg(highlight_bg),
            ));
            current.push(Span::styled(
                indicator,
                Style::default().fg(indicator_color).bg(highlight_bg),
            ));
            current.push(Span::styled(
                format!(" {} ", action.label),
                Style::default().fg(highlight_fg).bg(highlight_bg),
            ));
        } else {
            current.push(Span::styled(
                " ",
                Style::default().fg(unselected_fg).bg(unselected_bg),
            ));
            current.push(Span::styled(
                indicator,
                Style::default().fg(indicator_color).bg(unselected_bg),
            ));
            current.push(Span::styled(
                format!(" {} ", action.label),
                Style::default().fg(unselected_fg).bg(unselected_bg),
            ));
        }
        current_width += bw;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }

    // Top border + title.
    let title = " Approval Required ";
    let dashes_after = inner_width.saturating_sub(1 + title.len());
    let top = Line::from(vec![
        Span::styled("┌", Style::default().fg(border_color)),
        Span::styled("─", Style::default().fg(border_color)),
        Span::styled(title, Style::default().fg(title_color).add_modifier(Modifier::BOLD)),
        Span::styled("─".repeat(dashes_after), Style::default().fg(border_color)),
        Span::styled("┐", Style::default().fg(border_color)),
    ]);
    f.render_widget(Paragraph::new(top), Rect::new(area.x, area.y, area.width, 1));

    // Content rows.
    let mut current_y = area.y + 1;
    for (line_idx, tabs) in lines.iter().enumerate() {
        if current_y >= area.y + area.height.saturating_sub(2) {
            break;
        }
        if line_idx > 0 {
            let spacing = Line::from(vec![
                Span::styled("│", Style::default().fg(border_color)),
                Span::raw(" ".repeat(inner_width)),
                Span::styled("│", Style::default().fg(border_color)),
            ]);
            f.render_widget(
                Paragraph::new(spacing),
                Rect::new(area.x, current_y, area.width, 1),
            );
            current_y += 1;
            if current_y >= area.y + area.height.saturating_sub(2) {
                break;
            }
        }
        let content_w: usize = tabs.iter().map(|s| s.content.chars().count()).sum();
        let mut spans = vec![
            Span::styled("│", Style::default().fg(border_color)),
            Span::raw(" "),
        ];
        spans.extend(tabs.clone());
        let pad = inner_width.saturating_sub(content_w + 2);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled("│", Style::default().fg(border_color)));
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, current_y, area.width, 1),
        );
        current_y += 1;
    }

    // Footer controls.
    let footer_y = current_y;
    if footer_y < area.y + area.height.saturating_sub(1) {
        let controls = vec![
            Span::styled("space", Style::default().fg(accent)),
            Span::styled(" toggle", Style::default().fg(muted)),
            Span::raw("  "),
            Span::styled("←→", Style::default().fg(accent)),
            Span::styled(" navigate", Style::default().fg(muted)),
            Span::raw("  "),
            Span::styled("enter", Style::default().fg(accent)),
            Span::styled(" submit", Style::default().fg(muted)),
            Span::raw("  "),
            Span::styled("esc", Style::default().fg(accent)),
            Span::styled(" reject all", Style::default().fg(muted)),
        ];
        let cw: usize = controls.iter().map(|s| s.content.chars().count()).sum();
        let mut spans = vec![
            Span::styled("│", Style::default().fg(border_color)),
            Span::raw(" "),
        ];
        spans.extend(controls);
        let pad = inner_width.saturating_sub(cw + 2);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled("│", Style::default().fg(border_color)));
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, footer_y, area.width, 1),
        );
    }

    // Bottom border.
    let bottom_y = footer_y + 1;
    if bottom_y < area.y + area.height {
        let bottom = Line::from(vec![
            Span::styled("└", Style::default().fg(border_color)),
            Span::styled("─".repeat(inner_width), Style::default().fg(border_color)),
            Span::styled("┘", Style::default().fg(border_color)),
        ]);
        f.render_widget(
            Paragraph::new(bottom),
            Rect::new(area.x, bottom_y, area.width, 1),
        );
    }
}
