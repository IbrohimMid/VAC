//! Consolidator — lock-protected, time-gated, session-count-gated
//! driver that fires registered [`ConsolidationPolicy`] instances and
//! writes their proposals into the memdir.
//!
//! The lockfile lives at `<root>/.vac/memory/.consolidator.lock`. It
//! serializes writers across processes; a stale lock (missing pid) is
//! auto-recovered, a live lock held by another pid fails fast.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::fs;
use tracing::{info, warn};

/// Per-process nonce so concurrent writers (same pid, different tasks)
/// never collide on a temp filename when renaming into place.
fn next_nonce() -> u64 {
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Build a unique staging path suffix: `<pid>.<nanos>.<nonce>.tmp`.
pub(crate) fn tmp_suffix() -> String {
    let pid = std::process::id();
    let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let nonce = next_nonce();
    format!("{pid}.{nanos}.{nonce}.tmp")
}

use crate::error::{MemoryError, MemoryResult};
use crate::policy::{ConsolidationInput, PolicySet};
use crate::report::{ConsolidationReport, ConsolidatorPhase, WrittenFile};
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
#[non_exhaustive]
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

#[derive(Debug, Clone)]
pub struct OrientedContext {
    pub active_memories: Vec<crate::memdir::Memory>,
}

#[derive(Debug, Clone)]
pub struct GatheredContext {
    pub input: ConsolidationInput,
    pub index_lines: Vec<String>,
}

pub struct ConsolidatedContext {
    pub written: Vec<WrittenFile>,
    pub fired: Vec<String>,
    pub failed: Vec<(String, String)>,
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

    /// Diagnostic: does a non-stale lock exist owned by someone else?
    /// Pure-read — does not reclaim. The authoritative acquire path
    /// uses `create_new` O_EXCL and performs reclaim on its own.
    async fn lock_held_by_other(&self) -> MemoryResult<bool> {
        let path = self.lock_path();
        let body = match fs::read_to_string(&path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        if self.lock_file_is_stale(&path).await? {
            return Ok(false);
        }
        let other_pid = body.trim().parse::<u32>().unwrap_or(0);
        let me = std::process::id();
        Ok(other_pid != 0 && other_pid != me)
    }

    /// Acquire the consolidator lockfile. Uses `create_new` (O_EXCL on
    /// POSIX, CREATE_NEW on Windows) as the actual mutex — the TOCTOU
    /// pattern of "check + write separately" is deliberately avoided.
    /// A stale lockfile older than `stale_lock_after` is reclaimed and
    /// the acquire retried exactly once.
    async fn acquire_lock(&self) -> MemoryResult<LockGuard> {
        self.scanner.ensure_layout().await?;
        let path = self.lock_path();
        match self.try_create_lock(&path).await {
            Ok(g) => Ok(g),
            Err(MemoryError::Locked(pid)) => {
                // Check staleness and reclaim if old enough.
                if self.lock_file_is_stale(&path).await? {
                    let _ = fs::remove_file(&path).await;
                    self.try_create_lock(&path).await
                } else {
                    Err(MemoryError::Locked(pid))
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn try_create_lock(&self, path: &std::path::Path) -> MemoryResult<LockGuard> {
        use std::io::ErrorKind;
        let result = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await;
        match result {
            Ok(mut f) => {
                use tokio::io::AsyncWriteExt;
                let body = std::process::id().to_string();
                f.write_all(body.as_bytes()).await?;
                f.flush().await?;
                f.sync_data().await?;
                Ok(LockGuard {
                    path: path.to_path_buf(),
                })
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                let pid = fs::read_to_string(path)
                    .await
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(0);
                Err(MemoryError::Locked(pid))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn lock_file_is_stale(&self, path: &std::path::Path) -> MemoryResult<bool> {
        let meta = match fs::metadata(path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(e) => return Err(e.into()),
        };
        let age = std::time::SystemTime::now()
            .duration_since(meta.modified()?)
            .unwrap_or(Duration::ZERO);
        Ok(age >= self.config.stale_lock_after)
    }

    async fn write_stamp(&self) -> MemoryResult<()> {
        let path = self.stamp_path();
        let tmp = path.with_extension(tmp_suffix());
        fs::write(&tmp, chrono::Utc::now().to_rfc3339().as_bytes()).await?;
        fs::rename(&tmp, &path).await?;
        Ok(())
    }

    /// Run one consolidation cycle. Returns a report even when gates
    /// fire (the report's `skipped_reason` explains).
    pub async fn orient(&self) -> MemoryResult<OrientedContext> {
        // Read MEMORY.md index or scan active memories
        let active_memories = self.scanner.scan_kind(crate::memdir::MemoryKind::Active).await?;
        Ok(OrientedContext { active_memories })
    }

    pub async fn gather(&self, input: &ConsolidationInput, _oriented: &OrientedContext) -> MemoryResult<GatheredContext> {
        // Scan recent transcripts. For now, we just pass the input through.
        Ok(GatheredContext {
            input: input.clone(),
            index_lines: Vec::new(),
        })
    }

    pub async fn consolidate(
        &self,
        policies: &PolicySet,
        gathered: &GatheredContext,
    ) -> MemoryResult<ConsolidatedContext> {
        let mut fired = Vec::new();
        let mut written = Vec::new();
        let mut failed: Vec<(String, String)> = Vec::new();
        for policy in policies.iter() {
            let proposals = match policy.propose(&gathered.input).await {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        target: "vac_memory",
                        policy = %policy.name(),
                        error = %e,
                        "policy failed; skipping",
                    );
                    failed.push((policy.name().to_string(), e.to_string()));
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
        Ok(ConsolidatedContext { written, fired, failed })
    }

    pub async fn prune(&self, _oriented: &OrientedContext) -> MemoryResult<usize> {
        // For now, no pruning implemented. Just return 0.
        Ok(0)
    }

    pub async fn run_phases(
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
                policies_failed: Vec::new(),
                phases_fired: Vec::new(),
                pruned: 0,
            });
        }

        let _guard = self.acquire_lock().await?;
        let oriented = self.orient().await?;
        let gathered = self.gather(input, &oriented).await?;
        let consolidated = self.consolidate(policies, &gathered).await?;
        let pruned = self.prune(&oriented).await?;

        self.write_stamp().await?;
        info!(
            target: "vac_memory",
            policies = consolidated.fired.len(),
            files = consolidated.written.len(),
            "consolidator cycle complete",
        );
        Ok(ConsolidationReport {
            started_at,
            finished_at: chrono::Utc::now(),
            policies_fired: consolidated.fired,
            files_written: consolidated.written,
            skipped_reason: None,
            policies_failed: consolidated.failed,
            phases_fired: vec![
                ConsolidatorPhase::Orient,
                ConsolidatorPhase::Gather,
                ConsolidatorPhase::Consolidate,
                ConsolidatorPhase::Prune,
            ],
            pruned,
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
        let rep = c.run_phases(&builtin_policy_set(), &input).await.unwrap();
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
        let first = c.run_phases(&builtin_policy_set(), &input).await.unwrap();
        assert!(!first.was_skipped());
        let second = c.run_phases(&builtin_policy_set(), &input).await.unwrap();
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
            .run_phases(
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
    async fn concurrent_acquire_yields_exactly_one_winner() {
        // Two tasks race to acquire the same lock on the same scanner.
        // With O_EXCL create_new as the primitive, exactly one must
        // succeed; the other must see MemoryError::Locked.
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        // Two distinct consolidator handles pointing at the same root.
        let c1 = std::sync::Arc::new(Consolidator::new(scanner.clone(), config()));
        let c2 = std::sync::Arc::new(Consolidator::new(scanner, config()));
        let a = {
            let c = c1.clone();
            tokio::spawn(async move { c.acquire_lock().await })
        };
        let b = {
            let c = c2.clone();
            tokio::spawn(async move { c.acquire_lock().await })
        };
        let (ra, rb) = tokio::join!(a, b);
        let ra = ra.unwrap();
        let rb = rb.unwrap();
        let winners = [ra.is_ok(), rb.is_ok()].iter().filter(|b| **b).count();
        let losers = [&ra, &rb]
            .iter()
            .filter(|r| matches!(r, Err(MemoryError::Locked(_))))
            .count();
        // One task can lose on "already exists"; if both happen to run
        // strictly serialized the second task still sees Locked because
        // the first guard hasn't been dropped yet (both held until join).
        assert_eq!(winners, 1, "exactly one lock winner (got {winners})");
        assert_eq!(losers, 1, "exactly one Locked loser (got {losers})");
    }

    #[tokio::test]
    async fn tmp_suffix_produces_distinct_paths() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        for _ in 0..16 {
            assert!(set.insert(super::tmp_suffix()), "duplicate tmp suffix");
        }
    }

    #[tokio::test]
    async fn policy_failure_surfaces_in_report() {
        use crate::policy::{
            ConsolidationInput, ConsolidationPolicy, ConsolidationProposal, PolicySet,
        };
        use async_trait::async_trait;

        struct BoomPolicy;
        #[async_trait]
        impl ConsolidationPolicy for BoomPolicy {
            fn name(&self) -> &str { "boom" }
            fn description(&self) -> &str { "always fails" }
            async fn propose(
                &self,
                _: &ConsolidationInput,
            ) -> MemoryResult<Vec<ConsolidationProposal>> {
                Err(MemoryError::Other("kaboom".into()))
            }
        }

        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let c = Consolidator::new(scanner, config());
        let mut policies = PolicySet::new();
        policies.register(std::sync::Arc::new(BoomPolicy));
        let rep = c
            .run_phases(
                &policies,
                &ConsolidationInput {
                    raw_lines: vec!["learn: x".into()],
                    session_count: 10,
                },
            )
            .await
            .unwrap();
        assert_eq!(rep.policies_failed.len(), 1);
        assert_eq!(rep.policies_failed[0].0, "boom");
        assert!(rep.policies_failed[0].1.contains("kaboom"));
        assert!(rep.files_written.is_empty());
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
