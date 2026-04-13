//! Input and Output Events

use ratatui::style::Color;
use uuid::Uuid;

use crate::tui::stub_types::*;
use crate::tui::app::{LoadingOperation, SessionInfo};

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
    SetSessions(Vec<SessionInfo>),
    SessionRestored { id: String, title: String, messages: Vec<crate::tui::app::Message> },
    
    // Input events
    InputChanged(char),
    InputBackspace,
    InputDelete,
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
    
    // Control
    Quit,
    AttemptQuit,
    HandleEsc,
    Resized(u16, u16),
    ToggleMouseCapture,
    
    // Dialog/Approval
    ShowConfirmationDialog(ToolCall),
    DialogUp,
    DialogDown,
    DialogSelect,
    DialogCancel,
    ToggleApprovalStatus,
    ApproveTool,
    RejectTool,
    
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
    SwitchToSession(String),
    NewSession,
    
    // Additional events from event.rs mapping
    ShowRulebookSwitcher,
    RetryLastToolCall,
    ToggleCollapsedMessages,
    HandleCtrlS,
    ShowFileChangesPopup,
    FileChangesRevertFile,
    FileChangesRevertAll,
    FileChangesOpenEditor,
    RulebookSwitcherDeselectAll,
    ToggleAutoApprove,
    ToggleSidePanel,
    AutoApproveCurrentTool,
    ShowProfileSwitcher,
    MouseDragStart(u16, u16),
    MouseDrag(u16, u16),
    MouseDragEnd(u16, u16),
    MouseMove(u16, u16),
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
                | InputEvent::SetSessions(_)
                | InputEvent::SessionRestored { .. }
                | InputEvent::AddUserMessage(_)
        )
    }
}

#[derive(Debug)]
pub enum OutputEvent {
    UserMessage(String, Option<Vec<ToolCallResult>>, Vec<ContentPart>, Option<usize>),
    AcceptTool(ToolCall),
    RejectTool(ToolCall, bool),
    ListSessions,
    SwitchToSession(String),
    NewSession,
    ResumeSession(String),
    SendToolResult(ToolCallResult, bool, Vec<ToolCall>),
    CancelStream,
    ExecuteCommand(String),
}