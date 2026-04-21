//! Type Definitions Module

use chrono::{DateTime, Utc};
use ratatui::text::Line;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::services::Toast;
use crate::services::textarea::TextArea;
use crate::types::*;
use vac_shell::ShellCommand;

// Re-export all submodule types
pub mod rendering;
pub mod shell;
pub mod workbench;
pub mod runtime;
pub mod commands;
pub mod billing;

pub use rendering::{MessageLinesCache, PerMessageCache, QueueMetrics, RenderedMessageCache, RenderMetrics, VisibleLinesCache};
pub use shell::{ShellSession, ShellSessionStore, ShellState};
pub use workbench::{PlanState, ReviewDiffState, ReviewItem, ReviewItemStatus, ReviewState, WorkbenchTab, WorkspaceFocus};
pub use runtime::{ActivityItem, ActivityKind, RuntimeState, VilIssue, VilIssueKind, VilLogEntry, VilSeverity, VilState, VilStatusSnapshot};
pub use commands::{CommandSource, ExistingPlanPrompt, HelperCommand, PendingUserMessage, PlanComment};
pub use billing::{BillingInfo, LoadingOperation, LoadingStateManager, SessionInfo, ShortcutsPopupMode, TokenUsage, ToolCallStatus};

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

// ========== AppState Support Types ==========

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SidePanelSection {
    Context,
    Runtime,
    Changeset,
    Mcp,
    Sessions,
    Todos,
    Usage,
}

#[derive(Debug, Clone)]
pub enum SidePanelRowAction {
    SwitchSession(String),
    ShowMcpDetail(String),
    JumpToVilIssue(String),
}

/// A context chip attached above the input bar via @-mention (PR-T7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextChip {
    /// Display label shown in the chip.
    pub label: String,
    /// The resolved content to attach on submit (file contents, skill description, etc.)
    pub content: String,
    pub namespace: ChipNamespace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipNamespace {
    File,
    Skill,
    Todo,
    Session,
}

/// An entry in the session-resume overlay list (PR-T8).
#[derive(Debug, Clone)]
pub struct SessionResumeEntry {
    pub session_id: uuid::Uuid,
    pub title: String,
    pub project: String,
    pub last_message_preview: String,
    pub last_active: DateTime<Utc>,
    pub model: Option<String>,
    pub token_count: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct StartupSnapshot {
    pub boot_time: DateTime<Utc>,
    pub version: String,
    pub has_vil_engine: bool,
    pub active_rulebook: Option<String>,
    pub environment: String,
    // Phase 3: Startup hydration fields
    pub active_model: Option<String>,
    pub default_model: Option<String>,
    pub active_profile: Option<String>,
    pub selected_rulebooks: Vec<String>,
    pub mcp_server_count: usize,
    pub session_count: usize,
    pub pending_approvals_count: usize,
    pub provider_status: String,
    pub queue_depth: usize,
    /// Whether the terminal answered the Kitty graphics capability probe
    /// positively. Populated at startup by `event_loop::run_tui` before
    /// the first render. False when the terminal is not a TTY, when the
    /// probe times out, or when the reply is missing/malformed.
    pub kitty_graphics: bool,
}

impl Default for StartupSnapshot {
    fn default() -> Self {
        Self {
            boot_time: Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            has_vil_engine: false,
            active_rulebook: None,
            environment: "development".to_string(),
            active_model: None,
            default_model: None,
            active_profile: None,
            selected_rulebooks: Vec::new(),
            mcp_server_count: 0,
            session_count: 0,
            pending_approvals_count: 0,
            provider_status: "initializing".to_string(),
            queue_depth: 0,
            kitty_graphics: false,
        }
    }
}

/// Main application state for TUI
pub struct AppState {
    pub startup: StartupSnapshot,
    /// True once StartupHydrated has been received — gates first real render.
    pub hydrated: bool,
    /// Deadline after which a missing StartupHydrated forces hydration with fallback data.
    pub hydration_deadline: std::time::Instant,
    // Layout state
    pub side_panel_visible: bool,
    pub side_panel_width: u16,
    pub side_panel_section_collapsed: std::collections::HashSet<SidePanelSection>,
    pub side_panel_header_areas: std::collections::HashMap<SidePanelSection, ratatui::layout::Rect>,
    pub side_panel_row_areas: Vec<(SidePanelRowAction, ratatui::layout::Rect)>,

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

