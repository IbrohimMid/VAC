//! Input and Output Events

use uuid::Uuid;

use crate::app::{LoadingOperation, SessionInfo};
use crate::services::Toast;
use crate::types::*;
use std::path::PathBuf;

#[derive(Debug)]
pub enum InputEvent {
    // Backend events
    AssistantMessage(String),
    AddUserMessage(String),
    StreamAssistantMessage(Uuid, String),
    RunToolCall(ToolCall),
    ToolResult(ToolCallResult),
    StartLoadingOperation(LoadingOperation),
    EndLoadingOperation(LoadingOperation),
    Error(String),
    SetCurrentModel(Model),
    AvailableModelsLoaded(Vec<Model>),
    ShowToast(Toast),
    SetSessions(Vec<SessionInfo>),
    SetSessionResumeList(Vec<crate::app::types::SessionResumeEntry>),
    SetAgentTasks(Vec<vac_runtime::AgentTask>),
    SetAgentState(Option<vac_runtime::AgentSchedulerStateFile>),
    SetRuntimeJobs(Vec<vac_runtime::Job>),
    SetRuntimeState(Option<vac_runtime::AutopilotStateFile>),
    SetTaskGraphProjection(Option<vac_core::engine::TaskGraphProjection>),
    FileIndexReady(Vec<String>),
    McpConnected {
        name: String,
        tools: usize,
    },
    McpFailed {
        name: String,
        error: String,
    },
    McpServerState(String, vac_tools::mcp::McpConnectionState),
    ShowBanner(
        String,
        crate::services::banner::BannerStyle,
        crate::services::banner::BannerSeverity,
    ),
    VilStatusUpdated(crate::app::VilStatusSnapshot),
    ValidationResult(f64, Vec<String>),
    LspStatus(bool, String),
    LspDiagnostics(vac_core::lsp::types::LspWorkspaceSnapshot),
    TaskCancelled,
    ChangesetUpdated,
    IsolationBoundary {
        action: String,
        environment: String,
    },
    StartupHydrated(crate::app::StartupSnapshot),
    // Shell events
    ShellStarted(vac_shell::ShellCommand),
    ShellOutput(String, String),  // command_id, text
    ShellError(String, String),   // command_id, text
    ShellCompleted(String, i32),  // command_id, code
    ShellWaitingForInput(String), // command_id
    SessionRestored {
        id: String,
        title: String,
        messages: Vec<crate::app::Message>,
    },

    /// Hot-reload dari theme_loader watcher — ganti theme aktif.
    ThemeReloaded(crate::services::theme::Theme),

    // Input events
    InputChanged(char),
    InputBackspace,
    InputDelete,
    InputClear,
    InputDeleteWord,
    InputChangedNewline,
    InputSubmitted,
    HandlePaste(String),
    HandleClipboardImagePaste,
    InputCursorStart,
    InputCursorEnd,
    InputCursorPrevWord,
    InputCursorNextWord,
    CursorLeft,
    CursorRight,

    // Navigation
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    Up,
    Down,
    Tab,
    WorkbenchNextTab,

    // Control
    Quit,
    AttemptQuit,
    HandleEsc,
    Resized(u16, u16),
    ToggleMouseCapture,

    // Dialog/Approval
    ShowConfirmationDialog(ToolCall),
    ShowConfirmationDialogWithExplanation(ToolCall, Option<String>),
    RejectCurrentTool,
    ApproveAll,
    RejectAll,

    // Popups
    ShowModelSwitcher,
    ShowFileSearch,
    ShowChangeset,

    // Command palette
    ShowCommandPalette,
    HideCommandPalette,
    CommandPaletteInput(char),
    CommandPaletteBackspace,
    CommandPaletteUp,
    CommandPaletteDown,
    CommandPaletteSelect,

    // Shortcuts
    ShowShortcuts,
    HideShortcuts,

    // Session
    RequestSessionList,
    NewSession,

