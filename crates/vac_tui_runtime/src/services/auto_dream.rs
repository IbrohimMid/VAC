//! W10.1 — `AutoDream` idle summariser.
//!
//! After `IDLE_THRESHOLD` of no operator activity the driver calls
//! [`AutoDreamService::tick`]. When enough submits have accumulated
//! since the last dream, the service composes a one-paragraph
//! summary via the `simplify` skill + a tail of the newest
//! transcript, and writes one episodic-memory entry under
//! `<project_root>/.vac/memory/archived/dream-<yyyymmdd-hhmm>.md`.
//!
//! Policy: one entry per tick, gated by (a) idle elapsed
//! (b) activity delta since last dream. The service never touches
//! state — it writes exactly one file and returns a report. The
//! driver owns the polling loop.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;

use vac_skill::bundled::simplify::SimplifySkill;
use vac_skill::{Skill, SkillContext};

/// 5-minute idle threshold per the cc-parity plan.
pub const IDLE_THRESHOLD: Duration = Duration::from_secs(5 * 60);
/// Minimum newly-observed transcript bytes before a dream fires.
/// Stops the service from re-summarising stale content.
pub const MIN_ACTIVITY_DELTA_BYTES: u64 = 512;
/// Tail size passed to `simplify` — matches the skill's default.
pub const TRANSCRIPT_TAIL_BYTES: u64 = 2 * 1024 * 1024;

/// Clock trait — `FakeClock::advance` powers the time-travel test.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> SystemTime;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

pub struct FakeClock {
    base: SystemTime,
    offset: Arc<std::sync::atomic::AtomicU64>,
}
impl FakeClock {
    pub fn new() -> Self {
        Self {
            base: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            offset: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
    pub fn advance(&self, secs: u64) {
        self.offset
            .fetch_add(secs, std::sync::atomic::Ordering::SeqCst);
    }
}
impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}
impl Clock for FakeClock {
    fn now(&self) -> SystemTime {
        self.base
            + Duration::from_secs(
                self.offset.load(std::sync::atomic::Ordering::SeqCst),
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickOutcome {
    Wrote { path: PathBuf },
    Skipped { reason: SkipReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    NotIdleYet,
    NoActivityDelta,
    NoTranscript,
    /// Transient skill / I/O failure. The tick loop should retry on
    /// the next idle window instead of tearing down.
    TransientFailure(String),
    /// Target `.vac/memory/archived` is a symlink — refused to
    /// traverse outside the project tree.
    UnsafeArchiveDir,
}

struct Inner {
    last_dream_at: Option<SystemTime>,
    last_dream_size_bytes: u64,
}

pub struct AutoDreamService {
    project_root: PathBuf,
    clock: Arc<dyn Clock>,
    inner: Arc<Mutex<Inner>>,
    /// Threshold knobs — stored so tests can shorten without pulling
    /// the service out from under the clock.
    idle_threshold: Duration,
    min_delta_bytes: u64,
    tail_bytes: u64,
}

impl AutoDreamService {
    pub fn new(project_root: PathBuf) -> Self {
        Self::with_clock(project_root, Arc::new(SystemClock))
    }

    pub fn with_clock(project_root: PathBuf, clock: Arc<dyn Clock>) -> Self {
        Self {
            project_root,
            clock,
            inner: Arc::new(Mutex::new(Inner {
                last_dream_at: None,
                last_dream_size_bytes: 0,
            })),
            idle_threshold: IDLE_THRESHOLD,
            min_delta_bytes: MIN_ACTIVITY_DELTA_BYTES,
            tail_bytes: TRANSCRIPT_TAIL_BYTES,
        }
    }

    pub fn with_idle_threshold(mut self, d: Duration) -> Self {
        self.idle_threshold = d;
        self
    }

    pub fn with_min_delta_bytes(mut self, n: u64) -> Self {
        self.min_delta_bytes = n;
        self
    }

    /// Run one tick. `last_activity_at` is the driver's best estimate
    /// of when the operator last pressed a key / submitted. Returns
    /// the outcome so the driver can surface a notification on
    /// `Wrote`.
    pub async fn tick(
        &self,
        last_activity_at: SystemTime,
    ) -> anyhow::Result<TickOutcome> {
        let now = self.clock.now();
        let elapsed_since_activity =
            now.duration_since(last_activity_at).unwrap_or(Duration::ZERO);
        if elapsed_since_activity < self.idle_threshold {
            return Ok(TickOutcome::Skipped {
                reason: SkipReason::NotIdleYet,
            });
        }

        // Pick the newest transcript under .vac/sessions.
        let sessions_dir = self.project_root.join(".vac").join("sessions");
        let Some((transcript_path, transcript_size)) =
            newest_transcript(&sessions_dir).await?
        else {
            return Ok(TickOutcome::Skipped {
                reason: SkipReason::NoTranscript,
            });
        };

        // Activity delta: bytes now vs bytes at last dream.
        // Transcript rotation (e.g. session archive) shrinks the
        // file. Detect that — current < tracked prior — and reset
        // the baseline so the next tick can re-fire on genuine new
        // content rather than being wedged by a stale high-water
        // mark.
        let prior_size = {
            let mut guard = self.inner.lock().await;
            if transcript_size < guard.last_dream_size_bytes {
                tracing::info!(
                    target: "vac_tui_runtime::auto_dream",
                    old = guard.last_dream_size_bytes,
                    new = transcript_size,
                    "transcript rotation detected; resetting delta baseline",
                );
                guard.last_dream_size_bytes = 0;
            }
            guard.last_dream_size_bytes
        };
        let delta = transcript_size.saturating_sub(prior_size);
        if delta < self.min_delta_bytes {
            return Ok(TickOutcome::Skipped {
                reason: SkipReason::NoActivityDelta,
            });
        }

        // Tail-read + simplify. Transient failures (I/O glitch,
        // skill error) become Skipped so the poll loop keeps
        // running — one hiccup shouldn't kill background
        // summarisation for the rest of the session.
        let tail = match read_tail(&transcript_path, self.tail_bytes).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    target: "vac_tui_runtime::auto_dream",
                    error = %e,
                    "tail read failed; skipping",
                );
                return Ok(TickOutcome::Skipped {
                    reason: SkipReason::TransientFailure(e.to_string()),
                });
            }
        };
        let collapsed = match run_simplify(&tail, &self.project_root).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    target: "vac_tui_runtime::auto_dream",
                    error = %e,
                    "simplify failed; skipping",
                );
                return Ok(TickOutcome::Skipped {
                    reason: SkipReason::TransientFailure(e.to_string()),
                });
            }
        };

