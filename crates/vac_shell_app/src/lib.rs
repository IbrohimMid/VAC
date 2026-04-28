//! Slice 20 / 20.1 — real shell app integration.
//!
//! `ShellApp` is the single struct an operator-facing TUI host
//! constructs and drives. It owns every widget's view state plus
//! the `ShellComposition`, the `OverlayStack`, and the key→action
//! shim that maps logical key events into `OverlayIntent`,
//! `ShellAction`, and per-overlay events.
//!
//! Layout (inside `area`):
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │ activity stream / chat surface                             │
//! ├────────────────────────────────────────────────────────────┤
//! │ approval bar (visible only when the queue is non-empty)    │
//! ├────────────────────────────────────────────────────────────┤
//! │ status bar (1 line)                                        │
//! └────────────────────────────────────────────────────────────┘
//!
//! Overlays render above the body when active.
//! ```

use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use vac_shell_activity::{
    ActivityLogBrowserView, ActivityView, LogsBrowserKey, on_logs_browser_key,
};
use vac_shell_approval_bar::{
    ApprovalActionView, ApprovalBarEvent, ApprovalBarKey, ApprovalBarViewState, ApprovalStatus,
};
use vac_shell_approval_detail::{ApprovalDetailViewState, DetailEvent, DetailKey};
use vac_shell_bridge::{ApprovalController, ShellAction, SurfaceTarget};
use vac_shell_composition::ShellComposition;
use vac_shell_contracts::{OverlayIntent, SessionAction, ShellOverlay};
use vac_shell_diff_view::{DiffReviewKey, DiffReviewKeyEvent, DiffReviewView};
use vac_shell_host_status::{StatusInputs, project_status};
use vac_shell_init_checklist::{
    InitChecklistAction, InitChecklistEvent, InitChecklistKey, InitChecklistView, on_init_key,
    render_init_checklist,
};
use vac_shell_model_switcher::{ModelSwitcherView, SwitcherEvent, SwitcherKey};
use vac_shell_overlay::OverlayStack;
use vac_shell_palette::{PaletteEvent, PaletteKey, PaletteViewState};
use vac_shell_plan_view::render_plan_view;
use vac_shell_popup::ShellPopupViewState;
use vac_shell_session_browser::{SessionBrowserEvent, SessionBrowserKey, SessionBrowserView};
use vac_shell_shortcuts::ShortcutsView;
use vac_shell_status_bar::render_status_bar;

const RECENTS_LIMIT: usize = 5;

/// D10.5 — groups injected provider/callback fields so ShellApp doesn't
/// become an unbounded DI container. Hosts set these at construction time
/// via the builder methods or direct field assignment.
#[derive(Default)]
pub struct ShellAppProviders {
    /// Risk classification + command preview for the approval detail drawer.
    pub approval_detail: Option<Arc<dyn vac_shell_host_approval::ApprovalDetailProvider>>,
    /// D9/D11 summary callback — injected by host entrypoint to avoid engine dep in ShellApp.
    pub session_tool_use_provider: Option<
        Arc<
            dyn Fn(&std::path::Path) -> Option<vac_shell_contracts::SessionToolUseSurface>
                + Send
                + Sync,
        >,
    >,
    /// D16 checkpoint recovery callback — injected by host entrypoint.
    pub session_recovery_provider: Option<
        Arc<dyn Fn(&str) -> Option<vac_shell_contracts::SessionRecoverySummary> + Send + Sync>,
    >,
    /// D18 init checklist provider — injected by host entrypoint.
    pub init_checklist_provider:
        Option<Arc<dyn Fn() -> vac_shell_contracts::InitChecklistViewModel + Send + Sync>>,
}

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
    pub approval_detail: ApprovalDetailViewState,
    pub session_browser: SessionBrowserView,
    pub diff_review: DiffReviewView,
    pub activity: ActivityView,
    pub logs_browser: ActivityLogBrowserView,
    pub init_checklist: InitChecklistView,
    pub plan: Option<vac_shell_plan::PlanMetadata>,
    pub shell_popup: ShellPopupViewState,
    pub status_inputs: StatusInputs,
    /// Slice 20.1 — host-side sessions controller. Set by the host
    /// when sessions disk lifecycle should be active; left `None`
    /// in pure-render tests.
    pub sessions: Option<Arc<vac_shell_host_sessions::SessionsState>>,
    /// Slice 20.3 — production error reporting sink. Failed
    /// `apply_event` calls (host dispatch, sessions lifecycle,
    /// approval toggles) are recorded here as an
    /// `ShellActivityKind::Error` entry so the operator sees what
    /// went wrong instead of staring at a silent UI. Optional so
    /// pure-render tests don't have to instantiate one.
    pub activity_log: Option<Arc<vac_shell_host_activity::ActivityLog>>,
    /// D10.5 — grouped provider/callback fields.
    pub providers: ShellAppProviders,
}