    // Shell domain state
    pub shell: ShellState,

    // Streaming state
    pub is_streaming: bool,
    pub cancel_requested: bool,
    /// Number of Ctrl+C presses during a non-streaming idle — two consecutive quits.
    pub quit_press_count: u8,
    /// Timestamp of the first Ctrl+C press for quit disambiguation timeout.
    pub quit_first_press: Option<std::time::Instant>,
    pub streaming_message_id: Option<Uuid>,
    /// Wall-clock time streaming started, for tok/s computation.
    pub streaming_start: Option<std::time::Instant>,
    /// Total tokens received in the current stream.
    pub streaming_tokens: u64,

    // Command palette
    pub command_palette_input: String,
    pub command_palette_selected: usize,
    pub command_palette_scroll: usize,
    pub commands: Vec<HelperCommand>,

    // Helper Dropdown
    pub helper_scroll: usize,
    pub helper_selected: usize,
    pub filtered_helpers: Vec<HelperCommand>,
    pub recent_commands: crate::services::recent_commands::RecentCommands,

    // Shortcuts popup
    pub shortcuts_mode: ShortcutsPopupMode,
    pub shortcuts_scroll: usize,

    // Isolation Switcher
    pub isolation_switcher_selected: usize,
    pub isolation_modes: Vec<String>,
    pub active_isolation_mode: String,
    pub profile_switcher_selected: usize,
    pub profile_search_input: String,
    pub available_profiles: Vec<String>,
    pub filtered_profiles: Vec<String>,
    pub active_profile: String,

    pub rulebook_switcher_selected: usize,
    pub rulebook_search_input: String,
    pub available_rulebooks: Vec<crate::types::ListRuleBook>,
    pub filtered_rulebooks: Vec<crate::types::ListRuleBook>,
    pub selected_rulebooks: std::collections::HashSet<String>,

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

    pub available_models: Vec<Model>,
    pub model_switcher_filter: String,
    pub model_switcher_selected_idx: usize,

    pub all_files: Vec<String>,
    pub file_search_query: String,
    pub file_search_selected_idx: usize,
    pub file_search_results: Vec<String>,

    // ── File Picker v2 (PR-T6) ────────────────────────────────────────────────
    pub file_picker_query: String,
    pub file_picker_selected: usize,
    pub file_picker_results: Vec<std::path::PathBuf>,
    pub file_picker_multi_selected: std::collections::HashSet<usize>,
    pub file_picker_cwd: std::path::PathBuf,
    pub file_picker_type_filter: Option<String>,
    pub file_picker_preview: Option<String>,

    // ── @-mention context chips (PR-T7) ────────────────────────────────────────
    /// Resolved context chips above the input bar.
    pub context_chips: Vec<crate::app::types::ContextChip>,
    pub context_chip_cursor: Option<usize>,

    // Inline @ file picker
    pub at_trigger_active: bool,
    pub at_query: String,
    pub at_results: Vec<String>,
    pub at_selected_idx: usize,

    pub changeset_selected_idx: usize,
    pub changeset_diff_scroll: usize,
    pub changeset_selected_path: Option<String>,
    pub changeset_diff: Option<ReviewDiffState>,

    // Permission UX
    pub auto_approve: bool,
    pub project_root: PathBuf,

    // MCP
    pub mcp_server_states: HashMap<String, vac_tools::mcp::McpConnectionState>,

    // VIL domain state
    pub vil: VilState,

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
    pub banner_message: Option<crate::services::banner::BannerMessage>,
    pub banner_click_regions: Vec<(String, ratatui::layout::Rect)>,
    pub banner_dismiss_region: Option<ratatui::layout::Rect>,
    // PR-T16 — mouse click regions for workbench tabs and task tray rows.
    // Populated during view render, consumed by `handlers::mouse::dispatch_click`.
    pub workbench_tab_regions: Vec<(WorkbenchTab, ratatui::layout::Rect)>,
    pub task_tray_row_regions: Vec<ratatui::layout::Rect>,
    // Unit 8 (Wave 3.6) — Banner queue + severity
    pub banner_queue: crate::services::banner::BannerQueue,

