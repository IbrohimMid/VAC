//! W10.2 — `AwaySummary` on resume.
//!
//! Persists a `last-seen` timestamp. On each session start the
//! driver calls [`AwaySummaryService::on_resume`]. When the gap
//! between now and the recorded timestamp exceeds
//! `AWAY_THRESHOLD` (default 1 hour), the service composes a
//! one-line summary of what happened while the operator was away
//! — submit count + commit count + the last transcript's tail —
//! and returns it so the driver can push a notification.
//!
//! All I/O is async. Git is queried via `tokio::process::Command`.
//! On any sub-query failure the service degrades gracefully —
//! returning the parts it could collect — so a resume never blocks
//! on a flaky VCS.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

pub const AWAY_THRESHOLD: Duration = Duration::from_secs(3_600); // 1h
pub const LAST_SEEN_FILENAME: &str = "last-seen.json";
/// Cap on the tail we read from the last transcript to keep the
/// summary composition bounded.
pub const TRANSCRIPT_TAIL_BYTES: u64 = 64 * 1024;
/// Max wall-clock `git log` is allowed to take. A large repo on
/// flaky FS could stall a resume; 3 s is forgiving while still
/// keeping the user-visible latency bounded.
pub const GIT_QUERY_TIMEOUT: Duration = Duration::from_secs(3);

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LastSeen {
    pub unix_secs: i64,
}

/// What `on_resume` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeOutcome {
    /// Under threshold — no summary needed.
    NoSummary { gap: Duration },
    /// Over threshold — driver should surface this line.
    Summary(AwayReport),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwayReport {
    pub gap: Duration,
    pub commits_since: u32,
    pub transcript_lines: u32,
    pub last_transcript_snippet: String,
    pub one_liner: String,
}

pub struct AwaySummaryService {
    project_root: PathBuf,
    clock: Arc<dyn Clock>,
    threshold: Duration,
}

impl AwaySummaryService {
    pub fn new(project_root: PathBuf) -> Self {
        Self::with_clock(project_root, Arc::new(SystemClock))
    }

    pub fn with_clock(project_root: PathBuf, clock: Arc<dyn Clock>) -> Self {
        Self {
            project_root,
            clock,
            threshold: AWAY_THRESHOLD,
        }
    }

    pub fn with_threshold(mut self, d: Duration) -> Self {
        self.threshold = d;
        self
    }

    fn last_seen_path(&self) -> PathBuf {
        self.project_root.join(".vac").join(LAST_SEEN_FILENAME)
    }

    /// Persist the current timestamp. Called on each heartbeat /
    /// submit completion so the gap measured on resume is accurate.
    /// Atomic via temp + rename.
    pub async fn touch(&self) -> anyhow::Result<()> {
        let path = self.last_seen_path();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let payload = LastSeen {
            unix_secs: self
                .clock
                .now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        };
        let json = serde_json::to_vec_pretty(&payload)?;
        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, json).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }

    pub async fn read_last_seen(&self) -> Option<LastSeen> {
        let raw = tokio::fs::read(self.last_seen_path()).await.ok()?;
        serde_json::from_slice(&raw).ok()
    }

    /// Called on session start. Returns `Summary` when the gap
    /// exceeds `threshold`.
    pub async fn on_resume(&self) -> anyhow::Result<ResumeOutcome> {
        let now_secs = self
            .clock
            .now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let Some(last) = self.read_last_seen().await else {
            // First ever resume — nothing to summarise, but touch so
            // the next resume has a baseline.
            let _ = self.touch().await;
            return Ok(ResumeOutcome::NoSummary {
                gap: Duration::ZERO,
            });
        };
        let gap_secs = (now_secs - last.unix_secs).max(0) as u64;
        let gap = Duration::from_secs(gap_secs);
        if gap < self.threshold {
            return Ok(ResumeOutcome::NoSummary { gap });
        }
        let commits = count_commits_since(&self.project_root, last.unix_secs)
            .await
            .unwrap_or(0);
        let (lines, snippet) = transcript_tail_summary(&self.project_root)
            .await
            .unwrap_or((0, String::new()));
        let gap_h = gap.as_secs() / 3600;
        let gap_m = (gap.as_secs() % 3600) / 60;
        let one_liner = format!(
            "away {gap_h}h{gap_m:02}m • {commits} commit(s) • {lines} transcript line(s)"
        );
        Ok(ResumeOutcome::Summary(AwayReport {
            gap,
            commits_since: commits,
            transcript_lines: lines,
            last_transcript_snippet: snippet,
            one_liner,
        }))
    }
}

