//! D2 — crossterm `KeyEvent` → `RoutedKey` mapping.
//!
//! Pure mapping crate. No I/O, no state, no event loop. Hosts read
//! crossterm events from their own poller, call [`route_key`] with
//! the current active overlay, and forward the resulting
//! [`RoutedKey`] through `ShellApp`'s dispatch methods (D2.1
//! concern).
//!
//! # Boundary
//!
//! Runtime deps allowed by the D-track ADR:
//!
//! * `crossterm` — `KeyEvent` only.
//! * `vac_shell_app` — `GlobalKey`.
//! * `vac_shell_contracts` — `ShellOverlay`.
//! * widget crates — for the per-overlay key enums (`PaletteKey`,
//!   `SwitcherKey`, `SessionBrowserKey`, `DiffReviewKey`,
//!   `ApprovalBarKey`, `DetailKey`).
//!
//! Forbidden: `ratatui`, `vac_core`, `vac_session_engine`,
//! `vac_tui_runtime`, donor crates, `.stakpak` paths, provider
//! APIs, secret managers.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vac_shell_app::GlobalKey;
use vac_shell_approval_bar::ApprovalBarKey;
use vac_shell_approval_detail::DetailKey;
use vac_shell_contracts::ShellOverlay;
use vac_shell_diff_view::DiffReviewKey;
use vac_shell_model_switcher::SwitcherKey;
use vac_shell_palette::PaletteKey;
use vac_shell_session_browser::SessionBrowserKey;

/// Outcome of mapping a single crossterm `KeyEvent`. The host's
/// dispatch adapter (D2.1) pattern-matches on this and forwards
/// to the right `ShellApp::dispatch_*_key` method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutedKey {
    Global(GlobalKey),
    Palette(PaletteKey),
    ModelSwitcher(SwitcherKey),
    SessionBrowser(SessionBrowserKey),
    DiffReview(DiffReviewKey),
    ApprovalBar(ApprovalBarKey),
    ApprovalDetail(DetailKey),
    Ignored,
}

/// Map one `KeyEvent` against the active overlay.
///
/// Routing rule: **global keys win** (they're the operator's
/// always-available shortcuts: `Ctrl+P`, `Ctrl+M`, …, plus
/// `Esc`). When no global key matches, fall back to the active
/// overlay's per-widget map. When neither matches, return
/// [`RoutedKey::Ignored`] so the host can forward elsewhere or
/// drop.
pub fn route_key(event: KeyEvent, active_overlay: ShellOverlay) -> RoutedKey {
    if let Some(global) = match_global(event) {
        return RoutedKey::Global(global);
    }
    match active_overlay {
        ShellOverlay::Palette => match_palette(event)
            .map(RoutedKey::Palette)
            .unwrap_or(RoutedKey::Ignored),
        ShellOverlay::ModelSwitcher => match_model_switcher(event)
            .map(RoutedKey::ModelSwitcher)
            .unwrap_or(RoutedKey::Ignored),
        ShellOverlay::SessionBrowser => match_session_browser(event)
            .map(RoutedKey::SessionBrowser)
            .unwrap_or(RoutedKey::Ignored),
        ShellOverlay::DiffReview => match_diff_review(event)
            .map(RoutedKey::DiffReview)
            .unwrap_or(RoutedKey::Ignored),
        ShellOverlay::ApprovalDetail => match_approval_detail(event)
            .map(RoutedKey::ApprovalDetail)
            .unwrap_or(RoutedKey::Ignored),
        ShellOverlay::Shortcuts
        | ShellOverlay::ShellPopup
        | ShellOverlay::Plan
        | ShellOverlay::None => match_approval_bar(event)
            .map(RoutedKey::ApprovalBar)
            .unwrap_or(RoutedKey::Ignored),
    }
}

// =====================================================================
// Global key map
// =====================================================================