/// Outbound app events the host loop consumes after a key press.
/// Multiple events may fire for one key (e.g. an overlay close
/// plus a `ShellAction`); the loop applies each in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    /// Routed through `ShellComposition.host` by the loop.
    ShellAction(ShellAction),
    /// Session-lifecycle intent. The loop forwards to a
    /// `SessionsState` (`vac_shell_host_sessions`).
    Session(SessionAction),
    /// Diff-review intent.
    DiffReview(vac_shell_contracts::DiffReviewEvent),
    /// Approval drawer outcome. The loop applies the decision
    /// against the live `ApprovalController` from the composition.
    ApprovalDecision { id: String, approve: bool },
    /// Palette emitted a `Selected` slash. The loop dispatches via
    /// the registered command path.
    PaletteSelected(String),
}

impl ShellApp {
    pub fn new(composition: Arc<ShellComposition>) -> Self {
        let mut app = Self::default();
        app.composition = Some(composition);
        app.providers.approval_detail = Some(Arc::new(
            vac_shell_host_approval::DefaultApprovalDetailProvider,
        ));
        app
    }

    pub fn with_approval_detail_provider(
        mut self,
        p: Arc<dyn vac_shell_host_approval::ApprovalDetailProvider>,
    ) -> Self {
        self.providers.approval_detail = Some(p);
        self
    }

    pub fn with_session_tool_use_provider(
        mut self,
        f: Arc<
            dyn Fn(&std::path::Path) -> Option<vac_shell_contracts::SessionToolUseSurface>
                + Send
                + Sync,
        >,
    ) -> Self {
        self.providers.session_tool_use_provider = Some(f);
        self
    }

    pub fn with_session_recovery_provider(
        mut self,
        f: Arc<dyn Fn(&str) -> Option<vac_shell_contracts::SessionRecoverySummary> + Send + Sync>,
    ) -> Self {
        self.providers.session_recovery_provider = Some(f);
        self
    }

    pub fn composition(&self) -> Option<&ShellComposition> {
        self.composition.as_deref()
    }

    /// Refresh the approval bar's `actions` from the live queue.
    /// The host loop calls this when the queue changes (or every
    /// tick for simplicity). Pure projection — does not mutate the
    /// queue, only the view state.
    pub fn refresh_approval_bar(&mut self) {
        let comp = match &self.composition {
            Some(c) => c,
            None => return,
        };
        let snap = comp.approval_queue.snapshot();
        self.approval_bar.actions = snap
            .iter()
            .map(|r| ApprovalActionView {
                id: r.id.clone(),
                label: r.label.clone(),
                status: r.status,
            })
            .collect();
        self.approval_bar.visible = !self.approval_bar.actions.is_empty();
        if self.approval_bar.selected_index >= self.approval_bar.actions.len() {
            self.approval_bar.selected_index = 0;
        }
    }

    /// D17 — refresh logs browser from the activity log snapshot.
    pub fn refresh_logs_browser(&mut self) {
        if let Some(ref log) = self.activity_log {
            self.logs_browser.entries = log.snapshot();
        } else {
            self.logs_browser.entries.clear();
        }
    }

    /// D17 — open logs browser with fresh snapshot.
    fn open_logs_browser(&mut self) {
        self.refresh_logs_browser();
        self.overlays
            .apply_intent(OverlayIntent::Open(ShellOverlay::Logs));
        self.sync_visibility();
    }

    /// D18 — open init checklist with provider output.
    fn open_init_checklist(&mut self) {
        if let Some(provider) = &self.providers.init_checklist_provider {
            self.init_checklist.model = provider();
        } else {
            self.init_checklist.model = vac_shell_contracts::InitChecklistViewModel {
                title: "Init Checklist".to_string(),
                rows: vec![],
                next_action: Some("No init provider attached".to_string()),
            };
        }
        self.overlays
            .apply_intent(OverlayIntent::Open(ShellOverlay::Init));
        self.sync_visibility();
    }

