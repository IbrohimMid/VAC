import re

with open('/workspace/crates/vac_tui_runtime/src/app/types/mod.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Define the new struct definitions
new_structs = """
#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
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
}
"""

# Replace the AppState definition
pattern = re.compile(r'pub struct AppState \{.*?^\}', re.MULTILINE | re.DOTALL)
content = pattern.sub(new_structs.strip(), content)

with open('/workspace/crates/vac_tui_runtime/src/app/types/mod.rs', 'w', encoding='utf-8') as f:
    f.write(content)
