//! Consolidator — lock-protected, time-gated, session-count-gated
//! driver that fires registered [`ConsolidationPolicy`] instances and
//! writes their proposals into the memdir.
//!
//! The lockfile lives at `<root>/.vac/memory/.consolidator.lock`. It
//! serializes writers across processes; a stale lock (missing pid) is
//! auto-recovered, a live lock held by another pid fails fast.

use std::path::PathBuf;
use std::time::Duration;

use tokio::fs;
use tracing::{info, warn};

use crate::error::{MemoryError, MemoryResult};
use crate::policy::{ConsolidationInput, PolicySet};
use crate::report::{ConsolidationReport, WrittenFile};
use crate::scanner::MemoryScanner;

/// Policy knobs. Construct via [`Default`] for the sensible defaults.
#[derive(Debug, Clone)]
pub struct ConsolidatorConfig {
    /// Don't run unless at least this many sessions have accumulated
    /// since the last successful run. `0` disables the gate.
    pub min_session_count: u32,
    /// Don't run unless this wall-clock elapsed since the last
    /// successful run. `Duration::ZERO` disables the gate.
    pub min_interval: Duration,
    /// If a stale lockfile is older than this, reclaim it rather than
    /// fail. Prevents a crashed prior run from wedging the cadence.
    pub stale_lock_after: Duration,
}

impl Default for ConsolidatorConfig {
    fn default() -> Self {
        Self {
            min_session_count: 3,
            min_interval: Duration::from_secs(60 * 30), // 30 min
            stale_lock_after: Duration::from_secs(60 * 60 * 6), // 6 hrs
        }
    }
}

/// Gate evaluation outcome — explains why the consolidator did or
/// didn't run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsolidatorGate {
    Run,
    SkippedCooldown,
    SkippedInsufficientSessions,
    SkippedLockHeld,
}

/// The consolidator.
#[derive(Debug)]
pub struct Consolidator {
    scanner: MemoryScanner,
    config: ConsolidatorConfig,
}

impl Consolidator {
    pub fn new(scanner: MemoryScanner, config: ConsolidatorConfig) -> Self {
        Self { scanner, config }
    }

    pub fn scanner(&self) -> &MemoryScanner {
        &self.scanner
    }

    fn lock_path(&self) -> PathBuf {
        self.scanner.root().join(".consolidator.lock")
    }

    fn stamp_path(&self) -> PathBuf {
        self.scanner.root().join(".consolidator.stamp")
    }

    /// Evaluate gates without firing. Useful for drivers that want to
    /// tell operators "next run in N minutes".
    pub async fn evaluate_gate(
        &self,
        input: &ConsolidationInput,
    ) -> MemoryResult<ConsolidatorGate> {
        if input.session_count < self.config.min_session_count {
            return Ok(ConsolidatorGate::SkippedInsufficientSessions);
        }
        if self.cooldown_active().await? {
            return Ok(ConsolidatorGate::SkippedCooldown);
        }
        if self.lock_held_by_other().await? {
            return Ok(ConsolidatorGate::SkippedLockHeld);
        }
        Ok(ConsolidatorGate::Run)
    }

    async fn cooldown_active(&self) -> MemoryResult<bool> {
        if self.config.min_interval.is_zero() {
            return Ok(false);
        }
        let path = self.stamp_path();
        match fs::metadata(&path).await {
            Ok(m) => {
                let modified = m.modified()?;
                let elapsed = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or(Duration::ZERO);
                Ok(elapsed < self.config.min_interval)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn lock_held_by_other(&self) -> MemoryResult<bool> {
        let path = self.lock_path();
        match fs::read_to_string(&path).await {
            Ok(s) => {
                let meta = fs::metadata(&path).await?;
                let age = std::time::SystemTime::now()
                    .duration_since(meta.modified()?)
                    .unwrap_or(Duration::ZERO);
                if age >= self.config.stale_lock_after {
                    let _ = fs::remove_file(&path).await;
                    return Ok(false);
                }
                let other_pid = s.trim().parse::<u32>().unwrap_or(0);
                let me = std::process::id();
                Ok(other_pid != 0 && other_pid != me)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn acquire_lock(&self) -> MemoryResult<LockGuard> {
        self.scanner.ensure_layout().await?;
        let path = self.lock_path();
        if self.lock_held_by_other().await? {
            let pid = fs::read_to_string(&path)
                .await
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0);
            return Err(MemoryError::Locked(pid));
        }
        let tmp = path.with_extension("lock.tmp");
        fs::write(&tmp, std::process::id().to_string().as_bytes()).await?;
        fs::rename(&tmp, &path).await?;
        Ok(LockGuard { path })
    }

    async fn write_stamp(&self) -> MemoryResult<()> {
        let path = self.stamp_path();
        let tmp = path.with_extension("stamp.tmp");
        fs::write(&tmp, chrono::Utc::now().to_rfc3339().as_bytes()).await?;
        fs::rename(&tmp, &path).await?;
        Ok(())
    }

    /// Run one consolidation cycle. Returns a report even when gates
    /// fire (the report's `skipped_reason` explains).
    pub async fn run_once(
        &self,
        policies: &PolicySet,
        input: &ConsolidationInput,
    ) -> MemoryResult<ConsolidationReport> {
        let started_at = chrono::Utc::now();
        let gate = self.evaluate_gate(input).await?;
        if gate != ConsolidatorGate::Run {
            return Ok(ConsolidationReport {
                started_at,
                finished_at: chrono::Utc::now(),
                policies_fired: Vec::new(),
                files_written: Vec::new(),
                skipped_reason: Some(gate_reason(&gate)),
            });
        }

        let _guard = self.acquire_lock().await?;
        let mut fired = Vec::new();
        let mut written = Vec::new();
        for policy in policies.iter() {
            let proposals = match policy.propose(input).await {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        target: "vac_memory",
                        policy = %policy.name(),
                        error = %e,
                        "policy failed; skipping",
                    );
                    continue;
                }
            };
            if proposals.is_empty() {
                continue;
            }
            fired.push(policy.name().to_string());
            for prop in proposals {
                let mem = crate::memdir::Memory {
                    kind: prop.kind,
                    path: PathBuf::new(),
                    frontmatter: prop.frontmatter,
                    body: prop.body,
                };
                let topic = mem.frontmatter.topic.clone();
                let written_mem = self.scanner.write(mem).await?;
                written.push(WrittenFile {
                    policy: policy.name().to_string(),
                    topic,
                    kind: written_mem.kind,
                    path: written_mem.path,
                });
            }
        }
        self.write_stamp().await?;
        info!(
            target: "vac_memory",
            policies = fired.len(),
            files = written.len(),
            "consolidator cycle complete",
        );
        Ok(ConsolidationReport {
            started_at,
            finished_at: chrono::Utc::now(),
            policies_fired: fired,
            files_written: written,
            skipped_reason: None,
        })
    }
}

fn gate_reason(g: &ConsolidatorGate) -> String {
    match g {
        ConsolidatorGate::Run => "no skip".into(),
        ConsolidatorGate::SkippedCooldown => "cooldown not elapsed".into(),
        ConsolidatorGate::SkippedInsufficientSessions => {
            "not enough sessions since last run".into()
        }
        ConsolidatorGate::SkippedLockHeld => "lock held by another writer".into(),
    }
}

/// RAII guard that removes the lockfile on drop.
struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // Best effort — if the lock file is already gone we don't care.
        let _ = std::fs::remove_file(&self.path);
    }
}

impl LockGuard {
    #[cfg(test)]
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::builtin_policy_set;

