//! Input and Output Events

use uuid::Uuid;

use crate::tui::app::{LoadingOperation, SessionInfo};
use crate::tui::services::Toast;
use crate::tui::types::*;

#[derive(Debug)]
pub enum InputEvent {
    // Backend events
    AssistantMessage(String),
    AddUserMessage(String),
    StreamAssistantMessage(Uuid, String),
    RunToolCall(ToolCall),
    ToolResult(ToolCallResult),
    StreamToolResult(ToolCallResultProgress),
    StreamToolCallProgress(Vec<ToolCallStreamInfo>),
    StartLoadingOperation(LoadingOperation),
    EndLoadingOperation(LoadingOperation),
    Error(String),
    SetCurrentModel(Model),
    AvailableModelsLoaded(Vec<Model>),
    ShowToast(Toast),
    SetSessions(Vec<SessionInfo>),
    SetRuntimeJobs(Vec<vac_runtime::Job>),
    SetRuntimeState(Option<vac_runtime::AutopilotStateFile>),
    FileIndexReady(Vec<String>),
    McpConnected { name: String, tools: usize },
    McpFailed { name: String, error: String },
    McpServerState(String, vac_tools::mcp::McpConnectionState),
    VilStatusUpdated(crate::tui::app::VilStatusSnapshot),
    ChangesetUpdated,
    IsolationBoundary { action: String, environment: String },
    // Shell events
    ShellStarted(crate::tui::services::ShellCommand),
    ShellOutput(String, String), // command_id, text
    ShellError(String, String), // command_id, text
    ShellCompleted(String, i32), // command_id, code
    ShellWaitingForInput(String), // command_id
    SessionRestored {
        id: String,
        title: String,
        messages: Vec<crate::tui::app::Message>,
    },

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
    ToggleCollapsedMessages,
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
}

impl InputEvent {
    pub fn is_backend_event(&self) -> bool {
        matches!(
            self,
            InputEvent::StreamAssistantMessage(_, _)
                | InputEvent::AssistantMessage(_)
                | InputEvent::StartLoadingOperation(_)
                | InputEvent::EndLoadingOperation(_)
                | InputEvent::StreamToolResult(_)
                | InputEvent::StreamToolCallProgress(_)
                | InputEvent::Error(_)
                | InputEvent::RunToolCall(_)
                | InputEvent::ToolResult(_)
                | InputEvent::SetCurrentModel(_)
                | InputEvent::AvailableModelsLoaded(_)
                | InputEvent::ShowToast(_)
                | InputEvent::SetSessions(_)
                | InputEvent::SetRuntimeJobs(_)
                | InputEvent::SetRuntimeState(_)
                | InputEvent::FileIndexReady(_)
                | InputEvent::ShellStarted(_)
                | InputEvent::ShellOutput(_, _)
                | InputEvent::ShellError(_, _)
                | InputEvent::ShellCompleted(_, _)
                | InputEvent::ShellWaitingForInput(_)
                | InputEvent::McpServerState(_, _)
                | InputEvent::SessionRestored { .. }
                | InputEvent::AddUserMessage(_)
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
}