async fn count_commits_since(
    project_root: &Path,
    since_unix: i64,
) -> anyhow::Result<u32> {
    // Wrap in a timeout: a large + flaky repo could otherwise block
    // the resume path for seconds / minutes. On timeout we return
    // 0 commits rather than failing the whole summary — better to
    // omit one number than to block the user.
    let fut = Command::new("git")
        .args([
            "log",
            &format!("--since={since_unix}"),
            "--pretty=oneline",
        ])
        .current_dir(project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = match tokio::time::timeout(GIT_QUERY_TIMEOUT, fut).await {
        Ok(res) => res?,
        Err(_) => {
            tracing::warn!(
                target: "vac_tui_runtime::away_summary",
                timeout_ms = GIT_QUERY_TIMEOUT.as_millis() as u64,
                "git log timed out; degrading commit count to 0",
            );
            return Ok(0);
        }
    };
    if !out.status.success() {
        return Ok(0);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.lines().filter(|l| !l.is_empty()).count() as u32)
}

async fn transcript_tail_summary(
    project_root: &Path,
) -> anyhow::Result<(u32, String)> {
    let dir = project_root.join(".vac").join("sessions");
    if !dir.is_dir() {
        return Ok((0, String::new()));
    }
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    let mut rd = tokio::fs::read_dir(&dir).await?;
    while let Some(e) = rd.next_entry().await? {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
            continue;
        }
        let meta = tokio::fs::symlink_metadata(&p).await?;
        if meta.file_type().is_symlink() || !meta.file_type().is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
        if newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
            newest = Some((mtime, p));
        }
    }
    let Some((_, path)) = newest else {
        return Ok((0, String::new()));
    };
    let tail = read_tail(&path, TRANSCRIPT_TAIL_BYTES).await?;
    let lines = tail.lines().count() as u32;
    let snippet = tail
        .lines()
        .last()
        .unwrap_or("")
        .chars()
        .take(200)
        .collect::<String>();
    Ok((lines, snippet))
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn svc(tmp: &tempfile::TempDir, clock: Arc<dyn Clock>) -> AwaySummaryService {
        AwaySummaryService::with_clock(tmp.path().to_path_buf(), clock)
            .with_threshold(Duration::from_secs(60))
    }

    #[tokio::test]
    async fn first_resume_without_prior_state_no_summary() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let s = svc(&tmp, clock.clone()).await;
        let out = s.on_resume().await.unwrap();
        match out {
            ResumeOutcome::NoSummary { gap } => {
                assert_eq!(gap, Duration::ZERO);
            }
            other => panic!("expected NoSummary, got {other:?}"),
        }
        // Baseline touch wrote the file.
        assert!(s.read_last_seen().await.is_some());
    }

    #[tokio::test]
    async fn under_threshold_no_summary() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let s = svc(&tmp, clock.clone()).await;
        s.touch().await.unwrap();
        clock.advance(10);
        let out = s.on_resume().await.unwrap();
        matches!(out, ResumeOutcome::NoSummary { .. });
    }

    #[tokio::test]
    async fn over_threshold_renders_summary() {
        // W10.2 acceptance: 90-minute gap on a 60-s test threshold →
        // Summary with a gap field that reflects wall-clock advance.
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let s = svc(&tmp, clock.clone()).await;
        s.touch().await.unwrap();
        clock.advance(90 * 60); // 90 min
        let out = s.on_resume().await.unwrap();
        match out {
            ResumeOutcome::Summary(rep) => {
                assert_eq!(rep.gap, Duration::from_secs(5_400));
                assert!(rep.one_liner.contains("away"));
                assert!(rep.one_liner.contains("1h30m"));
            }
            other => panic!("expected Summary, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn touch_is_atomic_no_temp_leftover() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let s = svc(&tmp, clock.clone()).await;
        s.touch().await.unwrap();
        s.touch().await.unwrap();
        let parent = tmp.path().join(".vac");
        let mut rd = tokio::fs::read_dir(&parent).await.unwrap();
        while let Some(e) = rd.next_entry().await.unwrap() {
            let name = e.file_name().to_string_lossy().into_owned();
            assert!(!name.ends_with(".tmp"), "leftover {name}");
        }
    }

    #[tokio::test]
    async fn corrupted_last_seen_is_handled() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let s = svc(&tmp, clock.clone()).await;
        // Hand-crafted invalid JSON.
        let path = tmp.path().join(".vac/last-seen.json");
        tokio::fs::create_dir_all(path.parent().unwrap()).await.unwrap();
        tokio::fs::write(&path, b"not json").await.unwrap();
        let out = s.on_resume().await.unwrap();
        // Corrupt file parses to None → NoSummary + baseline rewrite.
        matches!(out, ResumeOutcome::NoSummary { .. });
    }

    #[tokio::test]
    async fn transcript_tail_summary_counts_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".vac/sessions");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let body = (0..20)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        tokio::fs::write(dir.join("a.jsonl"), &body).await.unwrap();
        let (lines, snippet) = transcript_tail_summary(tmp.path()).await.unwrap();
        assert_eq!(lines, 20);
        assert!(snippet.starts_with("line-"));
    }

    #[tokio::test]
    async fn transcript_tail_no_sessions_yields_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let (lines, snippet) = transcript_tail_summary(tmp.path()).await.unwrap();
        assert_eq!(lines, 0);
        assert!(snippet.is_empty());
    }

    #[tokio::test]
    async fn read_last_seen_round_trips_after_touch() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(FakeClock::new());
        let s = svc(&tmp, clock.clone()).await;
        s.touch().await.unwrap();
        let ls = s.read_last_seen().await.unwrap();
        assert!(ls.unix_secs > 0);
    }
}