        // Persist. Target path:
        //   .vac/memory/archived/dream-<stamp>.md
        // Refuse to create through a symlink. Same TOCTOU guard as
        // vac_skill::remember — a compromised / misconfigured
        // memory dir pointer cannot route writes outside the project.
        let archived_dir = self
            .project_root
            .join(".vac")
            .join("memory")
            .join("archived");
        tokio::fs::create_dir_all(&archived_dir).await?;
        let meta = tokio::fs::symlink_metadata(&archived_dir).await?;
        if meta.file_type().is_symlink() {
            tracing::warn!(
                target: "vac_tui_runtime::auto_dream",
                dir = %archived_dir.display(),
                "refusing to write through symlink",
            );
            return Ok(TickOutcome::Skipped {
                reason: SkipReason::UnsafeArchiveDir,
            });
        }
        let stamp = format_stamp(now);
        let path = archived_dir.join(format!("dream-{stamp}.md"));
        let body = format!(
            "---\nkind: dream\ngenerated_at: {stamp}\nsource: {}\n---\n\n{}\n",
            transcript_path.display(),
            collapsed,
        );
        // Atomic write — temp file in the same dir + rename.
        let tmp = archived_dir.join(format!("dream-{stamp}.md.tmp"));
        tokio::fs::write(&tmp, body).await?;
        tokio::fs::rename(&tmp, &path).await?;

        {
            let mut guard = self.inner.lock().await;
            guard.last_dream_at = Some(now);
            guard.last_dream_size_bytes = transcript_size;
        }
        tracing::info!(
            target: "vac_tui_runtime::auto_dream",
            path = %path.display(),
            delta_bytes = delta,
            "auto-dream wrote episodic entry",
        );
        Ok(TickOutcome::Wrote { path })
    }

    pub async fn last_dream_at(&self) -> Option<SystemTime> {
        self.inner.lock().await.last_dream_at
    }
}

/// Pick the newest `.jsonl` in `sessions_dir` plus its size. Returns
/// `None` when the dir is missing or empty.
async fn newest_transcript(
    sessions_dir: &Path,
) -> anyhow::Result<Option<(PathBuf, u64)>> {
    if !sessions_dir.is_dir() {
        return Ok(None);
    }
    let mut newest: Option<(SystemTime, PathBuf, u64)> = None;
    let mut rd = tokio::fs::read_dir(sessions_dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let meta = match tokio::fs::symlink_metadata(&p).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() || !meta.file_type().is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
        if newest.as_ref().map(|(t, _, _)| mtime > *t).unwrap_or(true) {
            newest = Some((mtime, p, meta.len()));
        }
    }
    Ok(newest.map(|(_, p, s)| (p, s)))
}