fn match_global(ev: KeyEvent) -> Option<GlobalKey> {
    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
    match (ev.code, ctrl) {
        (KeyCode::Char('p'), true) | (KeyCode::Char('P'), true) => {
            Some(GlobalKey::OpenPalette)
        }
        (KeyCode::Char('k'), true) | (KeyCode::Char('K'), true) => {
            Some(GlobalKey::OpenShortcuts)
        }
        (KeyCode::Char('m'), true) | (KeyCode::Char('M'), true) => {
            Some(GlobalKey::OpenModelSwitcher)
        }
        (KeyCode::Char('s'), true) | (KeyCode::Char('S'), true) => {
            Some(GlobalKey::OpenSessionBrowser)
        }
        (KeyCode::Char('d'), true) | (KeyCode::Char('D'), true) => {
            Some(GlobalKey::OpenDiffReview)
        }
        (KeyCode::Char('y'), true) | (KeyCode::Char('Y'), true) => {
            Some(GlobalKey::OpenApprovalDetail)
        }
        // `Ctrl+\`` (backtick) opens the shell popup. Some
        // terminals deliver this as a literal `\``; accept both.
        (KeyCode::Char('`'), true) => Some(GlobalKey::OpenShellPopup),
        // Plan view toggle — `Ctrl+L` ("plan").
        (KeyCode::Char('l'), true) | (KeyCode::Char('L'), true) => {
            Some(GlobalKey::OpenPlan)
        }
        (KeyCode::Esc, _) => Some(GlobalKey::Escape),
        _ => None,
    }
}

// =====================================================================
// Per-overlay maps
// =====================================================================

fn match_palette(ev: KeyEvent) -> Option<PaletteKey> {
    match ev.code {
        KeyCode::Up => Some(PaletteKey::Up),
        KeyCode::Down => Some(PaletteKey::Down),
        KeyCode::Enter => Some(PaletteKey::Enter),
        KeyCode::Backspace => Some(PaletteKey::Backspace),
        KeyCode::Char(c) => Some(PaletteKey::Char(c)),
        _ => None,
    }
}

fn match_model_switcher(ev: KeyEvent) -> Option<SwitcherKey> {
    match ev.code {
        KeyCode::Up => Some(SwitcherKey::Up),
        KeyCode::Down => Some(SwitcherKey::Down),
        KeyCode::Enter => Some(SwitcherKey::Enter),
        KeyCode::Backspace => Some(SwitcherKey::Backspace),
        KeyCode::Tab => Some(SwitcherKey::Tab),
        KeyCode::Char(c) => Some(SwitcherKey::Char(c)),
        _ => None,
    }
}

fn match_session_browser(ev: KeyEvent) -> Option<SessionBrowserKey> {
    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
    match (ev.code, ctrl) {
        // Resume / archive use upper-case shortcuts so plain
        // typing into the search field doesn't accidentally
        // resume a session.
        (KeyCode::Char('R'), false) => Some(SessionBrowserKey::Resume),
        (KeyCode::Char('A'), false) => Some(SessionBrowserKey::Archive),
        (KeyCode::Delete, _) => Some(SessionBrowserKey::Delete),
        (KeyCode::Up, _) => Some(SessionBrowserKey::Up),
        (KeyCode::Down, _) => Some(SessionBrowserKey::Down),
        (KeyCode::Enter, _) => Some(SessionBrowserKey::Enter),
        (KeyCode::Backspace, _) => Some(SessionBrowserKey::Backspace),
        (KeyCode::Char(c), false) => Some(SessionBrowserKey::Char(c)),
        _ => None,
    }
}

fn match_diff_review(ev: KeyEvent) -> Option<DiffReviewKey> {
    match ev.code {
        KeyCode::Up => Some(DiffReviewKey::Up),
        KeyCode::Down => Some(DiffReviewKey::Down),
        KeyCode::PageUp => Some(DiffReviewKey::PageUp),
        KeyCode::PageDown => Some(DiffReviewKey::PageDown),
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(DiffReviewKey::Approve),
        KeyCode::Char('n') | KeyCode::Char('N') => Some(DiffReviewKey::Reject),
        KeyCode::Char('o') | KeyCode::Char('O') => Some(DiffReviewKey::Open),
        _ => None,
    }
}

fn match_approval_bar(ev: KeyEvent) -> Option<ApprovalBarKey> {
    match ev.code {
        KeyCode::Left => Some(ApprovalBarKey::Left),
        KeyCode::Right => Some(ApprovalBarKey::Right),
        KeyCode::Char(' ') => Some(ApprovalBarKey::Space),
        KeyCode::Enter => Some(ApprovalBarKey::Enter),
        _ => None,
    }
}

fn match_approval_detail(ev: KeyEvent) -> Option<DetailKey> {
    match ev.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(DetailKey::Approve),
        KeyCode::Char('n') | KeyCode::Char('N') => Some(DetailKey::Reject),
        _ => None,
    }
}
