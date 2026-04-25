//! Step 2 slice 8 — model switcher widget.
//!
//! # Provenance
//!
//! Source: `vendor/stakpak/tui/src/services/model_switcher.rs` at
//! commit `2e75bd56970d114ab41653aed045300b9a3257a7` (Apache 2.0).
//! Filter logic, navigation order (recents first, then provider
//! grouping with `stakpak` first), and the reasoning-only mode are
//! preserved. Render is a thinned port that consumes `VacModelView`
//! instead of `stakai::Model`.
//!
//! # Decoupling moves
//!
//! * Donor types `stakai::Model` and `crate::app::ModelSwitcherMode`
//!   are gone. The widget consumes `VacModelView` (extended in this
//!   slice with `reasoning: bool` + `cost_label: Option<String>`)
//!   from `vac_shell_contracts` and ships its own [`SwitcherMode`]
//!   enum.
//! * Donor reads `state.model_switcher_state.{available_models,
//!   recent_models, search, selected}` directly. We collapse those
//!   onto a self-contained [`ModelSwitcherView`] the caller owns.
//! * Donor's special-case for the `stakpak` provider (always pin
//!   first) is generalised into a `pinned_provider` field — VAC
//!   hosts pass `None` or a different value as they see fit; default
//!   is `None`, so no donor-specific bias leaks unless the host
//!   opts in.
//! * Key handler is brand-new: emits `SwitcherEvent` (Selected,
//!   Dismissed, Consumed, Ignored) so the host owns the actual
//!   model switch via VAC config — no mutation lives here.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use vac_shell_contracts::{ProviderId, VacModelView};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwitcherMode {
    #[default]
    All,
    Reasoning,
}

/// View state owned entirely by the caller. Donor distributed these
/// fields between `state.model_switcher_state` and ad-hoc helpers;
/// pulling them onto one struct is what lets the widget travel.
#[derive(Debug, Clone, Default)]
pub struct ModelSwitcherView {
    pub visible: bool,
    pub mode: SwitcherMode,
    pub search: String,
    /// Model rows the host wants to expose. Order is preserved when
    /// nothing else applies — the widget only re-orders to put
    /// recents first and to group by provider.
    pub models: Vec<VacModelView>,
    /// Recent model `(provider, id)` pairs, newest-first, capped by
    /// the host. Pinned at the top of the navigation order.
    pub recent: Vec<(ProviderId, String)>,
    /// Provider that should be sorted first when grouping. Donor
    /// hard-coded `stakpak`; we leave the choice to the host.
    pub pinned_provider: Option<ProviderId>,
    /// Index into the *navigation order* (`navigation_order(view)`),
    /// not into `models`. The donor maintained the same invariant.
    pub selected: usize,
}

impl ModelSwitcherView {
    pub fn new(models: Vec<VacModelView>) -> Self {
        Self {
            models,
            ..Self::default()
        }
    }

    pub fn with_recent(mut self, recent: Vec<(ProviderId, String)>) -> Self {
        self.recent = recent;
        self
    }

    pub fn with_pinned_provider(mut self, provider: ProviderId) -> Self {
        self.pinned_provider = Some(provider);
        self
    }
}

/// Logical key event the widget reacts to. The crate stays free of
/// `crossterm` so it can be driven from any input source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitcherKey {
    Up,
    Down,
    Enter,
    Escape,
    Backspace,
    Tab,
    Char(char),
}

/// Outcome of a key dispatch. The host applies `Selected` against
/// VAC config — the widget never mutates anything beyond its view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherEvent {
    Selected { provider: ProviderId, id: String },
    Dismissed,
    Consumed,
    Ignored,
}

