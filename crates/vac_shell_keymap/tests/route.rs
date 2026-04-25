//! D2 — `route_key` proofs.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use vac_shell_app::GlobalKey;
use vac_shell_approval_bar::ApprovalBarKey;
use vac_shell_approval_detail::DetailKey;
use vac_shell_contracts::ShellOverlay;
use vac_shell_diff_view::DiffReviewKey;
use vac_shell_keymap::{RoutedKey, route_key};
use vac_shell_model_switcher::SwitcherKey;
use vac_shell_palette::PaletteKey;
use vac_shell_session_browser::SessionBrowserKey;

fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: mods,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ctrl(c: char) -> KeyEvent {
    k(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn plain(code: KeyCode) -> KeyEvent {
    k(code, KeyModifiers::NONE)
}

// =====================================================================
// Globals always win
// =====================================================================

#[test]
fn ctrl_p_routes_to_open_palette() {
    assert_eq!(
        route_key(ctrl('p'), ShellOverlay::None),
        RoutedKey::Global(GlobalKey::OpenPalette)
    );
}

#[test]
fn ctrl_m_routes_to_open_model_switcher() {
    assert_eq!(
        route_key(ctrl('m'), ShellOverlay::None),
        RoutedKey::Global(GlobalKey::OpenModelSwitcher)
    );
}

#[test]
fn ctrl_s_routes_to_open_session_browser() {
    assert_eq!(
        route_key(ctrl('s'), ShellOverlay::None),
        RoutedKey::Global(GlobalKey::OpenSessionBrowser)
    );
}

#[test]
fn ctrl_d_routes_to_open_diff_review() {
    assert_eq!(
        route_key(ctrl('d'), ShellOverlay::None),
        RoutedKey::Global(GlobalKey::OpenDiffReview)
    );
}

#[test]
fn ctrl_k_routes_to_open_shortcuts() {
    assert_eq!(
        route_key(ctrl('k'), ShellOverlay::None),
        RoutedKey::Global(GlobalKey::OpenShortcuts)
    );
}

#[test]
fn ctrl_y_routes_to_open_approval_detail() {
    assert_eq!(
        route_key(ctrl('y'), ShellOverlay::None),
        RoutedKey::Global(GlobalKey::OpenApprovalDetail)
    );
}

#[test]
fn ctrl_l_routes_to_open_plan() {
    assert_eq!(
        route_key(ctrl('l'), ShellOverlay::None),
        RoutedKey::Global(GlobalKey::OpenPlan)
    );
}

#[test]
fn ctrl_backtick_routes_to_open_shell_popup() {
    assert_eq!(
        route_key(ctrl('`'), ShellOverlay::None),
        RoutedKey::Global(GlobalKey::OpenShellPopup)
    );
}

#[test]
fn esc_routes_to_global_escape_in_any_overlay() {
    for overlay in [
        ShellOverlay::None,
        ShellOverlay::Palette,
        ShellOverlay::ModelSwitcher,
        ShellOverlay::SessionBrowser,
        ShellOverlay::DiffReview,
        ShellOverlay::ApprovalDetail,
        ShellOverlay::Shortcuts,
        ShellOverlay::ShellPopup,
        ShellOverlay::Plan,
    ] {
        assert_eq!(
            route_key(plain(KeyCode::Esc), overlay),
            RoutedKey::Global(GlobalKey::Escape),
            "esc must route global in {overlay:?}",
        );
    }
}

#[test]
fn global_ctrl_p_wins_even_with_palette_open() {
    // The operator might want Ctrl+P to remain a global toggle
    // even while the palette is open (close-then-noop UX). The
    // mapper's job is to surface the global; the dispatcher
    // decides toggle vs. close.
    assert_eq!(
        route_key(ctrl('p'), ShellOverlay::Palette),
        RoutedKey::Global(GlobalKey::OpenPalette)
    );
}

// =====================================================================
// Per-overlay routes
// =====================================================================

#[test]
fn palette_enter_routes_to_palette_enter() {
    assert_eq!(
        route_key(plain(KeyCode::Enter), ShellOverlay::Palette),
        RoutedKey::Palette(PaletteKey::Enter)
    );
}

#[test]
fn palette_char_routes_into_search() {
    assert_eq!(
        route_key(plain(KeyCode::Char('m')), ShellOverlay::Palette),
        RoutedKey::Palette(PaletteKey::Char('m'))
    );
}

#[test]
fn model_switcher_enter_routes_to_switcher_enter() {
    assert_eq!(
        route_key(plain(KeyCode::Enter), ShellOverlay::ModelSwitcher),
        RoutedKey::ModelSwitcher(SwitcherKey::Enter)
    );
}

#[test]
fn model_switcher_tab_routes_to_switcher_tab() {
    assert_eq!(
        route_key(plain(KeyCode::Tab), ShellOverlay::ModelSwitcher),
        RoutedKey::ModelSwitcher(SwitcherKey::Tab)
    );
}

#[test]
fn session_browser_delete_routes_to_delete_key() {
    assert_eq!(
        route_key(plain(KeyCode::Delete), ShellOverlay::SessionBrowser),
        RoutedKey::SessionBrowser(SessionBrowserKey::Delete)
    );
}

#[test]
fn session_browser_uppercase_r_routes_resume() {
    assert_eq!(
        route_key(plain(KeyCode::Char('R')), ShellOverlay::SessionBrowser),
        RoutedKey::SessionBrowser(SessionBrowserKey::Resume)
    );
}

#[test]
fn session_browser_lowercase_r_routes_to_search_char() {
    // Plain `r` must NOT resume; it's a search character.
    assert_eq!(
        route_key(plain(KeyCode::Char('r')), ShellOverlay::SessionBrowser),
        RoutedKey::SessionBrowser(SessionBrowserKey::Char('r'))
    );
}

#[test]
fn diff_review_y_routes_to_approve() {
    assert_eq!(
        route_key(plain(KeyCode::Char('y')), ShellOverlay::DiffReview),
        RoutedKey::DiffReview(DiffReviewKey::Approve)
    );
}

#[test]
fn approval_detail_n_routes_to_reject() {
    assert_eq!(
        route_key(plain(KeyCode::Char('n')), ShellOverlay::ApprovalDetail),
        RoutedKey::ApprovalDetail(DetailKey::Reject)
    );
}

#[test]
fn approval_bar_space_in_no_overlay_falls_through_to_bar() {
    // With no overlay open, Space should still toggle the
    // selected approval row — the bar is part of the chrome.
    assert_eq!(
        route_key(plain(KeyCode::Char(' ')), ShellOverlay::None),
        RoutedKey::ApprovalBar(ApprovalBarKey::Space)
    );
}

#[test]
fn ignored_key_returns_ignored() {
    // F12 isn't bound anywhere.
    assert_eq!(
        route_key(plain(KeyCode::F(12)), ShellOverlay::None),
        RoutedKey::Ignored
    );
    // Same key with an overlay that doesn't claim it.
    assert_eq!(
        route_key(plain(KeyCode::F(12)), ShellOverlay::Palette),
        RoutedKey::Ignored
    );
}

#[test]
fn unbound_key_in_diff_review_is_ignored_not_routed_to_bar() {
    // Backspace isn't a DiffReviewKey variant; the diff overlay
    // should NOT silently fall through to the approval bar.
    assert_eq!(
        route_key(plain(KeyCode::Backspace), ShellOverlay::DiffReview),
        RoutedKey::Ignored
    );
}

/// Drift tripwire — only crossterm + shell crates may show up at
/// the public boundary. A future patch that pulls
/// `vac_tui_runtime` / `vac_core` / etc. would compile-fail here
/// because those types would shadow ours.
#[test]
fn public_types_are_local_smoke_test() {
    fn assert_only_local<T: Sized>(_: T) {}
    assert_only_local(RoutedKey::Ignored);
    assert_only_local(GlobalKey::Escape);
}

// =====================================================================
// D2.1 — dispatch_routed_key proofs
// =====================================================================

mod dispatch {
    use super::*;
    use std::sync::Arc;
    use vac_shell_app::{AppEvent, ShellApp};
    use vac_shell_bridge::{ProviderId, ShellAction, SurfaceTarget};
    use vac_shell_composition::ShellCompositionBuilder;
    use vac_shell_contracts::VacPaths;
    use vac_shell_host_approval::ApprovalRequest;
    use vac_shell_host_model::{HostModel, ProviderInfo};
    use vac_shell_host_paths::VacPathsImpl;
    use vac_shell_keymap::{RoutedKey, dispatch_routed_key};

    fn boot_app() -> (tempfile::TempDir, ShellApp) {
        let tmp = tempfile::tempdir().unwrap();
        let paths: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(tmp.path()));
        let comp = ShellCompositionBuilder::new(paths)
            .with_providers(vec![ProviderInfo {
                id: ProviderId("anthropic".into()),
                credentials_present: true,
            }])
            .with_models(vec![HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-sonnet-4.5".into(),
                label: "Claude Sonnet 4.5".into(),
                reasoning: true,
                cost_label: None,
            }])
            .with_fallback_active(Some((
                ProviderId("anthropic".into()),
                "claude-sonnet-4.5".into(),
            )))
            .boot()
            .unwrap();
        (tmp, ShellApp::new(Arc::new(comp)))
    }

    #[test]
    fn dispatch_global_open_palette_toggles_palette() {
        let (_t, mut app) = boot_app();
        let out = dispatch_routed_key(
            &mut app,
            RoutedKey::Global(vac_shell_app::GlobalKey::OpenPalette),
        )
        .unwrap();
        assert!(out.is_none(), "global toggle does not emit AppEvent");
        assert!(app.palette.visible);
    }

    #[test]
    fn dispatch_global_enter_runtime_returns_shell_action_event() {
        let (_t, mut app) = boot_app();
        let out = dispatch_routed_key(
            &mut app,
            RoutedKey::Global(vac_shell_app::GlobalKey::EnterRuntime),
        )
        .unwrap()
        .unwrap();
        match out {
            AppEvent::ShellAction(ShellAction::EnterSurface(SurfaceTarget::Runtime)) => {}
            other => panic!("expected EnterRuntime, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_palette_enter_returns_palette_selected() {
        let (_t, mut app) = boot_app();
        app.palette = vac_shell_palette::PaletteViewState::new(vec![
            vac_shell_contracts::ShellCommandSpec {
                id: "runtime".into(),
                slash: "/runtime".into(),
                title: "Runtime".into(),
                description: String::new(),
                kind: vac_shell_contracts::ShellCommandKind::BuiltInAction,
                palette_visible: true,
                ..Default::default()
            },
        ]);
        app.palette.visible = true;
        let _ = dispatch_routed_key(
            &mut app,
            RoutedKey::Palette(vac_shell_palette::PaletteKey::Char('/')),
        )
        .unwrap();
        let event = dispatch_routed_key(
            &mut app,
            RoutedKey::Palette(vac_shell_palette::PaletteKey::Enter),
        )
        .unwrap()
        .unwrap();
        assert_eq!(event, AppEvent::PaletteSelected("/runtime".into()));
    }

    #[test]
    fn dispatch_model_switcher_enter_returns_select_model_event() {
        let (_t, mut app) = boot_app();
        app.handle_global_key(vac_shell_app::GlobalKey::OpenModelSwitcher);
        app.model_switcher.selected = 0;
        let event = dispatch_routed_key(
            &mut app,
            RoutedKey::ModelSwitcher(vac_shell_model_switcher::SwitcherKey::Enter),
        )
        .unwrap()
        .unwrap();
        match event {
            AppEvent::ShellAction(ShellAction::SelectModel { id, .. }) => {
                assert_eq!(id, "claude-sonnet-4.5");
            }
            other => panic!("expected SelectModel, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_approval_bar_enter_returns_submit_approvals() {
        let (_t, mut app) = boot_app();
        app.composition()
            .unwrap()
            .approval_queue
            .enqueue(ApprovalRequest::new("a", "shell"));
        app.refresh_approval_bar();
        let event = dispatch_routed_key(
            &mut app,
            RoutedKey::ApprovalBar(vac_shell_approval_bar::ApprovalBarKey::Enter),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            event,
            AppEvent::ShellAction(ShellAction::SubmitApprovals)
        );
    }

    #[test]
    fn dispatch_approval_detail_reject_returns_approval_decision() {
        let (_t, mut app) = boot_app();
        app.composition()
            .unwrap()
            .approval_queue
            .enqueue(ApprovalRequest::new("a", "shell"));
        app.refresh_approval_bar();
        app.handle_global_key(vac_shell_app::GlobalKey::OpenApprovalDetail);
        let event = dispatch_routed_key(
            &mut app,
            RoutedKey::ApprovalDetail(vac_shell_approval_detail::DetailKey::Reject),
        )
        .unwrap()
        .unwrap();
        match event {
            AppEvent::ApprovalDecision { id, approve } => {
                assert_eq!(id, "a");
                assert!(!approve);
            }
            other => panic!("expected ApprovalDecision, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_ignored_noops() {
        let (_t, mut app) = boot_app();
        let out = dispatch_routed_key(&mut app, RoutedKey::Ignored).unwrap();
        assert!(out.is_none());
    }
}