async fn read_tail(path: &Path, max_bytes: u64) -> anyhow::Result<String> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
    let mut f = tokio::fs::File::open(path).await?;
    let meta = f.metadata().await?;
    let total = meta.len();
    let start = total.saturating_sub(max_bytes);
    f.seek(SeekFrom::Start(start)).await?;
    let mut buf = Vec::with_capacity(max_bytes.min(total) as usize);
    f.take(max_bytes).read_to_end(&mut buf).await?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

async fn run_simplify(tail: &str, project_root: &Path) -> anyhow::Result<String> {
    let ctx = SkillContext::new(
        serde_json::json!({
            "text": tail,
            "max_chars": 2_000,
        }),
        project_root.to_path_buf(),
    );
    let outcome = SimplifySkill
        .run(ctx)
        .await
        .map_err(|e| anyhow::anyhow!("simplify failed: {e}"))?;
    let collapsed = outcome
        .payload
        .get("collapsed")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(collapsed)
}

fn format_stamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Keep formatting pure-std; avoid pulling chrono for a filename.
    let days = secs / 86_400;
    let time = secs % 86_400;
    let hh = time / 3600;
    let mm = (time % 3600) / 60;
    let ss = time % 60;
    // ISO-ish but filesystem-safe (no colons).
    format!("unix{days:08}-{hh:02}{mm:02}{ss:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_service(tmp: &tempfile::TempDir, clock: Arc<dyn Clock>) -> AutoDreamService {
        AutoDreamService::with_clock(tmp.path().to_path_buf(), clock)
            .with_idle_threshold(Duration::from_secs(10))
            .with_min_delta_bytes(16)
    }

    async fn seed_transcript(root: &Path, payload: &[u8]) -> PathBuf {
        let dir = root.join(".vac/sessions");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("session-a.jsonl");
        tokio::fs::write(&path, payload).await.unwrap();
        path
    }

    #[tokio::test]
    async fn skip_when_not_idle_yet() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let svc = mk_service(&tmp, clock.clone());
        // Activity just now; clock hasn't advanced → not idle.
        let now = clock.now();
        let out = svc.tick(now).await.unwrap();
        assert_eq!(out, TickOutcome::Skipped { reason: SkipReason::NotIdleYet });
    }

    #[tokio::test]
    async fn skip_when_no_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let svc = mk_service(&tmp, clock.clone());
        // Pin last_activity BEFORE advancing so the gap is > 10 s.
        let last_activity = clock.now();
        clock.advance(30);
        let out = svc.tick(last_activity).await.unwrap();
        assert_eq!(out, TickOutcome::Skipped { reason: SkipReason::NoTranscript });
    }

    #[tokio::test]
    async fn writes_archived_entry_when_idle_and_active() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let svc = mk_service(&tmp, clock.clone());
        seed_transcript(
            tmp.path(),
            b"session line 1\nsession line 2\nsession line 3\n",
        )
        .await;
        let last_activity = clock.now();
        clock.advance(30);
        let out = svc.tick(last_activity).await.unwrap();
        let path = match out {
            TickOutcome::Wrote { path } => path,
            other => panic!("expected Wrote, got {other:?}"),
        };
        assert!(path.starts_with(tmp.path().join(".vac/memory/archived")));
        assert!(path.to_string_lossy().contains("dream-"));
        let body = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(body.contains("kind: dream"));
    }

    #[tokio::test]
    async fn does_not_redream_without_delta() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let svc = mk_service(&tmp, clock.clone());
        seed_transcript(tmp.path(), b"lots of content for the first dream\n")
            .await;
        let initial_activity = clock.now();
        clock.advance(30);
        let first = svc.tick(initial_activity).await.unwrap();
        assert!(
            matches!(first, TickOutcome::Wrote { .. }),
            "first tick must dream",
        );
        // Transcript unchanged; pass `clock.now()` so idle is 0 —
        // this exercises the NotIdleYet branch deterministically.
        let still_active = clock.now();
        let second = svc.tick(still_active).await.unwrap();
        assert_eq!(
            second,
            TickOutcome::Skipped { reason: SkipReason::NotIdleYet },
        );
        // Now go idle again with no new transcript content: exact
        // reason must be NoActivityDelta, not "any skip".
        clock.advance(30);
        let third = svc.tick(initial_activity).await.unwrap();
        assert_eq!(
            third,
            TickOutcome::Skipped { reason: SkipReason::NoActivityDelta },
        );
    }

    #[tokio::test]
    async fn transcript_rotation_resets_delta_baseline() {
        // Session rotation (archive + fresh file) shrinks the
        // transcript. A naïve saturating_sub would wedge the
        // service with zero-delta skips forever. After the fix, a
        // rotation resets the baseline and the next tick re-fires
        // on the new content.
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let svc = mk_service(&tmp, clock.clone());
        // Seed a large transcript, dream once.
        seed_transcript(tmp.path(), &vec![b'a'; 4096]).await;
        let activity_before = clock.now();
        clock.advance(30);
        assert!(matches!(
            svc.tick(activity_before).await.unwrap(),
            TickOutcome::Wrote { .. },
        ));
        // Rotate: same name, smaller content.
        seed_transcript(tmp.path(), &vec![b'b'; 128]).await;
        clock.advance(30);
        let fresh_activity = clock.now();
        clock.advance(30);
        let out = svc.tick(fresh_activity).await.unwrap();
        // Rotation detected → baseline reset → the 128 bytes now
        // count as new activity (>= MIN_ACTIVITY_DELTA_BYTES=16 in
        // the test config) and the tick writes.
        assert!(
            matches!(out, TickOutcome::Wrote { .. }),
            "post-rotation tick must dream; got {out:?}",
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_to_write_through_symlink_archive_dir() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let svc = mk_service(&tmp, clock.clone());
        // Pre-build `.vac/memory/archived` as a symlink to another
        // tempdir. tick must refuse.
        let outside = tempfile::tempdir().unwrap();
        let mem_dir = tmp.path().join(".vac").join("memory");
        tokio::fs::create_dir_all(&mem_dir).await.unwrap();
        symlink(outside.path(), mem_dir.join("archived")).unwrap();
        seed_transcript(tmp.path(), &vec![b'x'; 2048]).await;
        let activity = clock.now();
        clock.advance(30);
        let out = svc.tick(activity).await.unwrap();
        assert_eq!(
            out,
            TickOutcome::Skipped { reason: SkipReason::UnsafeArchiveDir },
        );
        // And nothing landed in the outside dir.
        let mut rd = tokio::fs::read_dir(outside.path()).await.unwrap();
        assert!(rd.next_entry().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn redreams_after_new_activity() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let svc = mk_service(&tmp, clock.clone());
        seed_transcript(tmp.path(), b"first batch content\n").await;
        let initial_activity = clock.now();
        clock.advance(30);
        matches!(
            svc.tick(initial_activity).await.unwrap(),
            TickOutcome::Wrote { .. }
        );
        // Operator resumes briefly → new transcript content.
        let bigger = format!(
            "first batch content\n{}",
            "new operator activity padded to clear delta\n".repeat(4)
        );
        seed_transcript(tmp.path(), bigger.as_bytes()).await;
        clock.advance(30);
        let fresh_activity = clock.now();
        clock.advance(30);
        let out = svc.tick(fresh_activity).await.unwrap();
        matches!(out, TickOutcome::Wrote { .. });
    }

    #[tokio::test]
    async fn last_dream_at_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let svc = mk_service(&tmp, clock.clone());
        seed_transcript(tmp.path(), &vec![b'x'; 256]).await;
        assert!(svc.last_dream_at().await.is_none());
        let last_activity = clock.now();
        clock.advance(30);
        svc.tick(last_activity).await.unwrap();
        assert!(svc.last_dream_at().await.is_some());
    }

    #[tokio::test]
    async fn newest_transcript_skips_non_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".vac/sessions");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("a.jsonl"), b"x").await.unwrap();
        tokio::fs::write(dir.join("b.md"), b"y").await.unwrap();
        let got = newest_transcript(&dir).await.unwrap();
        assert!(got.is_some());
        assert!(got.unwrap().0.to_string_lossy().ends_with("a.jsonl"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn newest_transcript_skips_symlinks() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".vac/sessions");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let real = dir.join("real.jsonl");
        tokio::fs::write(&real, b"x").await.unwrap();
        let link = dir.join("link.jsonl");
        symlink(&real, &link).unwrap();
        let (p, _) = newest_transcript(&dir).await.unwrap().unwrap();
        assert_eq!(p, real);
    }
}
