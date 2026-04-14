//! Session management: persistence, auto-save/restore.

use crate::task::{Task, TaskId, TaskResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A VAC session representing a continuous work period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub project_root: PathBuf,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tasks: Vec<Task>,
    pub results: HashMap<TaskId, TaskResult>,
    pub metadata: SessionMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub total_tokens_used: u64,
    pub total_tasks_completed: usize,
    pub total_tasks_failed: usize,
    pub total_files_modified: usize,
    // TUI state persistence
    pub active_tab_idx: Option<usize>,
    pub history_selection: Option<usize>,
    pub last_focus: Option<String>,
}

impl Session {
    /// Create a new session for the given project root.
    pub fn new(project_root: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_root,
            created_at: now,
            updated_at: now,
            tasks: Vec::new(),
            results: HashMap::new(),
            metadata: SessionMetadata::default(),
        }
    }

    /// Save session to disk.
    pub fn save(&self) -> crate::error::VacResult<()> {
        let session_dir = self.project_root.join(".vac/sessions");
        std::fs::create_dir_all(&session_dir)?;
        let path = session_dir.join(format!("{}.json", self.id));
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load the most recent session from disk.
    pub fn load_latest(project_root: &Path) -> crate::error::VacResult<Option<Self>> {
        let session_dir = project_root.join(".vac/sessions");
        if !session_dir.exists() {
            return Ok(None);
        }

        let mut sessions: Vec<Self> = std::fs::read_dir(&session_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| {
                let content = std::fs::read_to_string(entry.path()).ok()?;
                serde_json::from_str(&content).ok()
            })
            .collect();

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions.into_iter().next())
    }

    /// List all sessions from disk, sorted by updated_at descending.
    pub fn list_all(project_root: &Path) -> crate::error::VacResult<Vec<Self>> {
        let session_dir = project_root.join(".vac/sessions");
        if !session_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions: Vec<Self> = std::fs::read_dir(&session_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| {
                let content = std::fs::read_to_string(entry.path()).ok()?;
                serde_json::from_str(&content).ok()
            })
            .collect();

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Add a task result and update metadata.
    pub fn record_result(&mut self, result: TaskResult) {
        match &result.status {
            crate::task::TaskStatus::Completed => self.metadata.total_tasks_completed += 1,
            crate::task::TaskStatus::Failed(_) => self.metadata.total_tasks_failed += 1,
            _ => {}
        }
        self.metadata.total_tokens_used += result.total_tokens_used;
        self.metadata.total_files_modified += result.modified_files.len();
        self.updated_at = Utc::now();
        self.results.insert(result.task_id, result);
    }
}