    /// Logical key the host derives from crossterm or another
    /// source.
    pub fn handle_global_key(&mut self, key: GlobalKey) -> Option<ShellAction> {
        match key {
            GlobalKey::OpenPalette => {
                // Slice 20.2 — project the live registry into the
                // palette state on open so the overlay never opens
                // empty or stale.
                if let Some(comp) = &self.composition {
                    self.palette =
                        vac_shell_palette::PaletteViewState::new(comp.command_registry.all());
                }
                self.toggle_overlay(ShellOverlay::Palette);
                None
            }
            GlobalKey::OpenShortcuts => {
                self.toggle_overlay(ShellOverlay::Shortcuts);
                None
            }
            GlobalKey::OpenModelSwitcher => {
                // Slice 20.2 — project the live model state into
                // the switcher view so Ctrl+M never opens empty.
                if let Some(comp) = &self.composition {
                    self.model_switcher =
                        vac_shell_host_model::build_switcher_view(&comp.model_state, RECENTS_LIMIT);
                }
                self.toggle_overlay(ShellOverlay::ModelSwitcher);
                None
            }
            GlobalKey::OpenSessionBrowser => {
                // Slice 20.2 / D10-HARDENING — populate session tiles (with
                // tool-use badges) from the live SessionsState (if attached).
                // The summarize closure is injected by the host via
                // `session_tool_use_provider`; ShellApp has no direct dep
                // on vac_shell_host_transcript_projection.
                // D16 — also inject checkpoint recovery via `session_recovery_provider`.
                if let (Some(comp), Some(sessions)) = (&self.composition, &self.sessions) {
                    let tool_provider = self.providers.session_tool_use_provider.clone();
                    let recovery_provider = self.providers.session_recovery_provider.clone();
                    self.session_browser.tiles = sessions.list_with_tool_use_and_recovery(
                        comp.paths.as_ref(),
                        |path| tool_provider.as_ref().and_then(|f| f(path)),
                        |id| recovery_provider.as_ref().and_then(|f| f(id)),
                    );
                    if self.session_browser.selected >= self.session_browser.tiles.len() {
                        self.session_browser.selected = 0;
                    }
                }
                self.toggle_overlay(ShellOverlay::SessionBrowser);
                None
            }
            GlobalKey::OpenPlan => {
                self.toggle_overlay(ShellOverlay::Plan);
                None
            }
            GlobalKey::OpenDiffReview => {
                self.toggle_overlay(ShellOverlay::DiffReview);
                None
            }
            GlobalKey::OpenShellPopup => {
                self.toggle_overlay(ShellOverlay::ShellPopup);
                None
            }
            GlobalKey::OpenApprovalDetail => {
                // D10 — drawer reads from the live approval bar selection,
                // then enriches via the approval detail provider.
                self.refresh_approval_bar();
                if let Some(action) = self.approval_bar.selected() {
                    let detail = if let (Some(comp), Some(provider)) =
                        (&self.composition, &self.providers.approval_detail)
                    {
                        let queue_snap = comp.approval_queue.snapshot();
                        queue_snap
                            .iter()
                            .find(|r| r.id == action.id)
                            .map(|req| provider.detail_for(req))
                    } else {
                        None
                    };
                    if let Some(d) = detail {
                        self.approval_detail.detail = Some(d);
                    }
                }
                self.toggle_overlay(ShellOverlay::ApprovalDetail);
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

    /// Slice 20.2 — frame refresh contract. The host loop calls
    /// this once per tick *before* `render` so projected widget
    /// state (currently the approval bar) is up-to-date with the
    /// live composition. Render itself stays `&self` to keep the
    /// contract simple for ratatui's draw closure.
    pub fn prepare_frame(&mut self) {
        self.refresh_approval_bar();
    }

    fn toggle_overlay(&mut self, overlay: ShellOverlay) {
        self.overlays.apply_intent(OverlayIntent::Toggle(overlay));
        self.sync_visibility();
    }

    fn sync_visibility(&mut self) {
        let top = self.overlays.top();
        self.palette.visible = top == ShellOverlay::Palette;
        self.shortcuts.visible = top == ShellOverlay::Shortcuts;
        self.model_switcher.visible = top == ShellOverlay::ModelSwitcher;
        self.session_browser.visible = top == ShellOverlay::SessionBrowser;
        self.diff_review.visible = top == ShellOverlay::DiffReview;
        self.shell_popup.visible = top == ShellOverlay::ShellPopup;
        self.approval_detail.visible = top == ShellOverlay::ApprovalDetail;
        self.logs_browser.visible = top == ShellOverlay::Logs;
        self.init_checklist.visible = top == ShellOverlay::Init;
    }

    // -----------------------------------------------------------
    // Slice 20.1 — overlay-key routing into AppEvent
    // -----------------------------------------------------------

    pub fn dispatch_palette_key(&mut self, key: PaletteKey) -> Option<AppEvent> {
        match vac_shell_palette::on_key(&mut self.palette, key) {
            PaletteEvent::Selected(slash) => Some(AppEvent::PaletteSelected(slash)),
            PaletteEvent::Dismissed => {
                self.overlays.apply_intent(OverlayIntent::CloseTop);
                self.sync_visibility();
                None
            }
            _ => None,
        }
    }

    pub fn dispatch_model_switcher_key(&mut self, key: SwitcherKey) -> Option<AppEvent> {
        match vac_shell_model_switcher::on_key(&mut self.model_switcher, key) {
            SwitcherEvent::Selected { provider, id } => {
                Some(AppEvent::ShellAction(ShellAction::SelectModel {
                    provider,
                    id,
                }))
            }
            SwitcherEvent::Dismissed => {
                self.overlays.apply_intent(OverlayIntent::CloseTop);
                self.sync_visibility();
                None
            }
            _ => None,
        }
    }

    pub fn dispatch_session_browser_key(&mut self, key: SessionBrowserKey) -> Option<AppEvent> {
        match vac_shell_session_browser::on_key(&mut self.session_browser, key) {
            SessionBrowserEvent::Action(action) => Some(AppEvent::Session(action)),
            SessionBrowserEvent::Dismissed => {
                self.overlays.apply_intent(OverlayIntent::CloseTop);
                self.sync_visibility();
                None
            }
            _ => None,
        }
    }

    pub fn dispatch_diff_review_key(&mut self, key: DiffReviewKey) -> Option<AppEvent> {
        match vac_shell_diff_view::on_key(&mut self.diff_review, key) {
            DiffReviewKeyEvent::Event(e) => match e {
                vac_shell_contracts::DiffReviewEvent::Dismiss => {
                    self.overlays.apply_intent(OverlayIntent::CloseTop);
                    self.sync_visibility();
                    None
                }
                other => Some(AppEvent::DiffReview(other)),
            },
            _ => None,
        }
    }

    pub fn dispatch_approval_bar_key(&mut self, key: ApprovalBarKey) -> Option<AppEvent> {
        match vac_shell_approval_bar::on_key(&mut self.approval_bar, key) {
            ApprovalBarEvent::Toggle(id) => {
                Some(AppEvent::ShellAction(ShellAction::ToggleApproval { id }))
            }
            ApprovalBarEvent::SubmitAll => {
                Some(AppEvent::ShellAction(ShellAction::SubmitApprovals))
            }
            ApprovalBarEvent::RejectAll => {
                Some(AppEvent::ShellAction(ShellAction::RejectAllApprovals))
            }
            _ => None,
        }
    }

    pub fn dispatch_approval_detail_key(&mut self, key: DetailKey) -> Option<AppEvent> {
        match vac_shell_approval_detail::on_key(&mut self.approval_detail, key) {
            DetailEvent::Approve(id) => Some(AppEvent::ApprovalDecision { id, approve: true }),
            DetailEvent::Reject(id) => Some(AppEvent::ApprovalDecision { id, approve: false }),
            DetailEvent::Dismissed => {
                self.overlays.apply_intent(OverlayIntent::CloseTop);
                self.sync_visibility();
                None
            }
            _ => None,
        }
    }

    pub fn dispatch_logs_browser_key(&mut self, key: LogsBrowserKey) -> Option<AppEvent> {
        on_logs_browser_key(&mut self.logs_browser, key);
        match key {
            LogsBrowserKey::Escape => {
                self.overlays.apply_intent(OverlayIntent::CloseTop);
                self.sync_visibility();
                None
            }
            _ => None,
        }
    }

    pub fn dispatch_init_checklist_key(&mut self, key: InitChecklistKey) -> Option<AppEvent> {
        let event = on_init_key(&mut self.init_checklist, key);
        match event {
            InitChecklistEvent::Action(action) => match action {
                InitChecklistAction::OpenDoctor => {
                    self.overlays.apply_intent(OverlayIntent::CloseTop);
                    self.sync_visibility();
                    Some(AppEvent::PaletteSelected("/doctor".to_string()))
                }
                InitChecklistAction::OpenStatus => {
                    self.overlays.apply_intent(OverlayIntent::CloseTop);
                    self.sync_visibility();
                    Some(AppEvent::PaletteSelected("/status".to_string()))
                }
                InitChecklistAction::OpenLogs => {
                    self.open_logs_browser();
                    None
                }
                InitChecklistAction::OpenSessions => {
                    self.overlays.apply_intent(OverlayIntent::CloseTop);
                    self.sync_visibility();
                    Some(AppEvent::PaletteSelected("/sessions".to_string()))
                }
                InitChecklistAction::OpenModelSwitcher => {
                    self.toggle_overlay(ShellOverlay::ModelSwitcher);
                    None
                }
                InitChecklistAction::None => None,
            },
            InitChecklistEvent::Consumed => match key {
                InitChecklistKey::Escape => {
                    self.overlays.apply_intent(OverlayIntent::CloseTop);
                    self.sync_visibility();
                    None
                }
                _ => None,
            },
        }
    }

    /// Apply an `AppEvent` against live host state. Returns
    /// `Result<(), AppError>` so callers can react to failures.
    /// Whenever an `Err` is produced, the matching error is also
    /// recorded into [`Self::activity_log`] (when attached) as a
    /// `ShellActivityKind::Error` entry — production hosts simply
    /// surface the activity stream and the operator sees the
    /// failure without bespoke error plumbing.
    pub fn apply_event(&mut self, event: AppEvent) -> Result<(), AppError> {
        let comp = match &self.composition {
            Some(c) => c.clone(),
            None => return Err(self.report_error("composition not attached", None)),
        };
        match event {
            AppEvent::ShellAction(a) => {
                let label = describe_shell_action(&a);
                if let Err(e) = comp.host.handle(a) {
                    return Err(self.report_error(
                        &format!("shell action failed: {label}"),
                        Some(e.to_string()),
                    ));
                }
                self.refresh_approval_bar();
                Ok(())
            }
            AppEvent::Session(action) => {
                if let Some(sessions) = &self.sessions {
                    let label = describe_session_action(&action);
                    if let Err(e) = sessions.apply(comp.paths.as_ref(), action.clone()) {
                        return Err(self.report_error(
                            &format!("session action failed: {label}"),
                            Some(e.to_string()),
                        ));
                    }
                    // D16 — log resume outcome to ActivityLog
                    if let SessionAction::Resume { id } = action {
                        if let Some(ref activity_log) = self.activity_log {
                            let recovery = self
                                .providers
                                .session_recovery_provider
                                .as_ref()
                                .and_then(|f| f(&id));
                            let (kind, severity, detail) =
                                match recovery.as_ref().map(|r| &r.status) {
                                    Some(vac_shell_contracts::SessionRecoveryStatus::Ready) => (
                                        vac_shell_contracts::ShellActivityKind::Status,
                                        vac_shell_contracts::Severity::Ok,
                                        Some(format!(
                                            "checkpoint ready: {}",
                                            recovery
                                                .as_ref()
                                                .and_then(|r| r.checkpoint_label.as_deref())
                                                .unwrap_or("unknown")
                                        )),
                                    ),
                                    Some(vac_shell_contracts::SessionRecoveryStatus::Missing) => (
                                        vac_shell_contracts::ShellActivityKind::Status,
                                        vac_shell_contracts::Severity::Warn,
                                        Some("checkpoint missing".to_string()),
                                    ),
                                    Some(vac_shell_contracts::SessionRecoveryStatus::Corrupt) => (
                                        vac_shell_contracts::ShellActivityKind::Error,
                                        vac_shell_contracts::Severity::Error,
                                        Some("checkpoint corrupt".to_string()),
                                    ),
                                    Some(vac_shell_contracts::SessionRecoveryStatus::Unknown)
                                    | None => (
                                        vac_shell_contracts::ShellActivityKind::Status,
                                        vac_shell_contracts::Severity::Warn,
                                        Some("checkpoint status unknown".to_string()),
                                    ),
                                };
                            let entry = vac_shell_contracts::ShellActivityEntry {
                                id: uuid::Uuid::new_v4().to_string(),
                                ts_unix: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0),
                                kind,
                                title: format!("resume requested: {id}"),
                                detail,
                                severity,
                            };
                            activity_log.record(entry);
                        }
                    }
                    Ok(())
                } else {
                    Err(self
                        .report_error("session action ignored: SessionsState not attached", None))
                }
            }
            AppEvent::DiffReview(_) => {
                // Real diff apply belongs to a host controller in a
                // later slice. For now the event is observed by the
                // host loop and applied externally.
                Ok(())
            }
            AppEvent::ApprovalDecision { id, approve } => {
                // Slice 20.2 — set the row to the requested
                // tentative decision. Submission stays explicit
                // through `ApprovalBarKey::Enter` (→
                // `ShellAction::SubmitApprovals`); the detail
                // drawer only marks rows, it does not drain the
                // queue.
                let snap = comp.approval_queue.snapshot();
                let target = if approve {
                    ApprovalStatus::Approved
                } else {
                    ApprovalStatus::Rejected
                };
                let row = snap.iter().find(|r| r.id == id);
                if row.is_none() {
                    self.refresh_approval_bar();
                    return Err(self.report_error(&format!("approval id not found: {id}"), None));
                }
                if row.unwrap().status != target {
                    let approval_ctrl: Arc<dyn ApprovalController> =
                        Arc::new(vac_shell_host_approval::ApprovalQueueController::new(
                            comp.approval_queue.clone(),
                        ));
                    if let Err(e) = approval_ctrl.toggle(&id) {
                        self.refresh_approval_bar();
                        return Err(self.report_error(
                            &format!("approval toggle failed for {id}"),
                            Some(e.to_string()),
                        ));
                    }
                }
                self.refresh_approval_bar();
                Ok(())
            }
            AppEvent::PaletteSelected(slash) => {
                // Slice 20.2 — built-in palette router. Closes the
                // palette overlay first (consistent UX for any
                // selection), then routes the slash:
                //
                //   /chat     → ShellAction::EnterSurface(Chat)
                //   /runtime  → ShellAction::EnterSurface(Runtime)
                //   /model    → open ModelSwitcher overlay
                //   /sessions → open SessionBrowser overlay
                //
                // Anything else is left to the embedding host,
                // which observes the event externally (via
                // `dispatch_palette_key`'s return value) and
                // dispatches it through whatever command pipeline
                // the host owns. This crate intentionally does not
                // know how to send a prompt template to an LLM —
                // that is product semantics.
                self.overlays.apply_intent(OverlayIntent::CloseAll);
                self.sync_visibility();
                match slash.as_str() {
                    "/chat" => {
                        if let Err(e) = comp
                            .host
                            .handle(ShellAction::EnterSurface(SurfaceTarget::Chat))
                        {
                            return Err(self.report_error(
                                "palette /chat: enter-surface failed",
                                Some(e.to_string()),
                            ));
                        }
                    }
                    "/runtime" => {
                        if let Err(e) = comp
                            .host
                            .handle(ShellAction::EnterSurface(SurfaceTarget::Runtime))
                        {
                            return Err(self.report_error(
                                "palette /runtime: enter-surface failed",
                                Some(e.to_string()),
                            ));
                        }
                    }
                    "/model" => {
                        self.model_switcher = vac_shell_host_model::build_switcher_view(
                            &comp.model_state,
                            RECENTS_LIMIT,
                        );
                        self.overlays
                            .apply_intent(OverlayIntent::Open(ShellOverlay::ModelSwitcher));
                        self.sync_visibility();
                    }
                    "/sessions" => {
                        if let (Some(comp), Some(sessions)) = (&self.composition, &self.sessions) {
                            let tool_provider = self.providers.session_tool_use_provider.clone();
                            let recovery_provider =
                                self.providers.session_recovery_provider.clone();
                            self.session_browser.tiles = sessions.list_with_tool_use_and_recovery(
                                comp.paths.as_ref(),
                                move |p| tool_provider.as_ref().and_then(|f| f(p)),
                                move |id| recovery_provider.as_ref().and_then(|f| f(id)),
                            );
                        }
                        self.overlays
                            .apply_intent(OverlayIntent::Open(ShellOverlay::SessionBrowser));
                        self.sync_visibility();
                    }
                    "/logs" => {
                        self.open_logs_browser();
                    }
                    "/init" => {
                        self.open_init_checklist();
                    }
                    _ => {
                        // Unknown / non-built-in slash — observed
                        // by the host loop via `dispatch_palette_key`;
                        // not our problem to route here.
                    }
                }
                Ok(())
            }
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let approval_bar_h = if self.approval_bar.is_visible() { 6 } else { 0 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(approval_bar_h),
                Constraint::Length(1),
            ])
            .split(area);

        // Body — the activity stream is always present.
        vac_shell_activity::render_activity(f, &self.activity, chunks[0]);

        // Approval bar — visible whenever the queue has rows.
        if approval_bar_h > 0 {
            vac_shell_approval_bar::render_approval_bar(f, &self.approval_bar, chunks[1]);
        }

        // Bottom status bar.
        if let Some(comp) = &self.composition {
            let view = project_status(comp, &self.status_inputs);
            render_status_bar(f, &view, chunks[2]);
        }

        // Overlays — render the active one in a centred rect.
        let overlay_area = centered_rect(80, 70, area);
        match self.overlays.top() {
            ShellOverlay::None => {}
            ShellOverlay::Palette => {
                vac_shell_palette::render_palette(f, &self.palette, overlay_area);
            }
            ShellOverlay::Shortcuts => {
                vac_shell_shortcuts::render_shortcuts_popup(f, &self.shortcuts, overlay_area);
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
                vac_shell_diff_view::render_diff_review(f, &self.diff_review, overlay_area);
            }
            ShellOverlay::ShellPopup => {
                vac_shell_popup::render_shell_popup(f, &self.shell_popup, overlay_area);
            }
            ShellOverlay::ApprovalDetail => {
                vac_shell_approval_detail::render_approval_detail(
                    f,
                    &self.approval_detail,
                    overlay_area,
                );
            }
            ShellOverlay::Logs => {
                vac_shell_activity::render_logs_browser(f, &self.logs_browser, overlay_area);
            }
            ShellOverlay::Init => {
                render_init_checklist(f, &self.init_checklist, overlay_area);
            }
        }
    }
}

/// Slice 20.3 — production error reporting type. Returned by
/// `apply_event` and recorded into the activity log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    pub title: String,
    pub detail: Option<String>,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.detail {
            Some(d) => write!(f, "{}: {}", self.title, d),
            None => write!(f, "{}", self.title),
        }
    }
}

