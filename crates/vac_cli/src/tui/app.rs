//! TuiApp state — sessions, focus, panels, scroll.

use std::path::Path;
use tokio::sync::oneshot;
use vac_core::engine::{EngineStatus, TaskHistoryEntry};
use vac_core::TaskResult;
use vac_runtime::jobs::Job;
use vac_runtime::executor::OperatingMode;

use super::services::detail::DetailMode;
use super::services::history::HistoryState;
use super::services::scroll::ScrollManager;

// ── Focus ─────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FocusPane {
    Composer,
    Transcript,
    History,
    Detail,
}

// ── Approval ──────────────────────────────────────────────────────────────────

pub struct PendingApproval {
    pub tool_name: String,
    pub summary: String,
    pub args_preview: String,
    pub responder: oneshot::Sender<bool>,
}

// ── Per-session state ─────────────────────────────────────────────────────────

pub use vil_swarm::SessionInfo;

#[derive(Clone)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
}

impl TuiApp {
    pub fn available_commands() -> Vec<CommandInfo> {
        vec![
            CommandInfo { name: "/sessions".to_string(), description: "List and resume previous sessions".to_string() },
            CommandInfo { name: "/resume".to_string(), description: "Resume last session".to_string() },
            CommandInfo { name: "/help".to_string(), description: "Show help information".to_string() },
            CommandInfo { name: "/quit".to_string(), description: "Exit VAC interactive mode".to_string() },
        ]
    }
}

#[derive(Clone)]
pub struct TranscriptEntry {
    pub label: String,
    pub body: String,
    pub color: ratatui::style::Color,
}

#[derive(Clone)]
pub struct SessionTab {
    pub label: String,
    pub transcript: Vec<TranscriptEntry>,
    pub history: Vec<TaskHistoryEntry>,
    pub last_result: Option<TaskResult>,
    pub active_task: Option<String>,
    // Lane telemetry (tool events go here, NOT transcript)
    pub thinking_log: Vec<String>,
    pub reading_log: Vec<String>,
    pub commands_log: Vec<String>,
}

impl SessionTab {
    pub fn new(idx: usize) -> Self {
        Self {
            label: format!("Session {}", idx + 1),
            transcript: vec![TranscriptEntry {
                label: "VAC".to_string(),
                body: "New session. Type a task and press Enter.".to_string(),
                color: ratatui::style::Color::Cyan,
            }],
            history: vec![],
            last_result: None,
            active_task: None,
            thinking_log: vec!["Ready".to_string()],
            reading_log: vec!["No reads yet".to_string()],
            commands_log: vec!["Waiting for task".to_string()],
        }
    }
}

// ── Main app state ────────────────────────────────────────────────────────────

pub struct TuiApp {
    pub running: bool,
    pub input: String,
    pub status: EngineStatus,
    pub sessions: Vec<SessionTab>,
    pub active_session: usize,
    pub focus: FocusPane,
    pub history: HistoryState,
    pub detail: DetailMode,
    pub scroll: ScrollManager,
    pub live_diff_files: Vec<String>,
    pub show_help: bool,
    pub active_provider: String,
    pub active_model: String,
    pub current_phase: String,
    pub last_activity: String,
    pub auth_ready: bool,
    pub auth_hint: Option<String>,
    pub spinner_tick: usize,
    pub pending_approval: Option<PendingApproval>,
    pub streaming_assistant: Option<usize>,
    // Runtime integration (optional — None when no scheduler attached)
    pub runtime_jobs: Vec<Job>,
    pub operating_mode: Option<OperatingMode>,
    // Session management
    pub show_sessions_popup: bool,
    pub available_sessions: Vec<SessionInfo>,
    pub session_selected: usize,
    pub session_search: String,
    // Command autocomplete
    pub show_command_list: bool,
    pub command_selected: usize,
    /// Cancellation token for the currently running task (None when idle)
    pub cancel_token: Option<tokio_util::sync::CancellationToken>,
}