    // Paste ledger (long text + image tray)
    pub pending_pastes: Vec<crate::services::clipboard_paste::PastedItem>,
    pub is_pasting: bool,
    pub paste_counter: usize,

    // File changes popup (compact, searchable)
    pub file_changes_selected: usize,
    pub file_changes_search: String,
    pub file_changes_scroll: usize,

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
    pub line_to_message_map: Vec<Uuid>,
    pub pending_revert_index: Option<usize>,

    // Plan domain state
    pub plan: PlanState,

    // Ask-User popup (triggered by `ask_user` tool call)
    pub ask_user_question: Option<String>,
    pub ask_user_options: Vec<crate::services::ask_user::AskUserOption>,
    pub ask_user_selected: usize,
    pub ask_user_input: String,
    pub ask_user_tool_call_id: Option<String>,
    pub ask_user_allow_free_text: bool,
    // Unit 7 (Wave 3.5) — structured ask-user UX
    pub ask_user_question_kind: crate::services::ask_user::AskUserQuestionKind,
    pub ask_user_multi_selected: std::collections::HashSet<usize>,
    pub ask_user_metadata: std::collections::HashMap<String, String>,
    pub ask_user_filter: String,
    pub ask_user_search_active: bool,
    pub ask_user_scroll: usize,

    // Text Selection
    pub selection_state: crate::services::text_selection::SelectionState,
    pub per_message_cache: PerMessageCache,
    pub render_metrics: RenderMetrics,
    pub assembled_lines_cache: Option<MessageLinesCache>,
    pub collapsed_message_lines_cache: Option<MessageLinesCache>,
    pub message_area_y: u16,
    pub message_area_height: u16,

    pub input_tx: Option<tokio::sync::mpsc::Sender<crate::app::events::InputEvent>>,

    /// Overlay stack: tracks open modal overlays and their event-capture order.
    pub overlay_manager: crate::overlay::OverlayManager,

    // ── Task Tray (PR-T9) ────────────────────────────────────────────────────
    pub task_tray_selected: usize,
    pub task_tray_scroll: usize,
    /// Whether the task tray shows only active (running/queued) or all jobs.
    pub task_tray_filter_active_only: bool,

    // ── Theme (PR-T5) ────────────────────────────────────────────────────────
    pub theme: crate::services::theme::Theme,
    pub theme_picker_selected: usize,

    // ── Session Resume Overlay (PR-T8) ───────────────────────────────────────
    pub session_resume_query: String,
    pub session_resume_selected: usize,
    pub session_resume_list: Vec<crate::app::types::SessionResumeEntry>,
    /// Sorted indices into `session_resume_list` after fuzzy + date filter.
    pub session_resume_filtered_indices: Vec<usize>,
    /// Date filter: `None` = all time, `Some(n)` = last n days.
    pub session_resume_date_filter_days: Option<u32>,

    // vil_workbench fields moved to VilState.workbench_selected / .workbench_group_filter

    // ===== Unit 5 (Wave 3.1) — Attachment tray preview & reorder =====
    /// Cursor in the paste tray; indexes into `pending_pastes`.
    pub pending_paste_selected: usize,
    /// When true, `J` / `K` swap the selected paste with its neighbor
    /// (instead of selecting). Toggle with `r` while the tray is focused.
    pub pending_paste_reorder_mode: bool,

    // ===== Context Composer (Wave 3) =====
    pub context_composer_visible: bool,
    pub validation_score: Option<f64>,
    pub validation_issues: Vec<String>,
    pub lsp_available: bool,
    pub lsp_diagnostics: Option<vac_core::lsp::types::LspWorkspaceSnapshot>,
    /// PR-T15 P1 — per-file overlay cache for inline diagnostics squiggles.
    /// Renderers clear it when the focused file changes to avoid stale spans
    /// leaking between files (see `services::diagnostics_overlay`).
    pub diagnostics_overlay_cache: crate::services::diagnostics_overlay::DiagnosticsOverlayCache,
    pub pinned_files: Vec<String>,
    pub pinned_diffs: Vec<String>,
    pub pinned_diagnostics: Vec<String>,
    pub pinned_runtime_items: Vec<String>,
    pub pinned_plan_items: Vec<String>,

