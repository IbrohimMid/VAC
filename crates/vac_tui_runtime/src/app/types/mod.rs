//! Type Definitions Module

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use uuid::Uuid;

use crate::services::Toast;
use crate::services::textarea::TextArea;
use crate::types::*;

// Re-export all submodule types
pub mod approvals;
pub mod ask_user;
pub mod at_mention;
pub mod banner;
pub mod billing;
pub mod changeset_ui;
pub mod command_palette;
pub mod file_index;
pub mod file_picker;
pub mod lsp_ui;
pub mod message_ui;
pub mod paste;
pub mod pins;
pub mod quit;
pub mod session_resume;
pub mod side_panel;
pub mod streaming;
pub mod task_tray;
pub mod switchers;
pub mod vil_dev;
pub mod workbench_ui;
pub mod commands;
pub mod helpers;
pub mod messages;
pub mod rendering;
pub mod runtime;
pub mod shell;
pub mod support;
pub mod workbench;

pub use approvals::ApprovalsState;
pub use ask_user::AskUserState;
pub use at_mention::AtMentionState;
pub use banner::BannerState;
pub use changeset_ui::ChangesetUiState;
pub use command_palette::CommandPaletteState;
pub use file_index::FileIndexState;
pub use file_picker::FilePickerState;
pub use lsp_ui::LspUiState;
pub use message_ui::MessageUiState;
pub use paste::PasteState;
pub use pins::PinsState;
pub use quit::QuitState;
pub use session_resume::SessionResumeState;
pub use side_panel::SidePanelState;
pub use streaming::StreamingState;
pub use task_tray::TaskTrayState;
pub use switchers::SwitchersState;
pub use vil_dev::VilDevState;
pub use workbench_ui::WorkbenchChromeState;
pub use billing::{
    BillingInfo, LoadingOperation, LoadingStateManager, SessionInfo, ShortcutsPopupMode,
    TokenUsage, ToolCallStatus,
};
pub use commands::{
    CommandSource, ExistingPlanPrompt, HelperCommand, PendingUserMessage, PlanComment,
};
pub use helpers::AppStateOptions;
pub use messages::Message;
pub use rendering::{
    MessageLinesCache, PerMessageCache, QueueMetrics, RenderMetrics, RenderedMessageCache,
    VisibleLinesCache,
};
pub use runtime::{
    ActivityItem, ActivityKind, RuntimeState, VilIssue, VilIssueKind, VilLogEntry, VilSeverity,
    VilState, VilStatusSnapshot,
};
pub use shell::{ShellSession, ShellSessionStore, ShellState};
pub use support::{
    ChipNamespace, ContextChip, SessionResumeEntry, SidePanelRowAction, SidePanelSection,
    StartupSnapshot,
};
pub use workbench::{
    PlanState, ReviewDiffState, ReviewItem, ReviewItemStatus, ReviewState, WorkbenchTab,
    WorkspaceFocus,
};

/// Main application state for TUI
pub struct AppState {
    pub startup: StartupSnapshot,
    /// True once StartupHydrated has been received — gates first real render.
    pub hydrated: bool,
    /// Deadline after which a missing StartupHydrated forces hydration with fallback data.
    pub hydration_deadline: std::time::Instant,
    // Layout state
    pub side_panel: SidePanelState,

    // Input state
    pub input: TextArea,
    pub cursor_position: usize,
    pub focus: WorkspaceFocus,

    // Messages
    pub messages: Vec<Message>,
    pub scroll: usize,

    // Loading state
    pub loading: bool,
    pub loading_manager: LoadingStateManager,
    pub spinner_frame: usize,

    // Session state
    pub session_id: String,
    pub sessions: Vec<SessionInfo>,
    pub session_title: Option<String>,
    pub checkpoint_path: Option<PathBuf>,

    // Model state
    pub current_model: Option<Model>,

    // Mouse capture
    pub mouse_capture_enabled: bool,

    // Approval state — see ApprovalsState for field details
    pub approvals: ApprovalsState,

