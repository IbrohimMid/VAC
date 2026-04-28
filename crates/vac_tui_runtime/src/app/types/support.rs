//! Lightweight support types used by `AppState` and the wider TUI runtime.

use chrono::{DateTime, Utc};

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
    pub sandbox_mode: vac_core::config::UserSandboxMode,
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
            sandbox_mode: vac_core::config::UserSandboxMode::DangerFullAccess,
            kitty_graphics: false,
        }
    }
}