    pub pending_user_messages: VecDeque<PendingUserMessage>,

    /// Queue metrics for I/O reliability tracking (Phase 4).
    pub queue_metrics: QueueMetrics,
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
            startup: StartupSnapshot::default(),
            hydrated: false,
            hydration_deadline: std::time::Instant::now() + std::time::Duration::from_secs(10),
            side_panel_visible: false,
            side_panel_width: 30,
            side_panel_section_collapsed: std::collections::HashSet::new(),
            side_panel_header_areas: std::collections::HashMap::new(),
            side_panel_row_areas: vec![],
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
            shell: ShellState::default(),
            is_streaming: false,
            cancel_requested: false,
            quit_press_count: 0,
            quit_first_press: None,
            streaming_message_id: None,
            streaming_start: None,
            streaming_tokens: 0,
            command_palette_input: String::new(),
            command_palette_selected: 0,
            command_palette_scroll: 0,
            commands: Self::default_commands(),
            helper_scroll: 0,
            helper_selected: 0,
            filtered_helpers: Vec::new(),
            recent_commands: crate::services::recent_commands::RecentCommands::load(),

            shortcuts_mode: ShortcutsPopupMode::default(),
            shortcuts_scroll: 0,
            isolation_switcher_selected: 0,
            isolation_modes: vec![
                "host".to_string(),
                "isolated".to_string(),
                "isolated (Rust)".to_string(),
                "isolated (Node)".to_string(),
                "isolated (Python)".to_string(),
            ],
            active_isolation_mode: "host".to_string(),
            profile_switcher_selected: 0,
            profile_search_input: String::new(),
            available_profiles: Vec::new(),
            filtered_profiles: Vec::new(),
            active_profile: "default".to_string(),
            rulebook_switcher_selected: 0,
            rulebook_search_input: String::new(),
            available_rulebooks: Vec::new(),
            filtered_rulebooks: Vec::new(),
            selected_rulebooks: std::collections::HashSet::new(),
            message_action_popup_selected: 0,
            message_action_target_id: None,
            changeset_store: vac_changeset::ChangesetStore::new(),
            modified_files: Vec::new(),
            workbench_tab: WorkbenchTab::Approvals,
            review: ReviewState::default(),
            sessions_selected_idx: 0,
            runtime: RuntimeState::default(),
            activity: Vec::new(),
            activity_scroll: 0,
            toasts: Vec::new(),
            available_models: Vec::new(),
            model_switcher_filter: String::new(),
            model_switcher_selected_idx: 0,
            all_files: Vec::new(),
            file_search_query: String::new(),
            file_search_selected_idx: 0,
            file_search_results: Vec::new(),
            file_picker_query: String::new(),
            file_picker_selected: 0,
            file_picker_results: Vec::new(),
            file_picker_multi_selected: std::collections::HashSet::new(),
            file_picker_cwd: options.project_root.clone(),
            file_picker_type_filter: None,
            file_picker_preview: None,
            context_chips: Vec::new(),
            context_chip_cursor: None,
            at_trigger_active: false,
            at_query: String::new(),
            at_results: Vec::new(),
            at_selected_idx: 0,
            changeset_selected_idx: 0,
            changeset_diff_scroll: 0,
            changeset_selected_path: None,
            changeset_diff: None,
            auto_approve: false,
            project_root: options.project_root,
            mcp_server_states: HashMap::new(),
            vil: VilState::default(),
            vwfd_inspector: crate::services::vwfd_inspector::VwfdInspectorState::default(),
            vil_expr_lint: crate::services::vil_expr_lint::LintState::new(),
            pending_image_parts: vec![],
            banner_message: None,
            banner_click_regions: Vec::new(),
            banner_dismiss_region: None,
            workbench_tab_regions: Vec::new(),
            task_tray_row_regions: Vec::new(),
            banner_queue: crate::services::banner::BannerQueue::new(),
            pending_pastes: Vec::new(),
            is_pasting: false,
            paste_counter: 0,
            file_changes_selected: 0,
            file_changes_search: String::new(),
            file_changes_scroll: 0,
            todos: Vec::new(),
            current_message_usage: TokenUsage::default(),
            total_session_usage: TokenUsage::default(),
            context_usage_percent: 0.0,
            billing_info: None,
            auth_display_info: (None, None, None),
            line_to_message_map: Vec::new(),
            pending_revert_index: None,
            plan: PlanState::default(),
            ask_user_question: None,
            ask_user_options: Vec::new(),
            ask_user_selected: 0,
            ask_user_input: String::new(),
            ask_user_tool_call_id: None,
            ask_user_allow_free_text: true,
            ask_user_question_kind: crate::services::ask_user::AskUserQuestionKind::SingleSelect,
            ask_user_multi_selected: std::collections::HashSet::new(),
            ask_user_metadata: std::collections::HashMap::new(),
            ask_user_filter: String::new(),
            ask_user_search_active: false,
            ask_user_scroll: 0,
            selection_state: crate::services::text_selection::SelectionState::default(),
            per_message_cache: HashMap::new(),
            render_metrics: RenderMetrics::default(),
            assembled_lines_cache: None,
            collapsed_message_lines_cache: None,
            message_area_y: 0,
            message_area_height: 0,
            input_tx: None,
            overlay_manager: crate::overlay::OverlayManager::new(),
            task_tray_selected: 0,
            task_tray_scroll: 0,
            task_tray_filter_active_only: false,
            theme: crate::services::theme::Theme::default(),
            theme_picker_selected: 0,
            session_resume_query: String::new(),
            session_resume_selected: 0,
            session_resume_list: Vec::new(),
            session_resume_filtered_indices: Vec::new(),
            session_resume_date_filter_days: None,
            // Unit 9 (Wave 4.1) — VIL Issue Workstation
            // vil workbench fields are in vil: VilState::default()
            // Unit 5 (Wave 3.1) — Attachment tray preview & reorder
            pending_paste_selected: 0,
            pending_paste_reorder_mode: false,
            // Context Composer
            context_composer_visible: false,
            validation_score: None,
            validation_issues: Vec::new(),
            lsp_available: false,
            lsp_diagnostics: None,
            diagnostics_overlay_cache: crate::services::diagnostics_overlay::DiagnosticsOverlayCache::default(),
            pinned_files: Vec::new(),
            pinned_diffs: Vec::new(),
            pinned_diagnostics: Vec::new(),
            pinned_runtime_items: Vec::new(),
            pinned_plan_items: Vec::new(),
            pending_user_messages: VecDeque::new(),
            queue_metrics: QueueMetrics::default(),
        }
    }

    fn default_commands() -> Vec<HelperCommand> {
        crate::services::helper_block::vac_commands()
    }

    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(Message::user(content, None));
    }

    /// Count of user-role messages in the current transcript. Computed from
    /// `messages` rather than denormalized so revert/reset operations don't
    /// need to remember to adjust a counter.
    pub fn user_message_count(&self) -> usize {
        self.messages.iter().filter(|m| m.role == "user").count()
    }

    /// Replace any pasted-content placeholder tokens in `raw` with the real
    /// content (for text pastes) or strip them (for image pastes — the image
    /// rides via `pending_image_parts`). Drains `pending_pastes` regardless of
    /// whether every placeholder was found, so the ledger stays in sync with
    /// a submission.
    pub fn expand_pending_pastes(&mut self, raw: &str) -> String {
        use crate::services::clipboard_paste::PastedKind;
        if self.pending_pastes.is_empty() {
            return raw.to_string();
        }
        let mut out = raw.to_string();
        for item in self.pending_pastes.drain(..) {
            let replacement = match item.kind {
                PastedKind::Text { content, .. } => content,
                PastedKind::Image { .. } => String::new(),
            };
            out = out.replace(&item.placeholder, &replacement);
        }
        out
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(Message::assistant(content));
    }

    pub fn record_vil_score(&mut self, score: f64) {
        let should_push = match self.vil.score_history.last().copied() {
            Some(prev) => (prev - score).abs() > 0.0001,
            None => true,
        };
        if should_push {
            self.vil.score_history.push(score);
            if self.vil.score_history.len() > 60 {
                let drain = self.vil.score_history.len().saturating_sub(60);
                self.vil.score_history.drain(0..drain);
            }
        }
    }

    pub fn push_vil_log(&mut self, message: impl Into<String>) {
        self.vil.event_log.push_back(VilLogEntry {
            at: Utc::now(),
            message: message.into(),
        });
        while self.vil.event_log.len() > 200 {
            self.vil.event_log.pop_front();
        }
    }

    pub fn filtered_commands(&self) -> Vec<HelperCommand> {
        let mut cmds: Vec<_> = if self.command_palette_input.is_empty() {
            self.commands
                .iter()
                .filter(|c| c.surface != crate::services::commands::CommandSurface::Hidden)
                .cloned()
                .collect()
        } else {
            self.commands
                .iter()
                .filter(|c| {
                    c.surface != crate::services::commands::CommandSurface::Hidden
                        && (c
                            .command
                            .to_lowercase()
                            .contains(&self.command_palette_input.to_lowercase())
                            || c.description
                                .to_lowercase()
                                .contains(&self.command_palette_input.to_lowercase()))
                })
                .cloned()
                .collect()
        };

        cmds.sort_by_key(|cmd| {
            let freq = self
                .recent_commands
                .frequencies
                .get(&cmd.command)
                .copied()
                .unwrap_or(0);
            let recent_idx = self
                .recent_commands
                .history
                .iter()
                .position(|h| h == &cmd.command)
                .unwrap_or(usize::MAX);
            (std::cmp::Reverse(freq), recent_idx)
        });

        cmds
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
        out.sort_by_key(|m| {
            let recent_idx = self
                .recent_commands
                .recent_models
                .iter()
                .position(|r| r == &m.id)
                .unwrap_or(usize::MAX);
            (recent_idx, m.provider.clone(), m.name.clone())
        });
        out
    }

    pub fn profile_switcher_filtered(&self) -> Vec<String> {
        let q = self.profile_search_input.trim().to_lowercase();
        self.available_profiles
            .iter()
            .filter(|p| q.is_empty() || p.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    pub fn rulebook_switcher_filtered(&self) -> Vec<crate::types::ListRuleBook> {
        let q = self.rulebook_search_input.trim().to_lowercase();
        self.available_rulebooks
            .iter()
            .filter(|r| {
                q.is_empty()
                    || r.id.to_lowercase().contains(&q)
                    || r.name.to_lowercase().contains(&q)
                    || r.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&q))
            })
            .cloned()
            .collect()
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

        self.review
            .items
            .retain(|k, v| active_paths.contains(k) || v.status != ReviewItemStatus::Pending);

        for entry in self.changeset_store.active_entries() {
            let path = &entry.path;
            let has_snapshot = session_id
                .map(|sid| {
                    crate::services::review::snapshot_path(&self.project_root, sid, path).exists()
                })
                .unwrap_or(false);

            self.review
                .items
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
        let filter = self.review.filter.trim().to_lowercase();
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
            .review
            .items
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
            self.review.selected_idx = 0;
            self.review.selected_path = None;
            self.review.diff = None;
            return;
        }

        if let Some(path) = self.review.selected_path.clone() {
            if let Some(idx) = paths.iter().position(|p| p == &path) {
                self.review.selected_idx = idx;
                return;
            }
        }

        if self.review.selected_idx >= paths.len() {
            self.review.selected_idx = paths.len() - 1;
        }
        self.review.selected_path = Some(paths[self.review.selected_idx].clone());
    }

    pub fn review_select_by_delta(&mut self, delta: isize) {
        let paths = self.review_filtered_paths();
        if paths.is_empty() {
            self.review.selected_idx = 0;
            self.review.selected_path = None;
            self.review.diff = None;
            return;
        }

        let len = paths.len() as isize;
        let mut idx = self.review.selected_idx as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len {
            idx = len - 1;
        }
        self.review.selected_idx = idx as usize;
        self.review.selected_path = Some(paths[self.review.selected_idx].clone());
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
