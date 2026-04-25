//! Slice 20 — real shell app integration.
//!
//! `ShellApp` is the single struct an operator-facing TUI host
//! constructs and drives. It owns every widget's view state plus
//! the `ShellComposition`, the `OverlayStack`, and the key→action
//! shim that maps logical key events into `OverlayIntent` and
//! `ShellAction` values.
//!
//! Layout (inside `area`):
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │ activity stream / chat surface / runtime body          │
//! ├─────────────────────────────────────────────────────────┤
//! │ status bar (1 line)                                    │
//! └─────────────────────────────────────────────────────────┘
//!
//! Overlays render above the body when active.
//! ```

use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use vac_shell_activity::ActivityView;
use vac_shell_approval_bar::ApprovalBarViewState;
use vac_shell_bridge::{ShellAction, ShellHost, SurfaceTarget};
use vac_shell_composition::ShellComposition;
use vac_shell_contracts::{OverlayIntent, ShellOverlay};
use vac_shell_diff_view::DiffReviewView;
use vac_shell_host_status::{StatusInputs, project_status};
use vac_shell_model_switcher::ModelSwitcherView;
use vac_shell_overlay::OverlayStack;
use vac_shell_palette::PaletteViewState;
use vac_shell_plan_view::render_plan_view;
use vac_shell_session_browser::SessionBrowserView;
use vac_shell_shortcuts::ShortcutsView;
use vac_shell_status_bar::render_status_bar;

/// All view state owned by the app. Each widget's state is
/// caller-owned per the boundary discipline; this is the caller.
#[derive(Default)]
pub struct ShellApp {
    pub composition: Option<Arc<ShellComposition>>,
    pub overlays: OverlayStack,
    pub palette: PaletteViewState,
    pub shortcuts: ShortcutsView,
    pub model_switcher: ModelSwitcherView,
    pub approval_bar: ApprovalBarViewState,
    pub session_browser: SessionBrowserView,
    pub diff_review: DiffReviewView,
    pub activity: ActivityView,
    pub plan: Option<vac_shell_plan::PlanMetadata>,
    pub status_inputs: StatusInputs,
}

impl ShellApp {
    pub fn new(composition: Arc<ShellComposition>) -> Self {
        Self {
            composition: Some(composition),
            ..Default::default()
        }
    }

    pub fn composition(&self) -> Option<&ShellComposition> {
        self.composition.as_deref()
    }

    /// Logical key the host derives from crossterm or another
    /// source. Slice 20 keeps this tiny — wider key handling lives
    /// in each widget's own crate.
    pub fn handle_global_key(&mut self, key: GlobalKey) -> Option<ShellAction> {
        match key {
            GlobalKey::OpenPalette => {
                self.overlays
                    .apply_intent(OverlayIntent::Toggle(ShellOverlay::Palette));
                self.palette.visible = self.overlays.top() == ShellOverlay::Palette;
                None
            }
            GlobalKey::OpenShortcuts => {
                self.overlays
                    .apply_intent(OverlayIntent::Toggle(ShellOverlay::Shortcuts));
                self.shortcuts.visible = self.overlays.top() == ShellOverlay::Shortcuts;
                None
            }
            GlobalKey::OpenModelSwitcher => {
                self.overlays
                    .apply_intent(OverlayIntent::Toggle(ShellOverlay::ModelSwitcher));
                self.model_switcher.visible =
                    self.overlays.top() == ShellOverlay::ModelSwitcher;
                None
            }
            GlobalKey::OpenSessionBrowser => {
                self.overlays
                    .apply_intent(OverlayIntent::Toggle(ShellOverlay::SessionBrowser));
                self.session_browser.visible =
                    self.overlays.top() == ShellOverlay::SessionBrowser;
                None
            }
            GlobalKey::OpenPlan => {
                self.overlays
                    .apply_intent(OverlayIntent::Toggle(ShellOverlay::Plan));
                None
            }
            GlobalKey::OpenDiffReview => {
                self.overlays
                    .apply_intent(OverlayIntent::Toggle(ShellOverlay::DiffReview));
                self.diff_review.visible = self.overlays.top() == ShellOverlay::DiffReview;
                None
            }
            GlobalKey::Escape => {
                self.overlays.apply_intent(OverlayIntent::CloseTop);
                self.sync_visibility();
                None
            }
            GlobalKey::EnterRuntime => Some(ShellAction::EnterSurface(SurfaceTarget::Runtime)),
            GlobalKey::EnterChat => Some(ShellAction::EnterSurface(SurfaceTarget::Chat)),
        }
    }

    fn sync_visibility(&mut self) {
        let top = self.overlays.top();
        self.palette.visible = top == ShellOverlay::Palette;
        self.shortcuts.visible = top == ShellOverlay::Shortcuts;
        self.model_switcher.visible = top == ShellOverlay::ModelSwitcher;
        self.session_browser.visible = top == ShellOverlay::SessionBrowser;
        self.diff_review.visible = top == ShellOverlay::DiffReview;
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        // Body — the activity stream is always present. Surface
        // switching (chat / runtime body) is the host's call to make
        // beyond slice 20.
        vac_shell_activity::render_activity(f, &self.activity, chunks[0]);

        // Bottom status bar.
        if let Some(comp) = &self.composition {
            let view = project_status(comp, &self.status_inputs);
            render_status_bar(f, &view, chunks[1]);
        }

        // Overlays — render the active one in a centred rect.
        let overlay_area = centered_rect(80, 70, area);
        match self.overlays.top() {
            ShellOverlay::None => {}
            ShellOverlay::Palette => {
                vac_shell_palette::render_palette(f, &self.palette, overlay_area);
            }
            ShellOverlay::Shortcuts => {
                vac_shell_shortcuts::render_shortcuts_popup(
                    f,
                    &self.shortcuts,
                    overlay_area,
                );
            }
            ShellOverlay::ModelSwitcher => {
                vac_shell_model_switcher::render_model_switcher(
                    f,
                    &self.model_switcher,
                    overlay_area,
                );
            }
            ShellOverlay::SessionBrowser => {
                vac_shell_session_browser::render_session_browser(
                    f,
                    &self.session_browser,
                    overlay_area,
                );
            }
            ShellOverlay::Plan => {
                render_plan_view(f, self.plan.as_ref(), overlay_area);
            }
            ShellOverlay::DiffReview => {
                vac_shell_diff_view::render_diff_review(
                    f,
                    &self.diff_review,
                    overlay_area,
                );
            }
            ShellOverlay::ShellPopup => {
                // The shell popup widget is wired separately by the
                // host because it owns its own state struct; slice 20
                // keeps its render path out of the catch-all stack.
            }
        }
    }
}

/// Logical global key the host derives from its input source. Each
/// widget consumes its own narrower keymap once the relevant
/// overlay is on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalKey {
    OpenPalette,
    OpenShortcuts,
    OpenModelSwitcher,
    OpenSessionBrowser,
    OpenPlan,
    OpenDiffReview,
    EnterRuntime,
    EnterChat,
    Escape,
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let h = area.height.saturating_mul(percent_y) / 100;
    let w = area.width.saturating_mul(percent_x) / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}
