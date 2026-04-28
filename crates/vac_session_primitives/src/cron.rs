//! C.1 — Cron storage + Create/Delete/List tools.
//!
//! Operators + the agent itself register recurring tasks via
//! [`CronCreate`]; each entry persists to
//! `<project_root>/.vac/cron.json` via atomic rename. A runtime
//! loop (Phase C.2) polls every 30 s, dispatches due jobs via
//! [`crate::SubagentRunner`] using the stored prompt + subagent
//! kind, and updates `last_fire_unix + fire_count`.
//!
//! Schedule syntax is the 6-field cron form the `cron` crate
//! parses (`sec min hour day month day_of_week`). Hand-written
//! entries use the `cron` crate directly; the tool input accepts
//! a pre-validated string so the tool layer surfaces parse errors
//! eagerly.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use cron::Schedule;
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, EngineResult};

pub const DEFAULT_CRON_FILENAME: &str = "cron.json";

/// A single cron entry. Survives across sessions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CronEntry {
    pub id: String,
    pub name: String,
    /// 6-field cron expression.
    pub schedule: String,
    /// Prompt the subagent sees.
    pub prompt: String,
    /// SubagentKind label (matches `SubagentKind::label`).
    pub subagent_type: String,
    pub created_at_unix: i64,
    #[serde(default)]
    pub last_fire_unix: i64,
    #[serde(default)]
    pub fire_count: u64,
    /// Short operator-readable description.
    #[serde(default)]
    pub description: String,
}

impl CronEntry {
    /// Parse + validate the schedule at construction so bad
    /// expressions error eagerly rather than silently never firing.
    pub fn validate_schedule(expr: &str) -> EngineResult<Schedule> {
        Schedule::from_str(expr)
            .map_err(|e| EngineError::Other(format!("invalid cron expression '{expr}': {e}")))
    }

    /// Compute the next fire time (unix seconds) after `from_unix`.
    /// Returns None when the schedule has no future matches (past
    /// end_date bounds or malformed), in which case callers treat
    /// the entry as dormant.
    pub fn next_fire_unix(&self, from_unix: i64) -> Option<i64> {
        let schedule = Schedule::from_str(&self.schedule).ok()?;
        let from = chrono::DateTime::<chrono::Utc>::from_timestamp(from_unix, 0)?;
        schedule.after(&from).next().map(|dt| dt.timestamp())
    }
}

/// Persisted collection of cron entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CronStore {
    pub entries: Vec<CronEntry>,
}

impl CronStore {
    pub fn resolve_path(project_root: &Path) -> PathBuf {
        project_root.join(".vac").join(DEFAULT_CRON_FILENAME)
    }

    pub async fn load(project_root: &Path) -> EngineResult<Self> {
        let path = Self::resolve_path(project_root);
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Self::default());
        }
        let raw = tokio::fs::read_to_string(&path).await?;
        if raw.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(&raw).map_err(EngineError::from)
    }

    /// Atomic save: temp + rename so a crash mid-write never
    /// leaves a truncated cron.json.
    pub async fn save(&self, project_root: &Path) -> EngineResult<()> {
        let path = Self::resolve_path(project_root);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension("json.tmp");
        let raw = serde_json::to_vec_pretty(self)?;
        tokio::fs::write(&tmp, raw).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }

    pub fn create(&mut self, entry: CronEntry) -> EngineResult<()> {
        if self.entries.iter().any(|e| e.id == entry.id) {
            return Err(EngineError::Other(format!(
                "cron id '{}' already registered",
                entry.id
            )));
        }
        CronEntry::validate_schedule(&entry.schedule)?;
        self.entries.push(entry);
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < before
    }

    pub fn list(&self) -> &[CronEntry] {
        &self.entries
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut CronEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Filter entries whose `next_fire_unix(from)` is <= `now`.
    /// Used by the Phase C.2 runtime loop each tick.
    pub fn due(&self, now_unix: i64) -> Vec<&CronEntry> {
        self.entries
            .iter()
            .filter(|e| {
                // Compute next fire *after* max(last_fire, created). The
                // subtraction below gates against double-firing a job
                // that just fired in this tick window.
                let from = e.last_fire_unix.max(e.created_at_unix);
                match e.next_fire_unix(from) {
                    Some(next) => next <= now_unix,
                    None => false,
                }
            })
            .collect()
    }
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> CronEntry {
        CronEntry {
            id: id.to_string(),
            name: format!("test-{id}"),
            schedule: "0 * * * * *".to_string(), // every minute at :00
            prompt: "check logs".to_string(),
            subagent_type: "explore".to_string(),
            created_at_unix: 1_700_000_000,
            last_fire_unix: 0,
            fire_count: 0,
            description: "sample job".to_string(),
        }
    }

    #[test]
    fn validate_schedule_rejects_garbage() {
        assert!(CronEntry::validate_schedule("not a cron expr").is_err());
        assert!(CronEntry::validate_schedule("0 * * * * *").is_ok());
    }

    #[test]
    fn create_rejects_duplicate_id() {
        let mut s = CronStore::default();
        s.create(sample("a")).unwrap();
        assert!(s.create(sample("a")).is_err());
    }

    #[test]
    fn delete_returns_true_when_present() {
        let mut s = CronStore::default();
        s.create(sample("a")).unwrap();
        assert!(s.delete("a"));
        assert!(!s.delete("a"));
        assert!(s.entries.is_empty());
    }

    #[tokio::test]
    async fn save_then_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = CronStore::default();
        s.create(sample("a")).unwrap();
        s.save(tmp.path()).await.unwrap();
        let back = CronStore::load(tmp.path()).await.unwrap();
        assert_eq!(back.entries, s.entries);
    }

    #[tokio::test]
    async fn load_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let s = CronStore::load(tmp.path()).await.unwrap();
        assert!(s.entries.is_empty());
    }

    #[test]
    fn due_picks_up_past_jobs() {
        let mut s = CronStore::default();
        s.create(sample("a")).unwrap();
        // 1e10 seconds from now — definitely past any "every minute" fire.
        let due = s.due(1_700_000_000 + 120);
        assert_eq!(due.len(), 1);
    }
}