    // Shell domain state
    pub shell: ShellState,

    // Streaming state
    pub streaming: StreamingState,
    pub quit: QuitState,

    // Command palette
    pub command_palette: CommandPaletteState,
    pub commands: Vec<HelperCommand>,

    // Helper Dropdown

    // Shortcuts popup

    // Switchers (isolation, profile, rulebook, model)
    pub switchers: SwitchersState,

    // Message Action Popup
    pub message_action_popup_selected: usize,
    pub message_action_target_id: Option<Uuid>,

    pub changeset_store: vac_changeset::ChangesetStore,
    pub modified_files: Vec<String>,

    pub workbench_tab: WorkbenchTab,
    // Git-review domain state
    pub review: ReviewState,

    pub sessions_selected_idx: usize,
    // Runtime / agent-scheduler domain state
    pub runtime: RuntimeState,

    pub activity: Vec<ActivityItem>,
    pub activity_scroll: usize,

    pub toasts: Vec<Toast>,


    pub file_index: FileIndexState,

    // ── File Picker v2 (PR-T6) ────────────────────────────────────────────────
    pub file_picker: FilePickerState,

    // ── @-mention context chips (PR-T7) ────────────────────────────────────────
    /// Resolved context chips above the input bar.
    pub context_chips: Vec<crate::app::types::ContextChip>,
    pub context_chip_cursor: Option<usize>,

    // Inline @ file picker
    pub at_mention: AtMentionState,

    pub changeset_ui: ChangesetUiState,

    // Permission UX
    pub auto_approve: bool,
    pub project_root: PathBuf,

    // MCP
    pub mcp_server_states: HashMap<String, vac_tools::mcp::McpConnectionState>,

    // VIL domain state
    pub vil: VilState,

    // T14: vil dev runner state grouped into VilDevState
    pub vil_dev: VilDevState,

    // VWFD inspector state (PR-T11). Default is an empty inspector; loaded
    // lazily when the user opens a .vwfd.yaml via the changeset or commands.
    pub vwfd_inspector: crate::services::vwfd_inspector::VwfdInspectorState,

    // ── vil-expr live linter (PR-T12) ─────────────────────────────────────────
    /// Debounced parser + validator for `vil-expr:` payloads typed into the
    /// input bar. Updated by the input handler on every keystroke and ticked
    /// by the event loop. View layer reads `lint_state.issues()` to render
    /// diagnostics in the composer overlay.
    pub vil_expr_lint: crate::services::vil_expr_lint::LintState,

    // Pending image attachments for next message submission
    pub pending_image_parts: Vec<crate::types::ContentPart>,

    // Banner (top-strip notices / CTAs)
    pub banner: BannerState,
    // PR-T16 — mouse click regions for workbench tabs and task tray rows.
    // Populated during view render, consumed by `handlers::mouse::dispatch_click`.
    pub workbench_chrome: WorkbenchChromeState,
    // PR-T16 P1 — per-surface click regions (review file list, approvals rows,
    // VIL issue rows, generic workbench body focus grab). Populated each render;
    // consumed by `handlers::mouse::dispatch_click`.
    /// PR-T17 / R8c — pending native Kitty graphics emission for the current
    /// frame. Populated by surface renderers (e.g. the review-tab image
    /// preview) when the probed terminal supports Kitty graphics
    /// (`startup.kitty_graphics == true`). The event loop flushes this
    /// payload to stdout *after* `terminal.draw()` completes, positioning the
    /// cursor at the target rect so the image overlays the ratatui
    /// placeholder. Ratatui itself never sees these bytes. Cleared every
    /// frame by the flush step so stale sequences cannot survive a tab
    /// switch.
    pub pending_kitty_emission: Option<(ratatui::layout::Rect, Vec<u8>)>,
    /// PR-T17 M3/L5 — dedup cache for the post-frame Kitty flush. Stores
    /// the `(rect, content_hash)` of the most recently emitted image. When
    /// the next frame queues an identical `(rect, hash)` the flush step
    /// skips the DCS write entirely, because Kitty graphics persist on the
    /// terminal's graphics plane until the cells are reused. This avoids
    /// re-transmitting megabytes of base64 per frame during an idle
    /// preview. Reset to `None` whenever a frame has no pending emission
    /// (e.g. tab switched away, preview dismissed) so that re-entering the
    /// preview always forces a fresh emission.
    pub last_kitty_emission: Option<(ratatui::layout::Rect, u64)>,
    /// PR-T17 / M1 — off-render-path cache for image previews. The render
    /// loop used to call `review_preview::prepare_image_preview` directly
    /// inside `terminal.draw`, which meant a blocking disk read + PNG
    /// header decode stalled the tokio runtime for the full duration of
    /// the preview load. The cache now owns the blocking work: the render
    /// path only reads cache state (Loading / Ready), and a short-lived OS
    /// thread delivers the result through an internal channel that the
    /// event loop drains once per iteration before the next draw.
    pub image_preview_cache: crate::services::image_preview_cache::ImagePreviewCache,

