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
pub mod team;
pub mod speculation;
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
pub use team::TeamContext;
pub use speculation::SpeculationCache;
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
#[derive(Debug)]
pub struct CoreState {
    pub hydrated: bool,
    pub hydration_deadline: std::time::Instant,
    pub loading: bool,
    pub loading_manager: LoadingStateManager,
    pub view_flags: ViewFlagsState,
    pub quit: QuitState,
    pub input_tx: Option<tokio::sync::mpsc::Sender<crate::app::events::InputEvent>>,
    pub project_root: PathBuf,
    pub theme: crate::services::theme::Theme,
    pub render_metrics: RenderMetrics,
    pub startup: StartupSnapshot,
}

impl Default for CoreState {
    fn default() -> Self {
        Self {
            hydrated: false,
            hydration_deadline: std::time::Instant::now(),
            loading: false,
            loading_manager: LoadingStateManager::new(),
            view_flags: ViewFlagsState::default(),
            quit: QuitState::default(),
            input_tx: None,
            project_root: PathBuf::new(),
            theme: crate::services::theme::Theme::default(),
            render_metrics: RenderMetrics::default(),
            startup: StartupSnapshot::default(),
        }
    }
}

#[derive(Debug)]
pub struct LayoutState {
    pub side_panel: SidePanelState,
    pub focus: WorkspaceFocus,
    pub scroll: ScrollState,
    pub workbench_tab: WorkbenchTab,
    pub workbench_chrome: WorkbenchChromeState,
    pub command_palette: CommandPaletteState,
    pub commands: Vec<HelperCommand>,
    pub banner: BannerState,
    pub toasts: Vec<Toast>,
    pub image_render: ImageRenderState,
    pub paste: PasteState,
    pub message_ui: MessageUiState,
    pub overlay_manager: crate::overlay::OverlayManager,
    pub lsp_ui: LspUiState,
    pub pins: PinsState,
    pub switchers: SwitchersState,
    pub session_resume: SessionResumeState,
    pub ask_user: AskUserState,
    /// G1 — parked MCP elicitation prompt. `None` when no overlay
    /// is active. The TuiElicitationHandler inserts here under a
    /// lock, then awaits a oneshot the overlay's input handler
    /// resolves.
    pub elicitation: Option<crate::services::elicitation::ElicitationPrompt>,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            side_panel: SidePanelState::default(),
            focus: WorkspaceFocus::Input,
            scroll: ScrollState::default(),
            workbench_tab: WorkbenchTab::Approvals,
            workbench_chrome: WorkbenchChromeState::default(),
            command_palette: CommandPaletteState::default(),
            commands: Vec::new(),
            banner: BannerState::default(),
            toasts: Vec::new(),
            image_render: ImageRenderState::default(),
            paste: PasteState::default(),
            message_ui: MessageUiState::default(),
            overlay_manager: crate::overlay::OverlayManager::new(),
            lsp_ui: LspUiState::default(),
            pins: PinsState::default(),
            switchers: SwitchersState::default(),
            session_resume: SessionResumeState::default(),
            ask_user: AskUserState::default(),
            elicitation: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct ComposerState {
    pub input: TextArea,
    pub context_chips: Vec<crate::app::types::ContextChip>,
    pub context_chip_cursor: Option<usize>,
    pub at_mention: AtMentionState,
    pub pending_image_parts: Vec<crate::types::ContentPart>,
    pub prompt_history: crate::services::prompt_suggest::PromptHistory,
    pub vil_expr_lint: crate::services::vil_expr_lint::LintState,
    pub selection_state: crate::services::text_selection::SelectionState,
}

#[derive(Debug, Default)]
pub struct TranscriptState {
    pub messages: Vec<Message>,
    pub pending_user_messages: std::collections::VecDeque<PendingUserMessage>,
    pub todos: Vec<vac_changeset::TodoItem>,
    pub streaming: StreamingState,
    pub rate_limit: crate::services::rate_limit::RateLimitState,
}

#[derive(Debug, Default)]
pub struct SessionDomainState {
    pub session_id: String,
    pub sessions: Vec<SessionInfo>,
    pub session_meta: SessionMetaState,
}

#[derive(Debug, Default)]
pub struct WorkspaceState {
    pub file_index: FileIndexState,
    pub file_picker: FilePickerState,
    pub modified_files: Vec<String>,
    pub changeset_store: vac_changeset::ChangesetStore,
    pub changeset_ui: ChangesetUiState,
    pub review: ReviewState,
    pub plan: PlanState,
}

#[derive(Debug, Default)]
pub struct VilDomainState {
    pub vil: VilState,
    pub vil_dev: VilDevState,
    pub vwfd_inspector: crate::services::vwfd_inspector::VwfdInspectorState,
}

#[derive(Debug, Default)]
pub struct ExecutionState {
    pub shell: ShellState,
    pub runtime: RuntimeState,
    pub bridge: BridgeState,
    pub mcp_maps: McpMapsState,
    pub approvals: ApprovalsState,
    pub task_tray: TaskTrayState,
    pub queue_metrics: QueueMetrics,
    pub activity: Vec<ActivityItem>,
    /// C1 — latest PolicyTracker snapshot (submits used + tokens
    /// consumed + loaded caps). `None` until a tracker is attached.
    /// Read by `SystemPulse::policy_facet`; refreshed by the idle
    /// tick that owns the tracker Arc.
    pub policy: Option<vac_core::policy_limits::PolicySnapshot>,
}

#[derive(Debug, Default)]
pub struct OperatorConfigState {
    pub operator: OperatorState,
    pub billing: BillingState,
}

pub struct AppState {
    pub core: CoreState,
    pub layout: LayoutState,
    pub composer: ComposerState,
    pub transcript: TranscriptState,
    pub session: SessionDomainState,
    pub workspace: WorkspaceState,
    pub vil_domain: VilDomainState,
    pub execution: ExecutionState,
    pub operator_config: OperatorConfigState,
    pub team: TeamContext,
    pub speculation: SpeculationCache,
}
