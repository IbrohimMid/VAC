//! Step 2 slice 7 — VAC-canonical paths.
//!
//! Implements `vac_shell_contracts::VacPaths` so the donor never
//! composes a path itself. Every shell call that wanted
//! `.stakpak/...` reads from here and gets `.vac/...` instead. This
//! is the closure of Blocker A from the donor extraction map.
//!
//! Also ships an `enumerate_sessions` helper — a read-only directory
//! scan that yields `SessionEntry` DTOs for the shortcuts popup's
//! Sessions tab. Resume / delete / transcript-open flows stay out;
//! the host owns those.

use std::path::{Path, PathBuf};

use vac_shell_contracts::{SessionEntry, VacPaths};

/// Concrete `VacPaths` impl rooted at a project directory.
///
/// Layout produced (all under `<project>/.vac/`):
///
/// ```text
///   project_root        = <project>
///   project_state_dir   = <project>/.vac
///   user_state_dir      = <user>/.config/vac     (XDG; falls back to ./.vac)
///   sessions_dir        = <project>/.vac/sessions
///   plan_file           = <project>/.vac/session/plan.md
///   commands_dir        = <project>/.vac/commands
/// ```
#[derive(Debug, Clone)]
pub struct VacPathsImpl {
    project_root: PathBuf,
    user_dir_override: Option<PathBuf>,
}

impl VacPathsImpl {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            user_dir_override: None,
        }
    }

    /// Override the user-global directory. Tests use this; production
    /// callers can pass `None` and let the default XDG resolution win.
    pub fn with_user_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.user_dir_override = Some(path.into());
        self
    }

    fn resolve_user_dir(&self) -> PathBuf {
        if let Some(p) = &self.user_dir_override {
            return p.clone();
        }
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("vac");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return PathBuf::from(home).join(".config").join("vac");
            }
        }
        // Last resort — keep state inside the project dir so a
        // missing HOME never silently leaks paths to `/`.
        self.project_root.join(".vac")
    }
}

impl VacPaths for VacPathsImpl {
    fn project_root(&self) -> PathBuf {
        self.project_root.clone()
    }

    fn project_state_dir(&self) -> PathBuf {
        self.project_root.join(".vac")
    }

    fn user_state_dir(&self) -> PathBuf {
        self.resolve_user_dir()
    }

    fn sessions_dir(&self) -> PathBuf {
        self.project_root.join(".vac").join("sessions")
    }

    fn plan_file(&self) -> PathBuf {
        self.project_root.join(".vac").join("session").join("plan.md")
    }

    fn commands_dir(&self) -> PathBuf {
        self.project_root.join(".vac").join("commands")
    }
}

/// Enumerate session transcripts in `paths.sessions_dir()`.
///
/// Reads only the directory listing — does NOT load transcripts,
/// does NOT touch resume / delete state, does NOT mutate. Returns
/// entries sorted newest-first by mtime so the shortcuts popup
/// shows the most recent at the top.
///
/// Files that are not `.jsonl` are ignored. Directory missing is
/// not an error — yields an empty list, mirroring the donor's
/// "no sessions yet" path.
pub fn enumerate_sessions(paths: &dyn VacPaths) -> Vec<SessionEntry> {
    let dir = paths.sessions_dir();
    let read = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut out: Vec<SessionEntry> = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if stem.is_empty() {
            continue;
        }
        let last_active = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.push(SessionEntry {
            id: stem.clone(),
            label: humanise_label(&path, &stem),
            last_active_unix: last_active,
        });
    }
    out.sort_by(|a, b| b.last_active_unix.cmp(&a.last_active_unix));
    out
}

fn humanise_label(_path: &Path, stem: &str) -> String {
    // For the read-only list, the stem (typically a UUID) is the
    // label. The host can replace this with a transcript title in a
    // later slice once it owns transcript loading.
    stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_state_dir_is_dot_vac_not_dot_stakpak() {
        let p = VacPathsImpl::new("/tmp/proj");
        assert_eq!(p.project_state_dir(), PathBuf::from("/tmp/proj/.vac"));
        assert!(!p.project_state_dir().to_string_lossy().contains(".stakpak"));
    }

    #[test]
    fn sessions_dir_lives_under_dot_vac() {
        let p = VacPathsImpl::new("/tmp/proj");
        assert_eq!(p.sessions_dir(), PathBuf::from("/tmp/proj/.vac/sessions"));
    }

    #[test]
    fn plan_file_is_under_dot_vac_session_plan_md() {
        let p = VacPathsImpl::new("/tmp/proj");
        assert_eq!(
            p.plan_file(),
            PathBuf::from("/tmp/proj/.vac/session/plan.md")
        );
    }

    #[test]
    fn commands_dir_lives_under_dot_vac() {
        let p = VacPathsImpl::new("/tmp/proj");
        assert_eq!(p.commands_dir(), PathBuf::from("/tmp/proj/.vac/commands"));
    }

    #[test]
    fn user_state_dir_uses_override_when_set() {
        let p = VacPathsImpl::new("/tmp/proj").with_user_dir("/home/test/.config/vac");
        assert_eq!(p.user_state_dir(), PathBuf::from("/home/test/.config/vac"));
    }
}