    // Paste ledger (long text + image tray)
    pub paste: PasteState,

    // File changes popup (compact, searchable)

    // Todos extracted from <todo>…</todo> blocks in assistant messages
    pub todos: Vec<vac_changeset::TodoItem>,

    // Token usage + context pressure + identity
    pub current_message_usage: TokenUsage,
    pub total_session_usage: TokenUsage,
    pub context_usage_percent: f32,
    pub billing_info: Option<BillingInfo>,
    pub auth_display_info: (Option<String>, Option<String>, Option<String>),

    // Revert anchors: line_to_message_map is populated during render and
    // consumed by message_at_row. pending_revert_index stages a confirmation
    // step for future two-step revert UX.
    // Plan domain state
    pub plan: PlanState,

    // Ask-User popup (triggered by `ask_user` tool call)
    pub ask_user: AskUserState,

    // Text Selection
    pub selection_state: crate::services::text_selection::SelectionState,
    pub message_ui: MessageUiState,
    pub render_metrics: RenderMetrics,

    pub input_tx: Option<tokio::sync::mpsc::Sender<crate::app::events::InputEvent>>,

    /// Overlay stack: tracks open modal overlays and their event-capture order.
    pub overlay_manager: crate::overlay::OverlayManager,

    // ── Task Tray (PR-T9) ────────────────────────────────────────────────────
    pub task_tray: TaskTrayState,

    // ── Theme (PR-T5) ────────────────────────────────────────────────────────
    pub theme: crate::services::theme::Theme,
    pub theme_picker_selected: usize,

    // ── Session Resume Overlay (PR-T8) ───────────────────────────────────────
    pub session_resume: SessionResumeState,

    // vil_workbench fields moved to VilState.workbench_selected / .workbench_group_filter

    // ===== Unit 5 (Wave 3.1) — Attachment tray preview & reorder =====

    // ===== Context Composer (Wave 3) =====
    pub context_composer_visible: bool,
    pub lsp_ui: LspUiState,
    /// PR-T15 P1 — per-file overlay cache for inline diagnostics squiggles.
    /// Renderers clear it when the focused file changes to avoid stale spans
    /// leaking between files (see `services::diagnostics_overlay`).
    /// R7 / PR-T15 — currently-displayed hover popup detail. Populated by
    /// `handlers::mouse::dispatch_click` when a VIL issue row (or other
    /// diagnostic-bearing row) is clicked; cleared on outside-click dismiss.
    /// `None` means no popup is on screen.
    // active_hover moved into LspUiState.
    /// R7 / PR-T15 — screen rect of the hover popup, recorded during the
    /// render pass. Used by `dispatch_click` to detect outside-popup clicks
    /// so a subsequent click can dismiss the popup before falling through
    /// to row/body hit-testing.
    pub pins: PinsState,

    pub pending_user_messages: VecDeque<PendingUserMessage>,

    /// Queue metrics for I/O reliability tracking (Phase 4).
    pub queue_metrics: QueueMetrics,
}