impl std::error::Error for AppError {}

impl ShellApp {
    /// Build the error, push it into the activity log if attached,
    /// and return it. Internal helper so every failure path goes
    /// through the same place.
    fn report_error(&self, title: &str, detail: Option<String>) -> AppError {
        let err = AppError {
            title: title.to_string(),
            detail: detail.clone(),
        };
        if let Some(log) = &self.activity_log {
            let id = format!("err-{}", now_unix());
            log.record_error(id, now_unix(), title, detail);
        }
        err
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn describe_shell_action(a: &ShellAction) -> String {
    match a {
        ShellAction::EnterSurface(SurfaceTarget::Chat) => "EnterSurface(Chat)".into(),
        ShellAction::EnterSurface(SurfaceTarget::Runtime) => "EnterSurface(Runtime)".into(),
        ShellAction::ToggleApproval { id } => format!("ToggleApproval({id})"),
        ShellAction::RejectAllApprovals => "RejectAllApprovals".into(),
        ShellAction::SubmitApprovals => "SubmitApprovals".into(),
        ShellAction::SelectModel { provider, id } => {
            format!("SelectModel({}/{id})", provider.0)
        }
    }
}

fn describe_session_action(a: &vac_shell_contracts::SessionAction) -> String {
    use vac_shell_contracts::SessionAction;
    match a {
        SessionAction::Open { id } => format!("Open({id})"),
        SessionAction::Resume { id } => format!("Resume({id})"),
        SessionAction::Archive { id } => format!("Archive({id})"),
        SessionAction::Delete { id } => format!("Delete({id})"),
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
    OpenShellPopup,
    OpenApprovalDetail,
    EnterRuntime,
    EnterChat,
    Escape,
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let h = area.height.saturating_mul(percent_y) / 100;
    let w = area.width.saturating_mul(percent_x) / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}
