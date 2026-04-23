//! Type Definitions Module

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use crate::services::Toast;
use crate::services::textarea::TextArea;

// Re-export all submodule types
pub mod approvals;
pub mod ask_user;
pub mod at_mention;
pub mod banner;
pub mod billing;
pub mod bridge;
pub mod changeset_ui;
pub mod command_palette;
pub mod file_index;
pub mod file_picker;
pub mod lsp_ui;
pub mod scroll;
pub mod session_meta;
pub mod message_ui;
pub mod operator;
pub mod paste;
pub mod pins;
pub mod quit;
pub mod session_resume;
pub mod side_panel;
pub mod streaming;
pub mod task_tray;
pub mod switchers;
pub mod view_flags;
pub mod vil_dev;
pub mod workbench_ui;
pub mod commands;
pub mod helpers;
pub mod image_render;
pub mod mcp_maps;
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
pub use bridge::BridgeState;
pub use changeset_ui::ChangesetUiState;
pub use command_palette::CommandPaletteState;
pub use file_index::FileIndexState;
pub use file_picker::FilePickerState;
pub use lsp_ui::LspUiState;
pub use scroll::ScrollState;
pub use session_meta::SessionMetaState;
pub use message_ui::MessageUiState;
pub use operator::OperatorState;
pub use paste::PasteState;
pub use pins::PinsState;
pub use quit::QuitState;
pub use session_resume::SessionResumeState;
pub use side_panel::SidePanelState;
pub use streaming::StreamingState;
pub use task_tray::TaskTrayState;
pub use switchers::SwitchersState;
pub use view_flags::ViewFlagsState;
pub use vil_dev::VilDevState;
pub use workbench_ui::WorkbenchChromeState;
pub use billing::{
    BillingInfo, BillingState, LoadingOperation, LoadingStateManager, SessionInfo,
    ShortcutsPopupMode, TokenUsage, ToolCallStatus,
};
pub use image_render::ImageRenderState;
pub use mcp_maps::McpMapsState;
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

/// Main application state for TUI.
///
/// **Field-grouping discipline (F3.4).** AppState is still transitional:
/// ~30 sub-structs (defined under `app/types/`, e.g. `operator`,
/// `session_meta`, `bridge`, `approvals`, `shell`, `streaming`,
/// `runtime`, `vil`, `vil_dev`, `switchers`, `review`, `plan`,
/// `file_picker`, `command_palette`, `lsp_ui`, `workbench_chrome`,
/// `session_resume`, `task_tray`, `message_ui`, `ask_user`, `banner`,
/// `paste`, `pins`, `changeset_ui`, `at_mention`, `side_panel`,
/// `quit`, `scroll`, `view_flags`, `file_index`) coexist with ~40
/// remaining flat fields that still need regrouping (billing/usage,
/// mcp/signal maps, kitty/image caches, theme, etc.).
///
/// Rule going forward: **any new domain state lands as a sub-struct
/// under `app/types/`, not as a new flat field here.** Flat fields
/// above this line are grandfathered pending a follow-up Fase.
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
    pub focus: WorkspaceFocus,

    // Messages
    pub messages: Vec<Message>,

    /// Grouped scroll offsets + cursor position.
    pub scroll: ScrollState,

    // Loading state
    pub loading: bool,
    pub loading_manager: LoadingStateManager,
    pub view_flags: ViewFlagsState,

    // Session state
    pub session_id: String,
    pub sessions: Vec<SessionInfo>,
    /// Grouped session metadata (title, checkpoint path, load flag).
    pub session_meta: SessionMetaState,

    /// Operator-facing cursor/selection state (current model,
    /// selected indices across overlays, open message-action popup).
    pub operator: OperatorState,

    // Mouse capture

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

    pub changeset_store: vac_changeset::ChangesetStore,
    pub modified_files: Vec<String>,

    pub workbench_tab: WorkbenchTab,
    // Git-review domain state
    pub review: ReviewState,

    // Runtime / agent-scheduler domain state
    pub runtime: RuntimeState,

    pub activity: Vec<ActivityItem>,

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
    pub project_root: PathBuf,

    // MCP
    /// R1.b — grouped MCP + runtime signal maps.
    pub mcp_maps: McpMapsState,
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
    /// R1.c — Kitty-graphics render state: pending emission, dedup
    /// cache, off-path preview cache. See `image_render.rs`.
    pub image_render: ImageRenderState,

    // Paste ledger (long text + image tray)
    pub paste: PasteState,

    // File changes popup (compact, searchable)

    // Todos extracted from <todo>…</todo> blocks in assistant messages
    pub todos: Vec<vac_changeset::TodoItem>,

    /// R1.a — token usage, context pressure, billing plan, identity.
    pub billing: BillingState,

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

    // ── Session Resume Overlay (PR-T8) ───────────────────────────────────────
    pub session_resume: SessionResumeState,

    // vil_workbench fields moved to VilState.workbench_selected / .workbench_group_filter

    // ===== Unit 5 (Wave 3.1) — Attachment tray preview & reorder =====

    // ===== Context Composer (Wave 3) =====
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

    /// Remote bridge session state — populated by `vac_bridge` in
    /// Fase 5. Present as a stable slot now so drivers can bind.
    pub bridge: BridgeState,
}