impl TuiApp {
    pub fn new(status: EngineStatus, history: Vec<TaskHistoryEntry>, auth_hint: Option<String>) -> Self {
        let auth_ready = auth_hint.is_none();
        let mut first = SessionTab::new(0);
        first.history = history;
        if let Some(hint) = &auth_hint {
            first.transcript.push(TranscriptEntry {
                label: "Auth".to_string(),
                body: hint.clone(),
                color: ratatui::style::Color::Yellow,
            });
            first.commands_log.push("Run `vac auth login` first".to_string());
        }
        Self {
            running: true,
            input: String::new(),
            status,
            sessions: vec![first],
            active_session: 0,
            focus: FocusPane::Composer,
            history: HistoryState::new(),
            detail: DetailMode::None,
            scroll: ScrollManager::default(),
            live_diff_files: vec![],
            show_help: true,
            active_provider: "kilo".to_string(),
            active_model: std::env::var("KILO_MODEL").unwrap_or_else(|_| "kilo-auto/free".to_string()),
            current_phase: "Idle".to_string(),
            last_activity: "Waiting for task".to_string(),
            auth_ready,
            auth_hint,
            spinner_tick: 0,
            pending_approval: None,
            streaming_assistant: None,
            runtime_jobs: vec![],
            operating_mode: None,
            show_sessions_popup: false,
            available_sessions: vec![],
            session_selected: 0,
            session_search: String::new(),
            show_command_list: false,
            command_selected: 0,
            cancel_token: None,
        }
    }

    // ── Session accessors ─────────────────────────────────────────────────────

    pub fn session(&self) -> &SessionTab { &self.sessions[self.active_session] }
    pub fn session_mut(&mut self) -> &mut SessionTab { &mut self.sessions[self.active_session] }

    pub fn new_session(&mut self) {
        let idx = self.sessions.len();
        self.sessions.push(SessionTab::new(idx));
        self.active_session = idx;
        self.reset_pane_state();
    }

    pub fn next_session(&mut self) {
        if self.sessions.len() > 1 {
            self.active_session = (self.active_session + 1) % self.sessions.len();
            self.reset_pane_state();
        }
    }

    pub fn prev_session(&mut self) {
        if self.sessions.len() > 1 {
            self.active_session = self.active_session.checked_sub(1).unwrap_or(self.sessions.len() - 1);
            self.reset_pane_state();
        }
    }

    fn reset_pane_state(&mut self) {
        self.history.clear();
        self.detail = DetailMode::None;
        self.streaming_assistant = None;
        self.scroll.transcript.offset = 0;
        self.focus = FocusPane::Composer;
    }

    // ── Focus cycling ─────────────────────────────────────────────────────────

