//! Report emitted when a consolidator run finishes. UIs render it as
//! a banner ("memory consolidator wrote 3 files across 2 policies").

use serde::{Deserialize, Serialize};

use crate::memdir::MemoryKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: chrono::DateTime<chrono::Utc>,
    pub policies_fired: Vec<String>,
    pub files_written: Vec<WrittenFile>,
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrittenFile {
    pub policy: String,
    pub topic: String,
    pub kind: MemoryKind,
    pub path: std::path::PathBuf,
}

impl ConsolidationReport {
    /// One-line banner suitable for `state.banner`.
    pub fn banner_line(&self) -> String {
        if let Some(r) = &self.skipped_reason {
            return format!("memory: skipped — {r}");
        }
        match self.files_written.len() {
            0 => "memory: no new learnings".to_string(),
            1 => format!(
                "memory: consolidated 1 file via {} policy",
                self.policies_fired.len()
            ),
            n => format!(
                "memory: consolidated {n} files across {} policies",
                self.policies_fired.len()
            ),
        }
    }

    pub fn was_skipped(&self) -> bool {
        self.skipped_reason.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(files: usize, policies: usize) -> ConsolidationReport {
        let now = chrono::Utc::now();
        ConsolidationReport {
            started_at: now,
            finished_at: now,
            policies_fired: (0..policies).map(|i| format!("p{i}")).collect(),
            files_written: (0..files)
                .map(|i| WrittenFile {
                    policy: format!("p{i}"),
                    topic: format!("t{i}"),
                    kind: MemoryKind::Active,
                    path: std::path::PathBuf::from(format!("{i}.md")),
                })
                .collect(),
            skipped_reason: None,
        }
    }

    #[test]
    fn banner_reflects_counts() {
        assert!(rep(0, 0).banner_line().contains("no new"));
        assert!(rep(1, 1).banner_line().contains("1 file"));
        assert!(rep(3, 2).banner_line().contains("3 files"));
    }

    #[test]
    fn skipped_reason_wins_over_counts() {
        let mut r = rep(5, 2);
        r.skipped_reason = Some("lock held".into());
        assert!(r.banner_line().contains("skipped"));
        assert!(r.was_skipped());
    }
}