    // Additional events from event.rs mapping
    ShowRulebookSwitcher,
    RetryLastToolCall,
    HandleCtrlS,
    ReviewOpen,
    ReviewClose,
    ReviewUp,
    ReviewDown,
    ReviewFilterInput(char),
    ReviewFilterBackspace,
    ReviewToggleDiff,
    ReviewRevertSelected,
    ReviewRevertFiltered,
    ReviewRevertAll,
    ReviewOpenEditor,
    RulebookSwitcherDeselectAll,
    ToggleAutoApprove,
    ToggleSidePanel,
    AutoApproveCurrentTool,
    ShowIsolationSwitcher,
    ShowProfileSwitcher,
    ShowMessageActionPopup,
    HandleCtrlZ,
    BackgroundShell,
    FocusShell,
    ShellKill,
    MouseDragStart(u16, u16),
    MouseDrag(u16, u16),
    MouseDragEnd(u16, u16),
    MouseMove(u16, u16),
    MouseRightClick(u16, u16),
    TaskCompleted(vac_core::task::TaskResult),

    // Raw Crossterm event mapped dynamically
    CrosstermEvent(crossterm::event::Event),
}

impl InputEvent {
    pub fn is_backend_event(&self) -> bool {
        matches!(
            self,
            InputEvent::StreamAssistantMessage(_, _)
                | InputEvent::AssistantMessage(_)
                | InputEvent::AddUserMessage(_)
                | InputEvent::StartLoadingOperation(_)
                | InputEvent::EndLoadingOperation(_)
                | InputEvent::Error(_)
                | InputEvent::RunToolCall(_)
                | InputEvent::ToolResult(_)
                | InputEvent::SetCurrentModel(_)
                | InputEvent::AvailableModelsLoaded(_)
                | InputEvent::ShowToast(_)
                | InputEvent::SetSessions(_)
                | InputEvent::SetAgentTasks(_)
                | InputEvent::SetAgentState(_)
                | InputEvent::SetRuntimeJobs(_)
                | InputEvent::SetRuntimeState(_)
                | InputEvent::SetTaskGraphProjection(_)
                | InputEvent::FileIndexReady(_)
                | InputEvent::ShellStarted(_)
                | InputEvent::ShellOutput(_, _)
                | InputEvent::ShellError(_, _)
                | InputEvent::ShellCompleted(_, _)
                | InputEvent::ShellWaitingForInput(_)
                | InputEvent::McpServerState(_, _)
                | InputEvent::ShowBanner(_, _, _)
                | InputEvent::SessionRestored { .. }
                | InputEvent::ValidationResult(_, _)
                | InputEvent::LspStatus(_, _)
                | InputEvent::LspDiagnostics(_)
                | InputEvent::TaskCancelled
        )
    }
}

#[derive(Debug)]
pub enum OutputEvent {
    UserMessage(
        String,
        Option<Vec<ToolCallResult>>,
        Vec<ContentPart>,
        Option<usize>,
    ),
    AcceptTool(ToolCall),
    RejectTool(ToolCall, bool, Option<String>),
    SwitchToModel(Model),
    ListSessions,
    ListAgentTasks,
    LoadAgentState,
    ListRuntimeJobs,
    LoadRuntimeState,
    CancelRuntimeJob(uuid::Uuid),
    RetryRuntimeJob(uuid::Uuid),
    SwitchToSession(String),
    NewSession,
    ResumeSession(String),
    SendToolResult(ToolCallResult, bool, Vec<ToolCall>),
    CancelStream,
    SwitchProfile(String),
    ApplyRulebooks(Vec<String>),
    /// Direct tool invocation from TUI popup (tool_name, args).
    /// Result is displayed as an assistant message.
    InvokeVilTool(String, serde_json::Value),
    ExecuteCommand(String, String), // command, active_isolation_mode
    RetryMessage(uuid::Uuid),
    RevertToMessage(uuid::Uuid),
    ExportBundle(PathBuf),
    ImportBundle(PathBuf),
    /// Files selected via file picker v2 and confirmed (PR-T6).
    FilesAttached(Vec<PathBuf>),
    /// Load session list for the resume overlay (PR-T8).
    LoadSessionResumeList,
}
