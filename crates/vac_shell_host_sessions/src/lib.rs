use std::sync::{Arc, Mutex};

use vac_shell_contracts::{
    SessionAction, SessionEntry, SessionPreview, SessionRecoverySummary, VacPaths,
};
use vac_shell_host_paths::enumerate_sessions;

#[derive(Debug, thiserror::Error)]
pub enum SessionsError {
    #[error("session id not found: {0}")]
    NotFound(String),
    #[error("io error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Default)]
pub struct SessionsState {
    inner: Arc<Mutex<SessionsStateInner>>,
}

#[derive(Debug, Default)]
struct SessionsStateInner {
    pending_resume: Option<String>,
    archived: Vec<String>,
    deleted: Vec<String>,
}

impl SessionsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn list(&self, paths: &dyn VacPaths) -> Vec<SessionEntry> {
        enumerate_sessions(paths)
    }

    pub fn list_with_summaries(
        &self,
        paths: &dyn VacPaths,
        summarize: impl Fn(&std::path::Path) -> Option<vac_shell_contracts::SessionToolSummary>,
    ) -> Vec<vac_shell_contracts::SessionTileView> {
        let entries = enumerate_sessions(paths);
        entries
            .into_iter()
            .map(|entry| {
                let jsonl = paths.sessions_dir().join(format!("{}.jsonl", entry.id));
                let tool_summary = summarize(&jsonl);
                vac_shell_contracts::SessionTileView {
                    entry,
                    tool_summary,
                    tool_details: Vec::new(),
                    recovery: None,
                }
            })
            .collect()
    }

    pub fn list_with_tool_use(
        &self,
        paths: &dyn VacPaths,
        project: impl Fn(&std::path::Path) -> Option<vac_shell_contracts::SessionToolUseSurface>,
    ) -> Vec<vac_shell_contracts::SessionTileView> {
        let entries = enumerate_sessions(paths);
        entries
            .into_iter()
            .map(|entry| {
                let jsonl = paths.sessions_dir().join(format!("{}.jsonl", entry.id));
                let (tool_summary, tool_details) = match project(&jsonl) {
                    Some(surface) => (Some(surface.summary), surface.calls),
                    None => (None, Vec::new()),
                };
                vac_shell_contracts::SessionTileView {
                    entry,
                    tool_summary,
                    tool_details,
                    recovery: None,
                }
            })
            .collect()
    }

    pub fn list_with_tool_use_and_recovery(
        &self,
        paths: &dyn VacPaths,
        project_tool_use: impl Fn(
            &std::path::Path,
        ) -> Option<vac_shell_contracts::SessionToolUseSurface>,
        project_recovery: impl Fn(&str) -> Option<vac_shell_contracts::SessionRecoverySummary>,
    ) -> Vec<vac_shell_contracts::SessionTileView> {
        let entries = enumerate_sessions(paths);
        entries
            .into_iter()
            .map(|entry| {
                let jsonl = paths.sessions_dir().join(format!("{}.jsonl", entry.id));
                let (tool_summary, tool_details) = match project_tool_use(&jsonl) {
                    Some(surface) => (Some(surface.summary), surface.calls),
                    None => (None, Vec::new()),
                };
                let recovery = project_recovery(&entry.id);
                vac_shell_contracts::SessionTileView {
                    entry,
                    tool_summary,
                    tool_details,
                    recovery,
                }
            })
            .collect()
    }