    /// Cycle focus: Composer → Transcript → History → Detail (if open) → Composer.
    /// Modal (approval/revert confirm) locks focus — Tab ignored in event_loop.
    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            FocusPane::Composer => FocusPane::Transcript,
            FocusPane::Transcript => FocusPane::History,
            FocusPane::History => {
                if self.detail.is_some() {
                    FocusPane::Detail
                } else {
                    FocusPane::Composer
                }
            }
            FocusPane::Detail => FocusPane::Composer,
        };
    }

    // ── Scroll ────────────────────────────────────────────────────────────────

    pub fn scroll_up(&mut self) {
        match self.focus {
            FocusPane::Transcript => { self.scroll.transcript.scroll_up(3); }
            FocusPane::History => self.history_up(),
            FocusPane::Detail => { self.scroll.detail.scroll_up(3); }
            _ => {}
        }
    }

    pub fn scroll_down(&mut self) {
        use super::services::transcript::count_transcript_lines;
        match self.focus {
            FocusPane::Transcript => {
                let max_w = self.scroll.transcript.area.width.saturating_sub(4) as usize;
                let total = count_transcript_lines(&self.session().transcript, max_w.max(20));
                let height = self.scroll.transcript.area.height.saturating_sub(2) as usize;
                self.scroll.transcript.scroll_down(3, total, height);
            }
            FocusPane::History => self.history_down(),
            FocusPane::Detail => {
                let height = self.scroll.detail.area.height.saturating_sub(2) as usize;
                self.scroll.detail.scroll_down(3, 50, height);
            }
            _ => {}
        }
    }

    // ── History navigation ────────────────────────────────────────────────────

    pub fn history_up(&mut self) {
        let history = self.session().history.clone();
        self.history.select_prev(&history);
        if let Some(i) = self.history.selected() {
            self.detail = DetailMode::TaskDetail(i);
            self.keep_history_visible(i);
        }
    }

    pub fn history_down(&mut self) {
        let history = self.session().history.clone();
        self.history.select_next(&history);
        if let Some(i) = self.history.selected() {
            self.detail = DetailMode::TaskDetail(i);
            self.keep_history_visible(i);
        }
    }

    /// Scroll history viewport only if item `i` is outside the visible window.
    pub fn keep_history_visible(&mut self, i: usize) {
        let item_top = i * 2;
        let item_bot = item_top + 1;
        let visible_h = self.scroll.history.area.height.saturating_sub(2) as usize;
        let offset = self.scroll.history.offset;
        if item_top < offset {
            self.scroll.history.offset = item_top;
        } else if item_bot >= offset + visible_h && visible_h > 0 {
            self.scroll.history.offset = item_bot + 1 - visible_h;
        }
    }

    // ── Transcript helpers ────────────────────────────────────────────────────

    pub fn push_transcript(&mut self, label: impl Into<String>, body: impl Into<String>, color: ratatui::style::Color) {
        self.streaming_assistant = None;
        self.session_mut().transcript.push(TranscriptEntry {
            label: label.into(),
            body: body.into(),
            color,
        });
        self.trim_transcript();
        self.scroll.transcript.pin_to_bottom(); // auto-scroll to bottom
    }

    pub fn append_assistant_chunk(&mut self, chunk: &str) {
        if self.streaming_assistant.is_none() {
            self.session_mut().transcript.push(TranscriptEntry {
                label: "Assistant".to_string(),
                body: String::new(),
                color: ratatui::style::Color::Green,
            });
            self.streaming_assistant = Some(self.session().transcript.len() - 1);
            self.trim_transcript();
        }
        if let Some(idx) = self.streaming_assistant {
            if let Some(e) = self.session_mut().transcript.get_mut(idx) {
                e.body.push_str(chunk);
            }
        }
        self.scroll.transcript.pin_to_bottom();
    }

    pub fn finish_streaming(&mut self) { self.streaming_assistant = None; }

    fn trim_transcript(&mut self) {
        let t = &mut self.session_mut().transcript;
        if t.len() > 240 {
            let overflow = t.len() - 240;
            t.drain(0..overflow);
            if let Some(idx) = self.streaming_assistant {
                self.streaming_assistant = idx.checked_sub(overflow);
            }
        }
    }

    // ── Lane telemetry helpers ────────────────────────────────────────────────

    pub fn push_thinking(&mut self, msg: impl Into<String>) {
        push_lane(&mut self.session_mut().thinking_log, msg.into());
    }
    pub fn push_reading(&mut self, msg: impl Into<String>) {
        push_lane(&mut self.session_mut().reading_log, msg.into());
    }
    pub fn push_commands(&mut self, msg: impl Into<String>) {
        push_lane(&mut self.session_mut().commands_log, msg.into());
    }

    pub fn set_activity(&mut self, phase: impl Into<String>, detail: impl Into<String>) {
        self.current_phase = phase.into();
        self.last_activity = detail.into();
    }

    // ── Revert ────────────────────────────────────────────────────────────────

    pub fn revert_selected(&mut self, project_root: &Path) {
        let Some(idx) = self.history.selected() else { return };
        let Some(entry) = self.session().history.get(idx) else { return };
        let task_id = entry.task_id.to_string();
        
        // Use SnapshotManifest API instead of manual scan
        let manifest_dir = project_root.join(".vac/snapshots");
        let manifest_path = manifest_dir.join(format!("{}.manifest.json", task_id));
        
        if !manifest_path.exists() {
            self.push_transcript("Revert", format!("No snapshot for task {}", &task_id[..8]), ratatui::style::Color::Red);
            self.detail = DetailMode::None;
            return;
        }
        
        match vac_core::snapshot::SnapshotManifest::load(&manifest_path) {
            Ok(manifest) => {
                match manifest.restore() {
                    Ok(report) => {
                        let msg = if report.failed.is_empty() {
                            format!("Reverted {} file(s) to before task {}", report.restored_count, &task_id[..8])
                        } else {
                            format!("Reverted {} file(s), {} failed", report.restored_count, report.failed.len())
                        };
                        self.push_transcript("Revert", msg, ratatui::style::Color::Magenta);
                    }
                    Err(e) => {
                        self.push_transcript("Revert", format!("Restore failed: {}", e), ratatui::style::Color::Red);
                    }
                }
            }
            Err(e) => {
                self.push_transcript("Revert", format!("Failed to load manifest: {}", e), ratatui::style::Color::Red);
            }
        }
        
        self.detail = DetailMode::None;
        self.focus = FocusPane::Transcript;
    }

    /// Save TUI state to session metadata (for future restore).
    /// TODO: Wire to session save on exit.
    pub fn save_tui_state(&self) -> (Option<usize>, Option<usize>, Option<String>) {
        let active_tab = Some(self.active_session);
        let history_sel = self.history.selected();
        let last_focus = Some(format!("{:?}", self.focus));
        (active_tab, history_sel, last_focus)
    }
}

fn push_lane(log: &mut Vec<String>, msg: String) {
    log.push(msg);
    if log.len() > 24 { let n = log.len() - 24; log.drain(0..n); }
}
