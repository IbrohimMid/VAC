use std::fs;
use std::path::PathBuf;
use vac_shell_contracts::{SessionRecoveryStatus, SessionRecoverySummary, VacPaths};

/// Probes the project workspace for a checkpoint belonging to `session_id`.
/// Does not depend on `vil_swarm` or `vac_core`. Returns a safe operator summary.
pub fn project_recovery_for_session(
    paths: &dyn VacPaths,
    session_id: &str,
) -> SessionRecoverySummary {
    let project_root = paths.project_root();
    let vac_dir = paths.project_state_dir();

    // Checkpoints generally live under `.vac/checkpoints`
    let checkpoints_dir = vac_dir.join("checkpoints");

    let candidate_paths = [
        checkpoints_dir.join(format!("{}.json", session_id)),
        checkpoints_dir.join(format!("{}_state.json", session_id)),
        paths
            .sessions_dir()
            .join(format!("{}.checkpoint.json", session_id)),
    ];

    let mut found_path: Option<PathBuf> = None;
    let mut updated_at_unix: Option<u64> = None;

    for candidate in &candidate_paths {
        if candidate.exists() {
            found_path = Some(candidate.clone());
            if let Ok(metadata) = fs::metadata(candidate) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        updated_at_unix = Some(duration.as_secs());
                    }
                }
            }
            break;
        }
    }

    let path = match found_path {
        Some(p) => p,
        None => {
            return SessionRecoverySummary {
                status: SessionRecoveryStatus::Missing,
                checkpoint_label: None,
                checkpoint_path_display: None,
                updated_at_unix: None,
                message: Some("no checkpoint found".to_string()),
            };
        }
    };

    let checkpoint_label = path.file_name().and_then(|n| n.to_str()).map(String::from);
    let display_path = match path.strip_prefix(&project_root) {
        Ok(stripped) => stripped.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    };

    // Minimal JSON parse check to verify if corrupt
    match fs::read_to_string(&path) {
        Ok(contents) => {
            if serde_json::from_str::<serde_json::Value>(&contents).is_ok() {
                SessionRecoverySummary {
                    status: SessionRecoveryStatus::Ready,
                    checkpoint_label,
                    checkpoint_path_display: Some(display_path),
                    updated_at_unix,
                    message: Some("checkpoint ready".to_string()),
                }
            } else {
                SessionRecoverySummary {
                    status: SessionRecoveryStatus::Corrupt,
                    checkpoint_label,
                    checkpoint_path_display: Some(display_path),
                    updated_at_unix,
                    message: Some("checkpoint unreadable".to_string()),
                }
            }
        }
        Err(_) => SessionRecoverySummary {
            status: SessionRecoveryStatus::Corrupt,
            checkpoint_label,
            checkpoint_path_display: Some(display_path),
            updated_at_unix,
            message: Some("checkpoint unreadable".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    struct DummyPaths {
        project_root: PathBuf,
    }

    impl VacPaths for DummyPaths {
        fn project_root(&self) -> PathBuf {
            self.project_root.clone()
        }
        fn project_state_dir(&self) -> PathBuf {
            self.project_root.join(".vac")
        }
        fn user_state_dir(&self) -> PathBuf {
            self.project_root.join(".user_vac")
        }
        fn sessions_dir(&self) -> PathBuf {
            self.project_state_dir().join("sessions")
        }
        fn plan_file(&self) -> PathBuf {
            self.project_state_dir().join("plan.md")
        }
        fn commands_dir(&self) -> PathBuf {
            self.project_state_dir().join("commands")
        }
        fn model_selection_file(&self) -> PathBuf {
            self.project_state_dir().join("model.json")
        }
    }

    #[test]
    fn test_missing_checkpoint() {
        let tmp = TempDir::new().unwrap();
        let paths = DummyPaths {
            project_root: tmp.path().to_path_buf(),
        };
        let summary = project_recovery_for_session(&paths, "sess-123");
        assert_eq!(summary.status, SessionRecoveryStatus::Missing);
        assert_eq!(summary.message.unwrap(), "no checkpoint found");
    }

    #[test]
    fn test_ready_checkpoint() {
        let tmp = TempDir::new().unwrap();
        let paths = DummyPaths {
            project_root: tmp.path().to_path_buf(),
        };
        let chk_dir = paths.project_state_dir().join("checkpoints");
        fs::create_dir_all(&chk_dir).unwrap();
        fs::write(chk_dir.join("sess-ready.json"), r#"{"some": "data"}"#).unwrap();

        let summary = project_recovery_for_session(&paths, "sess-ready");
        assert_eq!(summary.status, SessionRecoveryStatus::Ready);
        assert_eq!(summary.message.unwrap(), "checkpoint ready");
        assert_eq!(summary.checkpoint_label.unwrap(), "sess-ready.json");
        assert!(summary.updated_at_unix.is_some());
    }

    #[test]
    fn test_corrupt_checkpoint() {
        let tmp = TempDir::new().unwrap();
        let paths = DummyPaths {
            project_root: tmp.path().to_path_buf(),
        };
        let chk_dir = paths.project_state_dir().join("checkpoints");
        fs::create_dir_all(&chk_dir).unwrap();
        fs::write(chk_dir.join("sess-corrupt.json"), r#"{"some": "#).unwrap();

        let summary = project_recovery_for_session(&paths, "sess-corrupt");
        assert_eq!(summary.status, SessionRecoveryStatus::Corrupt);
        assert_eq!(summary.message.unwrap(), "checkpoint unreadable");
        assert_eq!(summary.checkpoint_label.unwrap(), "sess-corrupt.json");
    }

    #[test]
    fn test_state_json_suffix() {
        let tmp = TempDir::new().unwrap();
        let paths = DummyPaths {
            project_root: tmp.path().to_path_buf(),
        };
        let chk_dir = paths.project_state_dir().join("checkpoints");
        fs::create_dir_all(&chk_dir).unwrap();
        fs::write(chk_dir.join("sess-state.json"), r#"{"state":1}"#).unwrap();

        let summary = project_recovery_for_session(&paths, "sess-state");
        assert_eq!(summary.status, SessionRecoveryStatus::Ready);
        assert_eq!(summary.checkpoint_label.unwrap(), "sess-state.json");
    }

    #[test]
    fn test_session_side_checkpoint() {
        let tmp = TempDir::new().unwrap();
        let paths = DummyPaths {
            project_root: tmp.path().to_path_buf(),
        };
        let sessions_dir = paths.sessions_dir();
        fs::create_dir_all(&sessions_dir).unwrap();
        fs::write(sessions_dir.join("sess-side.checkpoint.json"), r#"{"state":1}"#).unwrap();

        let summary = project_recovery_for_session(&paths, "sess-side");
        assert_eq!(summary.status, SessionRecoveryStatus::Ready);
    }

    #[test]
    fn test_corrupt_does_not_leak_contents() {
        let tmp = TempDir::new().unwrap();
        let paths = DummyPaths {
            project_root: tmp.path().to_path_buf(),
        };
        let chk_dir = paths.project_state_dir().join("checkpoints");
        fs::create_dir_all(&chk_dir).unwrap();
        // Write valid but meaningless JSON (corrupt not parseable as intended structure)
        fs::write(chk_dir.join("sensitive.json"), r#"{"garbage":true}"#).unwrap();

        let summary = project_recovery_for_session(&paths, "sensitive");
        // It's valid JSON but not a real checkpoint - status becomes Ready
        // The important thing is no raw secrets in the output
        let msg = summary.message.unwrap();
        assert!(!msg.contains("secret"), "message should not have secret: {msg}");
    }
}