    pub fn preview(
        &self,
        paths: &dyn VacPaths,
        id: &str,
        max_lines: usize,
    ) -> Result<SessionPreview, SessionsError> {
        let path = paths.sessions_dir().join(format!("{id}.jsonl"));
        let bytes = std::fs::read_to_string(&path).map_err(|e| SessionsError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let mut lines: Vec<String> = bytes
            .lines()
            .take(max_lines)
            .map(|l| l.to_string())
            .collect();
        let title = lines.first().cloned();
        if !lines.is_empty() {
            lines.remove(0);
        }
        Ok(SessionPreview {
            id: id.to_string(),
            title,
            lines,
        })
    }

    pub fn apply(&self, paths: &dyn VacPaths, action: SessionAction) -> Result<(), SessionsError> {
        match action {
            SessionAction::Open { .. } => Ok(()),
            SessionAction::Resume { id } => {
                self.ensure_exists(paths, &id)?;
                self.inner.lock().expect("sessions lock").pending_resume = Some(id);
                Ok(())
            }
            SessionAction::Archive { id } => {
                self.ensure_exists(paths, &id)?;
                let from = paths.sessions_dir().join(format!("{id}.jsonl"));
                let archive_dir = paths.sessions_dir().join("archive");
                std::fs::create_dir_all(&archive_dir).map_err(|e| SessionsError::Io {
                    path: archive_dir.display().to_string(),
                    source: e,
                })?;
                let to = archive_dir.join(format!("{id}.jsonl"));
                std::fs::rename(&from, &to).map_err(|e| SessionsError::Io {
                    path: to.display().to_string(),
                    source: e,
                })?;
                self.inner.lock().expect("sessions lock").archived.push(id);
                Ok(())
            }
            SessionAction::Delete { id } => {
                let path = paths.sessions_dir().join(format!("{id}.jsonl"));
                if !path.exists() {
                    return Err(SessionsError::NotFound(id));
                }
                std::fs::remove_file(&path).map_err(|e| SessionsError::Io {
                    path: path.display().to_string(),
                    source: e,
                })?;
                self.inner.lock().expect("sessions lock").deleted.push(id);
                Ok(())
            }
        }
    }

    fn ensure_exists(&self, paths: &dyn VacPaths, id: &str) -> Result<(), SessionsError> {
        let path = paths.sessions_dir().join(format!("{id}.jsonl"));
        if !path.exists() {
            return Err(SessionsError::NotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn take_resume_request(&self) -> Option<String> {
        self.inner
            .lock()
            .expect("sessions lock")
            .pending_resume
            .take()
    }

    pub fn archived_ids(&self) -> Vec<String> {
        self.inner.lock().expect("sessions lock").archived.clone()
    }

    pub fn deleted_ids(&self) -> Vec<String> {
        self.inner.lock().expect("sessions lock").deleted.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use vac_shell_contracts::SessionToolSummary;
    use vac_shell_test_support::{FakeVacPaths, temp_session_transcript};

    #[test]
    fn list_with_summaries_injects_summary_per_tile() {
        let tmp = tempdir().unwrap();
        temp_session_transcript(tmp.path().join("abc123.jsonl"), "{}");

        let state = SessionsState::new();
        let fake = FakeVacPaths(tmp.path().to_path_buf());
        let expected = SessionToolSummary {
            total_calls: 3,
            ok_count: 2,
            error_count: 1,
            ..Default::default()
        };
        let exp2 = expected.clone();
        let tiles = state.list_with_summaries(&fake, move |_p| Some(exp2.clone()));
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].entry.id, "abc123");
        assert_eq!(tiles[0].tool_summary, Some(expected));
    }

    #[test]
    fn list_with_summaries_none_when_closure_returns_none() {
        let tmp = tempdir().unwrap();
        temp_session_transcript(tmp.path().join("xyz.jsonl"), "{}");

        let state = SessionsState::new();
        let fake = FakeVacPaths(tmp.path().to_path_buf());
        let tiles = state.list_with_summaries(&fake, |_| None);
        assert_eq!(tiles.len(), 1);
        assert!(tiles[0].tool_summary.is_none());
    }

    #[test]
    fn list_with_tool_use_injects_details() {
        let tmp = tempdir().unwrap();
        temp_session_transcript(tmp.path().join("abc.jsonl"), "{}");

        let state = SessionsState::new();
        let fake = FakeVacPaths(tmp.path().to_path_buf());

        let exp_summary = vac_shell_contracts::SessionToolSummary {
            total_calls: 1,
            ok_count: 1,
            ..Default::default()
        };
        let detail = vac_shell_contracts::SessionToolUseDetail {
            call_id: "call-1".into(),
            tool_name: "test_tool".into(),
            status: vac_shell_contracts::ToolUseUiStatus::Ok,
            summary: "success".into(),
            duration_ms: 10,
        };
        let exp_surface = vac_shell_contracts::SessionToolUseSurface {
            summary: exp_summary.clone(),
            calls: vec![detail.clone()],
        };

        let tiles = state.list_with_tool_use(&fake, move |_| Some(exp_surface.clone()));
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].tool_summary, Some(exp_summary));
        assert_eq!(tiles[0].tool_details, vec![detail]);
    }

    #[test]
    fn list_with_tool_use_maps_none_to_empty() {
        let tmp = tempdir().unwrap();
        temp_session_transcript(tmp.path().join("def.jsonl"), "{}");

        let state = SessionsState::new();
        let fake = FakeVacPaths(tmp.path().to_path_buf());
        let tiles = state.list_with_tool_use(&fake, |_| None);
        assert_eq!(tiles.len(), 1);
        assert!(tiles[0].tool_summary.is_none());
        assert!(tiles[0].tool_details.is_empty());
    }
}