/// Filter the model list by mode + search. Returns indices into
/// `view.models`. Search matches against label, provider, or id
/// (case-insensitive); empty search is a passthrough.
pub fn filter_models(view: &ModelSwitcherView) -> Vec<usize> {
    let needle = view.search.trim().to_lowercase();
    view.models
        .iter()
        .enumerate()
        .filter(|(_, m)| match view.mode {
            SwitcherMode::All => true,
            SwitcherMode::Reasoning => m.reasoning,
        })
        .filter(|(_, m)| {
            if needle.is_empty() {
                return true;
            }
            m.label.to_lowercase().contains(&needle)
                || m.provider.0.to_lowercase().contains(&needle)
                || m.id.to_lowercase().contains(&needle)
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// Navigation order — recents first (in their order), then provider
/// groups (pinned provider first if set, else alphabetical), each
/// preserving caller order. Returns indices into `view.models`.
pub fn navigation_order(view: &ModelSwitcherView) -> Vec<usize> {
    let filtered: Vec<usize> = filter_models(view);
    let filtered_set: std::collections::HashSet<usize> = filtered.iter().copied().collect();

    let mut order: Vec<usize> = Vec::with_capacity(filtered.len());

    // Recents first — keep host order, drop entries that don't
    // match the current filter.
    for (rp, rid) in &view.recent {
        if let Some(idx) = view
            .models
            .iter()
            .position(|m| &m.provider == rp && &m.id == rid)
        {
            if filtered_set.contains(&idx) && !order.contains(&idx) {
                order.push(idx);
            }
        }
    }

    // Group remaining filtered models by provider.
    let mut grouped: HashMap<&str, Vec<usize>> = HashMap::new();
    for &idx in &filtered {
        if order.contains(&idx) {
            continue;
        }
        let prov = view.models[idx].provider.0.as_str();
        grouped.entry(prov).or_default().push(idx);
    }

    let mut providers: Vec<&str> = grouped.keys().copied().collect();
    let pinned = view.pinned_provider.as_ref().map(|p| p.0.as_str());
    providers.sort_by(|a, b| match (Some(*a) == pinned, Some(*b) == pinned) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    });

    for prov in providers {
        for idx in grouped.remove(prov).unwrap_or_default() {
            order.push(idx);
        }
    }

    order
}

/// Stateless key handler. Mutates view (selection, search,
/// esc-clears) and returns the host-visible intent. The host applies
/// `Selected` to VAC config; nothing inside this crate touches
/// provider state.
pub fn on_key(view: &mut ModelSwitcherView, key: SwitcherKey) -> SwitcherEvent {
    if !view.visible {
        return SwitcherEvent::Ignored;
    }
    let order = navigation_order(view);
    match key {
        SwitcherKey::Up => {
            if !order.is_empty() {
                view.selected = view.selected.saturating_sub(1);
            }
            SwitcherEvent::Consumed
        }
        SwitcherKey::Down => {
            if !order.is_empty() {
                view.selected = (view.selected + 1).min(order.len() - 1);
            }
            SwitcherEvent::Consumed
        }
        SwitcherKey::Enter => match order.get(view.selected).and_then(|i| view.models.get(*i)) {
            Some(model) => SwitcherEvent::Selected {
                provider: model.provider.clone(),
                id: model.id.clone(),
            },
            None => SwitcherEvent::Consumed,
        },
        SwitcherKey::Escape => {
            view.visible = false;
            view.search.clear();
            view.selected = 0;
            SwitcherEvent::Dismissed
        }
        SwitcherKey::Backspace => {
            view.search.pop();
            view.selected = 0;
            SwitcherEvent::Consumed
        }
        SwitcherKey::Tab => {
            view.mode = match view.mode {
                SwitcherMode::All => SwitcherMode::Reasoning,
                SwitcherMode::Reasoning => SwitcherMode::All,
            };
            view.selected = 0;
            SwitcherEvent::Consumed
        }
        SwitcherKey::Char(c) => {
            view.search.push(c);
            view.selected = 0;
            SwitcherEvent::Consumed
        }
    }
}

/// Render the model switcher popup. No-op when `!view.visible`.
pub fn render_model_switcher(f: &mut Frame, view: &ModelSwitcherView, area: Rect) {
    if !view.visible {
        return;
    }
    f.render_widget(ratatui::widgets::Clear, area);

    let cyan = Color::Cyan;
    let muted = Color::DarkGray;
    let text = Color::Gray;
    let highlight_fg = Color::Black;
    let highlight_bg = Color::Cyan;
    let success = Color::Green;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(cyan));
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    // Header line: title + mode + search.
    let mode_label = match view.mode {
        SwitcherMode::All => "all",
        SwitcherMode::Reasoning => "reasoning",
    };
    let header = Line::from(vec![
        Span::styled(" Model Switcher", Style::default().fg(cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("[{}]", mode_label), Style::default().fg(muted)),
        Span::raw("  "),
        Span::styled("> ", Style::default().fg(Color::Magenta)),
        Span::styled(view.search.clone(), Style::default().fg(text)),
        Span::styled("|", Style::default().fg(cyan)),
    ]);
    let header_area = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
    f.render_widget(Paragraph::new(header), header_area);

    // List body — render the navigation order as plain rows with
    // provider headers between groups.
    let order = navigation_order(view);
    let mut items: Vec<ListItem> = Vec::new();
    let mut last_provider: Option<&str> = None;
    let mut recent_count = view.recent.len().min(order.len());
    for (row_idx, &idx) in order.iter().enumerate() {
        let m = &view.models[idx];

        // Recents rendered without a provider header; the rest get a
        // header on first encounter.
        if recent_count > 0 {
            recent_count -= 1;
            if row_idx == 0 {
                items.push(ListItem::new(Line::from(Span::styled(
                    " recent",
                    Style::default().fg(cyan).add_modifier(Modifier::BOLD),
                ))));
            }
        } else if last_provider.as_deref() != Some(m.provider.0.as_str()) {
            last_provider = Some(m.provider.0.as_str());
            items.push(ListItem::new(Line::from(Span::styled(
                format!(" {}", m.provider.0),
                Style::default().fg(cyan).add_modifier(Modifier::BOLD),
            ))));
        }

        let is_selected = row_idx == view.selected;
        let row_fg = if is_selected { highlight_fg } else { text };
        let row_bg = if is_selected { highlight_bg } else { Color::Reset };

        let mut spans: Vec<Span<'static>> = vec![
            Span::styled("  ", Style::default()),
            Span::styled(m.label.clone(), Style::default().fg(row_fg).bg(row_bg)),
        ];
        if m.reasoning {
            spans.push(Span::styled(
                "  ★",
                Style::default()
                    .fg(success)
                    .bg(row_bg)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(cost) = &m.cost_label {
            spans.push(Span::styled(
                format!("   {}", cost),
                Style::default().fg(muted).bg(row_bg),
            ));
        }
        if !m.credentials_present {
            spans.push(Span::styled(
                "   (no creds)",
                Style::default().fg(Color::Red).bg(row_bg),
            ));
        }
        if m.active {
            spans.push(Span::styled(
                "   active",
                Style::default().fg(success).bg(row_bg),
            ));
        }
        items.push(ListItem::new(Line::from(spans)));
    }

    let list_area = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(2),
    };
    let list = List::new(items)
        .block(Block::default())
        .style(Style::default().bg(Color::Reset));
    f.render_widget(list, list_area);

    // Footer — keys.
    let footer = Line::from(vec![
        Span::styled(" ↑/↓", Style::default().fg(muted)),
        Span::styled(" navigate", Style::default().fg(cyan)),
        Span::raw("  "),
        Span::styled("enter", Style::default().fg(muted)),
        Span::styled(" select", Style::default().fg(cyan)),
        Span::raw("  "),
        Span::styled("tab", Style::default().fg(muted)),
        Span::styled(" toggle reasoning", Style::default().fg(cyan)),
        Span::raw("  "),
        Span::styled("esc", Style::default().fg(muted)),
        Span::styled(" close", Style::default().fg(cyan)),
    ]);
    let footer_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(footer), footer_area);

    f.render_widget(block, area);
}
