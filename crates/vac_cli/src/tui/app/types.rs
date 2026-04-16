//! Type Definitions Module

use chrono::{DateTime, Utc};
use ratatui::text::Line;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::tui::services::textarea::TextArea;
use crate::tui::services::Toast;
use crate::tui::types::*;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFocus {
    Conversation,
    Input,
    Workbench,
    Activity,
}

impl WorkspaceFocus {
    pub fn next(self) -> Self {
        match self {
            Self::Conversation => Self::Input,
            Self::Input => Self::Workbench,
            Self::Workbench => Self::Activity,
            Self::Activity => Self::Conversation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchTab {
    Approvals,
    Review,
    Sessions,
}

impl WorkbenchTab {
    pub fn next(self) -> Self {
        match self {
            Self::Approvals => Self::Review,
            Self::Review => Self::Sessions,
            Self::Sessions => Self::Approvals,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityKind {
    Status,
    Tool,
    Approval,
    Review,
    Session,
    Error,
}

#[derive(Debug, Clone)]
pub struct ActivityItem {
    pub at: DateTime<Utc>,
    pub kind: ActivityKind,
    pub message: String,
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

#[derive(Debug, Clone, PartialEq)]
pub enum ReviewItemStatus {
    Pending,
    Restored,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ReviewItem {
    pub path: String,
    pub status: ReviewItemStatus,
    pub has_snapshot: bool,
    pub last_error: Option<String>,
    pub dirty_generation: u64,
}

#[derive(Debug, Clone)]
pub struct ReviewDiffState {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub scroll: usize,
    pub last_error: Option<String>,
}

// ========== AppState ==========

/// Main application state for TUI
pub struct AppState {
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

    // Approval state
    pub pending_approvals: Vec<ToolCall>,
    pub pending_tool_calls: Vec<ToolCall>,
    pub approved_tools: Vec<ToolCall>,
    pub rejected_tools: Vec<ToolCall>,
    pub approval_selected_idx: usize,
    pub approval_detail_scroll: usize,
    pub approval_explanations: HashMap<String, Option<String>>,
    pub reject_reason_input: Option<String>, // Some(_) = reason prompt active

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

    pub changeset_store: crate::tui::services::ChangesetStore,
    pub modified_files: Vec<String>,

    pub workbench_tab: WorkbenchTab,
    pub review_open: bool,
    pub review_filter: String,
    pub review_selected_idx: usize,
    pub review_selected_path: Option<String>,
    pub review_items: HashMap<String, ReviewItem>,
    pub review_diff: Option<ReviewDiffState>,
    pub review_generation: u64,

    pub sessions_selected_idx: usize,

    pub activity: Vec<ActivityItem>,
    pub activity_scroll: usize,

    pub toasts: Vec<Toast>,

    pub available_models: Vec<Model>,
    pub show_model_switcher: bool,
    pub model_switcher_filter: String,
    pub model_switcher_selected_idx: usize,

    pub all_files: Vec<String>,
    pub show_file_search: bool,
    pub file_search_query: String,
    pub file_search_selected_idx: usize,
    pub file_search_results: Vec<String>,

    pub show_changeset: bool,
    pub changeset_selected_idx: usize,
    pub changeset_diff_scroll: usize,
    pub changeset_selected_path: Option<String>,
    pub changeset_diff: Option<ReviewDiffState>,

    // Permission UX
    pub auto_approve: bool,
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
            focus: WorkspaceFocus::Input,
            messages: Vec::new(),
            scroll: 0,
            loading: false,
            loading_manager: LoadingStateManager::new(),
            spinner_frame: 0,
            session_id: options
                .session_id
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            sessions: Vec::new(),
            session_title: None,
            checkpoint_path: options.checkpoint_path,
            current_model: options.model,
            mouse_capture_enabled: true,
            pending_approvals: Vec::new(),
            pending_tool_calls: Vec::new(),
            approved_tools: Vec::new(),
            rejected_tools: Vec::new(),
            approval_selected_idx: 0,
            approval_detail_scroll: 0,
            approval_explanations: HashMap::new(),
            reject_reason_input: None,
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
            changeset_store: crate::tui::services::ChangesetStore::new(),
            modified_files: Vec::new(),
            workbench_tab: WorkbenchTab::Approvals,
            review_open: false,
            review_filter: String::new(),
            review_selected_idx: 0,
            review_selected_path: None,
            review_items: HashMap::new(),
            review_diff: None,
            review_generation: 0,
            sessions_selected_idx: 0,
            activity: Vec::new(),
            activity_scroll: 0,
            toasts: Vec::new(),
            available_models: Vec::new(),
            show_model_switcher: false,
            model_switcher_filter: String::new(),
            model_switcher_selected_idx: 0,
            all_files: Vec::new(),
            show_file_search: false,
            file_search_query: String::new(),
            file_search_selected_idx: 0,
            file_search_results: Vec::new(),
            show_changeset: false,
            changeset_selected_idx: 0,
            changeset_diff_scroll: 0,
            changeset_selected_path: None,
            changeset_diff: None,
            auto_approve: false,
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

    pub fn model_switcher_filtered(&self) -> Vec<Model> {
        let q = self.model_switcher_filter.trim().to_lowercase();
        let mut out = self
            .available_models
            .iter()
            .filter(|m| {
                q.is_empty()
                    || m.name.to_lowercase().contains(&q)
                    || m.provider.to_lowercase().contains(&q)
                    || m.id.to_lowercase().contains(&q)
            })
            .cloned()
            .collect::<Vec<_>>();
        out.sort_by(|a, b| a.provider.cmp(&b.provider).then_with(|| a.name.cmp(&b.name)));
        out
    }

    pub fn review_sync_items(&mut self) {
        let session_id = uuid::Uuid::parse_str(&self.session_id).ok();
        // Use changeset_store as primary source - active entries only
        let active_paths: std::collections::HashSet<String> = self
            .changeset_store
            .active_entries()
            .iter()
            .map(|e| e.path.clone())
            .collect();

        self.review_items
            .retain(|k, v| active_paths.contains(k) || v.status != ReviewItemStatus::Pending);

        for entry in self.changeset_store.active_entries() {
            let path = &entry.path;
            let has_snapshot = session_id
                .map(|sid| {
                    crate::tui::services::review::snapshot_path(&self.project_root, sid, path)
                        .exists()
                })
                .unwrap_or(false);

            self.review_items
                .entry(path.clone())
                .and_modify(|it| {
                    it.has_snapshot = has_snapshot;
                    it.status = ReviewItemStatus::Pending;
                    it.last_error = None;
                })
                .or_insert_with(|| ReviewItem {
                    path: path.clone(),
                    status: ReviewItemStatus::Pending,
                    has_snapshot,
                    last_error: None,
                    dirty_generation: 0,
                });
        }
    }

    pub fn review_filtered_paths(&self) -> Vec<String> {
        let filter = self.review_filter.trim().to_lowercase();
        // Primary source: changeset_store active entries (insertion order preserved)
        let mut ordered: Vec<String> = vec![];
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for entry in self.changeset_store.active_entries() {
            if seen.insert(entry.path.clone()) {
                ordered.push(entry.path.clone());
            }
        }

        // Include any review_items not in store (e.g. Restored/Failed still visible)
        let mut extra: Vec<String> = self
            .review_items
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        extra.sort();
        ordered.extend(extra);

        ordered
            .into_iter()
            .filter(|p| {
                if filter.is_empty() {
                    true
                } else {
                    p.to_lowercase().contains(&filter)
                }
            })
            .collect()
    }

    pub fn review_normalize_selection(&mut self) {
        let paths = self.review_filtered_paths();
        if paths.is_empty() {
            self.review_selected_idx = 0;
            self.review_selected_path = None;
            self.review_diff = None;
            return;
        }

        if let Some(path) = self.review_selected_path.clone() {
            if let Some(idx) = paths.iter().position(|p| p == &path) {
                self.review_selected_idx = idx;
                return;
            }
        }

        if self.review_selected_idx >= paths.len() {
            self.review_selected_idx = paths.len() - 1;
        }
        self.review_selected_path = Some(paths[self.review_selected_idx].clone());
    }

    pub fn review_select_by_delta(&mut self, delta: isize) {
        let paths = self.review_filtered_paths();
        if paths.is_empty() {
            self.review_selected_idx = 0;
            self.review_selected_path = None;
            self.review_diff = None;
            return;
        }

        let len = paths.len() as isize;
        let mut idx = self.review_selected_idx as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len {
            idx = len - 1;
        }
        self.review_selected_idx = idx as usize;
        self.review_selected_path = Some(paths[self.review_selected_idx].clone());
    }

    pub fn approval_normalize_selection(&mut self) {
        if self.pending_approvals.is_empty() {
            self.approval_selected_idx = 0;
            self.approval_reset_detail();
            return;
        }
        if self.approval_selected_idx >= self.pending_approvals.len() {
            self.approval_selected_idx = self.pending_approvals.len() - 1;
            self.approval_reset_detail();
        }
    }

    pub fn push_activity(&mut self, kind: ActivityKind, message: impl Into<String>) {
        self.activity.push(ActivityItem {
            at: Utc::now(),
            kind,
            message: message.into(),
        });
        if self.activity.len() > 500 {
            let overflow = self.activity.len() - 500;
            self.activity.drain(0..overflow);
            self.activity_scroll = self.activity_scroll.saturating_sub(overflow);
        }
    }

    fn approval_reset_detail(&mut self) {
        self.approval_detail_scroll = 0;
    }
}