    fn config() -> ConsolidatorConfig {
        ConsolidatorConfig {
            min_session_count: 0,
            min_interval: Duration::ZERO,
            stale_lock_after: Duration::from_secs(60),
        }
    }

    #[tokio::test]
    async fn run_once_writes_files_for_matching_input() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let c = Consolidator::new(scanner, config());
        let input = ConsolidationInput {
            raw_lines: vec![
                "learn: use nextest, not cargo test".into(),
                "panic: runtime OOM during ingest".into(),
                "review-thread: unresolved — refactor vil_llm".into(),
            ],
            session_count: 10,
        };
        let rep = c.run_once(&builtin_policy_set(), &input).await.unwrap();
        assert!(!rep.was_skipped());
        assert!(!rep.files_written.is_empty());
        assert!(rep.policies_fired.len() >= 2);
        // Stamp exists after a run.
        assert!(tokio::fs::try_exists(c.stamp_path()).await.unwrap());
    }

    #[tokio::test]
    async fn cooldown_skips_second_run() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let mut cfg = config();
        cfg.min_interval = Duration::from_secs(3600);
        let c = Consolidator::new(scanner, cfg);
        let input = ConsolidationInput {
            raw_lines: vec!["learn: x".into()],
            session_count: 10,
        };
        let first = c.run_once(&builtin_policy_set(), &input).await.unwrap();
        assert!(!first.was_skipped());
        let second = c.run_once(&builtin_policy_set(), &input).await.unwrap();
        assert!(second.was_skipped());
        assert!(second.skipped_reason.unwrap().contains("cooldown"));
    }

    #[tokio::test]
    async fn insufficient_sessions_skips() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let mut cfg = config();
        cfg.min_session_count = 5;
        let c = Consolidator::new(scanner, cfg);
        let rep = c
            .run_once(
                &builtin_policy_set(),
                &ConsolidationInput {
                    raw_lines: vec!["learn: x".into()],
                    session_count: 1,
                },
            )
            .await
            .unwrap();
        assert!(rep.was_skipped());
        assert!(
            rep.skipped_reason
                .unwrap()
                .contains("sessions"),
        );
    }

    #[tokio::test]
    async fn lock_held_by_other_skips() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let c = Consolidator::new(scanner, config());
        // Plant a lockfile with a different-looking pid.
        let fake_pid = std::process::id().wrapping_add(7);
        tokio::fs::write(c.lock_path(), fake_pid.to_string()).await.unwrap();
        let gate = c
            .evaluate_gate(&ConsolidationInput {
                raw_lines: vec![],
                session_count: 10,
            })
            .await
            .unwrap();
        assert_eq!(gate, ConsolidatorGate::SkippedLockHeld);
    }

    #[tokio::test]
    async fn stale_lock_is_reclaimed() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let mut cfg = config();
        cfg.stale_lock_after = Duration::from_millis(1);
        let c = Consolidator::new(scanner, cfg);
        let fake_pid = std::process::id().wrapping_add(11);
        tokio::fs::write(c.lock_path(), fake_pid.to_string()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let gate = c
            .evaluate_gate(&ConsolidationInput {
                raw_lines: vec![],
                session_count: 10,
            })
            .await
            .unwrap();
        assert_eq!(gate, ConsolidatorGate::Run);
    }

    #[tokio::test]
    async fn lock_guard_drops_lockfile() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let c = Consolidator::new(scanner, config());
        let lock_path = {
            let g = c.acquire_lock().await.unwrap();
            let p = g.path().to_path_buf();
            assert!(tokio::fs::try_exists(&p).await.unwrap());
            p
            // drop guard
        };
        // After drop the file should be gone.
        assert!(!tokio::fs::try_exists(&lock_path).await.unwrap());
    }
}
