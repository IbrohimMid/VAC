//! Type Definitions Module

use ratatui::text::Line;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::tui::types::*;
use crate::tui::services::textarea::TextArea;

// ========== Cache Types ==========

pub type MessageLinesCache = (Vec<Message>, usize, Vec<Line<'static>>);

#[derive(Clone, Debug)]
pub struct RenderedMessageCache {
    pub content_hash: u64,
    pub rendered_lines: Arc<Vec<Line<'static>>>,
    pub width: usize,
}

pub type PerMessageCache = HashMap<Uuid, RenderedMessageCache>;

#[derive(Clone, Debug)]
pub struct VisibleLinesCache {
    pub scroll: usize,
    pub width: usize,
    pub height: usize,
    pub lines: Arc<Vec<Line<'static>>>,
    pub source_generation: u64,
}

#[derive(Debug, Default, Clone)]
pub struct RenderMetrics {
    pub last_render_time_us: u64,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub total_lines: usize,
    pub avg_render_time_us: u64,
    render_count: u64,
}

// ========== Helper Types ==========

#[derive(Debug, Clone, PartialEq)]
pub enum CommandSource {
    BuiltIn,
    BuiltInWithPrompt { prompt_content: String },
    Custom { prompt_content: String },
}

#[derive(Debug, Clone)]
pub struct HelperCommand {
    pub command: String,
    pub description: String,
    pub source: CommandSource,
}

// ========== Session Types ==========

#[derive(Debug, Clone)]
pub struct ExistingPlanPrompt {
    pub inline_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub title: String,
    pub id: String,
    pub updated_at: String,
    pub checkpoints: Vec<String>,
}



#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LoadingOperation {
    LlmRequest,
    ToolExecution,
    SessionsList,
    StreamProcessing,
    CheckpointResume,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallStatus {
    Approved,
    Rejected,
    Executed,
    Skipped,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ShortcutsPopupMode {
    #[default]
    Commands,
    Shortcuts,
    Sessions,
}

#[derive(Debug)]
pub struct LoadingStateManager {
    active_operations: std::collections::HashSet<LoadingOperation>,
}

impl Default for LoadingStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadingStateManager {
    pub fn new() -> Self {
        Self {
            active_operations: std::collections::HashSet::new(),
        }
    }

    pub fn start_operation(&mut self, operation: LoadingOperation) {
        self.active_operations.insert(operation);
    }

    pub fn end_operation(&mut self, operation: LoadingOperation) {
        self.active_operations.remove(&operation);
    }

    pub fn is_loading(&self) -> bool {
        !self.active_operations.is_empty()
    }

    pub fn clear_all(&mut self) {
        self.active_operations.clear();
    }
}

// ========== Message ==========

#[derive(Debug, Clone)]
pub struct Message {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl Message {
    pub fn user(content: String, tool_calls: Option<Vec<ToolCall>>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: "user".to_string(),
            content,
            tool_calls,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: "assistant".to_string(),
            content,
            tool_calls: None,
        }
    }
}

// ========== AppState ==========

/// Main application state for TUI
pub struct AppState {
    // Input state
    pub input: TextArea,
    pub cursor_position: usize,
    
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
    
    // Dialog state
    pub is_dialog_open: bool,
    pub dialog_command: Option<ToolCall>,
    pub dialog_selected: usize,
    pub dialog_focused: bool,
    
    // Approval state
    pub pending_tool_calls: Vec<ToolCall>,
    pub approved_tools: Vec<ToolCall>,
    pub rejected_tools: Vec<ToolCall>,
    
    // Shell state
    pub shell_popup_visible: bool,
    pub shell_output: String,
    
    // Streaming state
    pub is_streaming: bool,
    pub cancel_requested: bool,
    pub streaming_message_id: Option<Uuid>,
    
    // Command palette
    pub show_command_palette: bool,
    pub command_palette_input: String,
    pub command_palette_selected: usize,
    pub commands: Vec<HelperCommand>,
    
    // Shortcuts popup
    pub show_shortcuts: bool,
    pub shortcuts_mode: ShortcutsPopupMode,
    
    // Diff preview
    pub show_diff_preview: bool,
    pub diff_file_path: Option<String>,
    pub diff_old_content: Option<String>,
    pub diff_new_content: Option<String>,
    
    // File changes popup state
    pub show_file_changes_popup: bool,
    pub file_changes_search: String,
    pub file_changes_scroll: usize,
    pub file_changes_selected: usize,
    pub modified_files: Vec<String>,
    
    // Permission UX
    pub auto_approve: bool,
    pub permission_explanation: Option<String>,
    pub project_root: PathBuf,
}

/// Options for creating AppState
pub struct AppStateOptions {
    pub model: Option<Model>,
    pub session_id: Option<String>,
    pub checkpoint_path: Option<PathBuf>,
    pub project_root: PathBuf,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(AppStateOptions {
            model: None,
            session_id: None,
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap_or_default(),
        })
    }
}

impl AppState {
    pub fn new(options: AppStateOptions) -> Self {
        Self {
            input: TextArea::new(),
            cursor_position: 0,
            messages: Vec::new(),
            scroll: 0,
            loading: false,
            loading_manager: LoadingStateManager::new(),
            spinner_frame: 0,
            session_id: options.session_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            sessions: Vec::new(),
            session_title: None,
            checkpoint_path: options.checkpoint_path,
            current_model: options.model,
            mouse_capture_enabled: true,
            is_dialog_open: false,
            dialog_command: None,
            dialog_selected: 0,
            dialog_focused: true,
            pending_tool_calls: Vec::new(),
            approved_tools: Vec::new(),
            rejected_tools: Vec::new(),
            shell_popup_visible: false,
            shell_output: String::new(),
            is_streaming: false,
            cancel_requested: false,
            streaming_message_id: None,
            show_command_palette: false,
            command_palette_input: String::new(),
            command_palette_selected: 0,
            commands: Self::default_commands(),
            show_shortcuts: false,
            shortcuts_mode: ShortcutsPopupMode::default(),
            show_diff_preview: false,
            diff_file_path: None,
            diff_old_content: None,
            diff_new_content: None,
            show_file_changes_popup: false,
            file_changes_search: String::new(),
            file_changes_scroll: 0,
            file_changes_selected: 0,
            modified_files: Vec::new(),
            auto_approve: false,
            permission_explanation: None,
            project_root: options.project_root,
        }
    }
    
    fn default_commands() -> Vec<HelperCommand> {
        crate::tui::services::helper_block::vac_commands()
    }
    
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(Message::user(content, None));
    }
    
    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(Message::assistant(content));
    }
    
    pub fn filtered_commands(&self) -> Vec<HelperCommand> {
        if self.command_palette_input.is_empty() {
            return self.commands.clone();
        }
        self.commands
            .iter()
            .filter(|c| c.command.starts_with(&self.command_palette_input))
            .cloned()
            .collect()
    }
}
