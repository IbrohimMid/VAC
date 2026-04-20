//! Shell session and state types.

use uuid::Uuid;
use vac_shell::ShellCommand;

#[derive(Debug, Clone)]
pub struct ShellSession {
    pub id: Uuid,
    pub label: String,
    pub command: Option<ShellCommand>,
    pub output: String,
    pub history: Vec<String>,
    pub history_idx: Option<usize>,
    pub waiting_for_input: bool,
    pub backgrounded: bool,
    pub exit_code: Option<i32>,
    pub last_error: Option<String>,
    pub prompt_ready: bool,
    pub password_mode: bool,
    pub lifecycle: vac_shell::ShellLifecycle,
}

impl ShellSession {
    pub fn new(label: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            label,
            command: None,
            output: String::new(),
            history: Vec::new(),
            history_idx: None,
            waiting_for_input: false,
            backgrounded: false,
            exit_code: None,
            last_error: None,
            prompt_ready: false,
            password_mode: false,
            lifecycle: vac_shell::ShellLifecycle::Running,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShellSessionStore {
    pub sessions: Vec<ShellSession>,
    pub active_idx: Option<usize>,
    pub popup_visible: bool,
}

impl ShellSessionStore {
    pub fn active(&self) -> Option<&ShellSession> {
        self.active_idx.and_then(|idx| self.sessions.get(idx))
    }

    pub fn active_mut(&mut self) -> Option<&mut ShellSession> {
        self.active_idx.and_then(|idx| self.sessions.get_mut(idx))
    }

    pub fn push_new(&mut self, label: String) -> usize {
        self.sessions.push(ShellSession::new(label));
        let idx = self.sessions.len().saturating_sub(1);
        self.active_idx = Some(idx);
        idx
    }

    pub fn remove(&mut self, idx: usize) {
        if idx >= self.sessions.len() {
            return;
        }
        self.sessions.remove(idx);
        self.active_idx = match self.active_idx {
            None => None,
            Some(_) if self.sessions.is_empty() => None,
            Some(active_idx) if active_idx == idx => Some(idx.min(self.sessions.len() - 1)),
            Some(active_idx) if active_idx > idx => Some(active_idx - 1),
            Some(active_idx) => Some(active_idx),
        };
    }

    pub fn switch_to(&mut self, idx: usize) {
        if idx < self.sessions.len() {
            self.active_idx = Some(idx);
        }
    }

    pub fn find_by_command_id(&self, command_id: &str) -> Option<usize> {
        self.sessions.iter().position(|session| {
            session
                .command
                .as_ref()
                .is_some_and(|command| command.id == command_id)
        })
    }

    pub fn find_by_command_id_mut(&mut self, command_id: &str) -> Option<&mut ShellSession> {
        let idx = self.find_by_command_id(command_id)?;
        self.sessions.get_mut(idx)
    }
}

/// Shell-session domain state. Accessed via `app_state.shell`.
#[derive(Debug, Clone, Default)]
pub struct ShellState {
    pub session_store: ShellSessionStore,
}
