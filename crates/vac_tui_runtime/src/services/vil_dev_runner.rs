//! Background `vil dev` runner (PR-T14).
//!
//! Spawns the user-configured `vil dev` command (from `VilConfig::dev_command`)
//! via `tokio::process::Command`, captures `stdout` and `stderr` line-by-line,
//! and emits structured [`RunnerEvent`]s back to the caller over an `mpsc`
//! channel. The caller is responsible for routing those events to the tray,
//! Activity panel, and session timeline (PR-T9 + PR-T8).
//!
//! Checkpoint markers of the form `[vil-checkpoint] <session_id> <iso-ts>`
//! that appear on `stdout` are parsed into [`RunnerEvent::Checkpoint`].
//!
//! Shutdown is a two-stage kill: SIGTERM first, then SIGKILL fallback after a
//! caller-supplied timeout. On non-Unix platforms we skip straight to
//! [`tokio::process::Child::kill`] (which maps to TerminateProcess on Windows).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

// ── Event surface ───────────────────────────────────────────────────────

/// Structured event emitted by a running `vil dev` process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerEvent {
    Started { pid: u32 },
    Stdout(String),
    Stderr(String),
    Checkpoint { session_id: String, ts: String },
    Exited { code: Option<i32>, signal: Option<i32> },
}

// ── Parser ────────────────────────────────────────────────────────────────

/// Tag that marks a checkpoint line. Public so tests and callers can reuse it
/// when emitting synthetic checkpoints.
pub const CHECKPOINT_TAG: &str = "[vil-checkpoint]";

/// Parse one stdout line. Returns `Some((session_id, ts))` when the line looks
/// like `[vil-checkpoint] <session_id> <iso-ts>` (with any amount of leading
/// whitespace). Returns `None` otherwise.
pub fn parse_checkpoint_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix(CHECKPOINT_TAG)?.trim_start();
    let mut parts = rest.split_whitespace();
    let session_id = parts.next()?;
    let ts = parts.next()?;
    // Trailing tokens are treated as part of the timestamp only if the parser
    // is called on a well-formed line; otherwise we reject them to keep the
    // contract tight.
    if parts.next().is_some() {
        return None;
    }
    if session_id.is_empty() || ts.is_empty() {
        return None;
    }
    Some((session_id.to_string(), ts.to_string()))
}

// ── Runner ────────────────────────────────────────────────────────────────

/// Handle to a spawned `vil dev` process.
///
/// Drop the handle to abandon the process without waiting (the stdout/stderr
/// tasks will terminate when the child closes its pipes). For a deterministic
/// shutdown call [`VilDevRunner::kill_graceful`] first.
pub struct VilDevRunner {
    child: Option<Child>,
    pid: Option<u32>,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    tx: mpsc::Sender<RunnerEvent>,
}

impl VilDevRunner {
    /// Create a new runner that will publish events to `tx`.
    pub fn new(tx: mpsc::Sender<RunnerEvent>) -> Self {
        Self {
            child: None,
            pid: None,
            stdout_task: None,
            stderr_task: None,
            tx,
        }
    }

    /// Current OS pid, if the process has been spawned and not yet reaped.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// True after [`spawn`] and before the child exits or is killed.
    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    /// Spawn `sh -c <dev_command>` (`cmd /C` on Windows) under `cwd`.
    ///
    /// Returns an error if the command string is empty, if the shell cannot be
    /// spawned, or if stdio pipes cannot be captured. Emits
    /// [`RunnerEvent::Started`] when the child reports a pid.
    pub async fn spawn(&mut self, dev_command: &str, cwd: &Path) -> Result<()> {
        let command = dev_command.trim();
        if command.is_empty() {
            return Err(anyhow!("dev_command is empty"));
        }
        if self.child.is_some() {
            return Err(anyhow!("runner already has an active child"));
        }

        let mut cmd = shell_command(command);
        cmd.current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true);

        let mut child = cmd.spawn().with_context(|| {
            format!("failed to spawn vil dev command: {command:?}")
        })?;

        let pid = child.id();
        if let Some(pid) = pid {
            let _ = self.tx.send(RunnerEvent::Started { pid }).await;
        }

        if let Some(stdout) = child.stdout.take() {
            let tx = self.tx.clone();
            let handle = tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some((session_id, ts)) = parse_checkpoint_line(&line) {
                        let _ = tx
                            .send(RunnerEvent::Checkpoint { session_id, ts })
                            .await;
                    }
                    if tx.send(RunnerEvent::Stdout(line)).await.is_err() {
                        break;
                    }
                }
            });
            self.stdout_task = Some(handle);
        }

        if let Some(stderr) = child.stderr.take() {
            let tx = self.tx.clone();
            let handle = tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx.send(RunnerEvent::Stderr(line)).await.is_err() {
                        break;
                    }
                }
            });
            self.stderr_task = Some(handle);
        }

        self.child = Some(child);
        self.pid = pid;
        Ok(())
    }

    /// Wait for the child to exit on its own. Emits [`RunnerEvent::Exited`]
    /// when the child is reaped. Returns immediately with `Ok(None)` if no
    /// child is currently running.
    pub async fn wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        let Some(mut child) = self.child.take() else {
            return Ok(None);
        };
        let status = child.wait().await.context("failed to reap vil dev child")?;
        let _ = self.tx.send(exit_event(&status)).await;
        self.pid = None;
        self.await_stream_tasks().await;
        Ok(Some(status))
    }

    /// Attempt SIGTERM, then SIGKILL after `sigterm_wait` if the child is
    /// still alive. On non-Unix platforms only `kill` is invoked, because
    /// SIGTERM has no meaningful equivalent.
    pub async fn kill_graceful(&mut self, sigterm_wait: Duration) -> Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };

        #[cfg(unix)]
        if let Some(pid) = self.pid {
            send_sigterm(pid);
            match timeout(sigterm_wait, child.wait()).await {
                Ok(Ok(status)) => {
                    let _ = self.tx.send(exit_event(&status)).await;
                    self.pid = None;
                    self.await_stream_tasks().await;
                    return Ok(());
                }
                Ok(Err(e)) => {
                    return Err(anyhow!("failed to reap vil dev child after SIGTERM: {e}"));
                }
                Err(_) => {
                    // Timeout — fall through to SIGKILL path below.
                }
            }
        }

        // Either non-Unix, or SIGTERM timed out — force SIGKILL.
        child
            .start_kill()
            .context("failed to send SIGKILL to vil dev child")?;
        let status = child.wait().await.context("failed to reap vil dev child")?;
        let _ = self.tx.send(exit_event(&status)).await;
        self.pid = None;
        self.await_stream_tasks().await;
        Ok(())
    }

    async fn await_stream_tasks(&mut self) {
        if let Some(handle) = self.stdout_task.take() {
            let _ = handle.await;
        }
        if let Some(handle) = self.stderr_task.take() {
            let _ = handle.await;
        }
    }
}

fn exit_event(status: &std::process::ExitStatus) -> RunnerEvent {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        RunnerEvent::Exited {
            code: status.code(),
            signal: status.signal(),
        }
    }
    #[cfg(not(unix))]
    {
        RunnerEvent::Exited {
            code: status.code(),
            signal: None,
        }
    }
}

#[cfg(unix)]
fn send_sigterm(pid: u32) {
    // Safety: libc::kill is async-signal-safe and accepts any pid_t. We only
    // pass pids we obtained from `Child::id()`, so the value is a valid
    // process id or a stale one (in which case kill returns ESRCH, which we
    // ignore — the child is already gone).
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd
}

#[cfg(not(unix))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(command);
    cmd
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn checkpoint_parser_extracts_session() {
        let line = "[vil-checkpoint] s1 2026-04-21T00:00:00Z";
        let parsed = parse_checkpoint_line(line).expect("should parse");
        assert_eq!(parsed.0, "s1");
        assert_eq!(parsed.1, "2026-04-21T00:00:00Z");
    }

    #[test]
    fn checkpoint_parser_accepts_leading_whitespace() {
        let parsed = parse_checkpoint_line("   [vil-checkpoint] abc 2026-04-21T00:00:00Z")
            .expect("should parse");
        assert_eq!(parsed.0, "abc");
    }

    #[test]
    fn checkpoint_parser_rejects_non_checkpoint_lines() {
        assert!(parse_checkpoint_line("hello world").is_none());
        assert!(parse_checkpoint_line("[vil-other] s1 ts").is_none());
        assert!(parse_checkpoint_line("[vil-checkpoint] s1").is_none()); // missing ts
        assert!(
            parse_checkpoint_line("[vil-checkpoint] s1 ts extra").is_none(),
            "extra tokens should be rejected"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn vil_dev_spawns_and_streams() {
        let (tx, mut rx) = mpsc::channel(64);
        let mut runner = VilDevRunner::new(tx);
        runner
            .spawn(
                "echo hello; sleep 0.05; echo '[vil-checkpoint] s1 2026-04-21T00:00:00Z'",
                std::env::temp_dir().as_path(),
            )
            .await
            .expect("spawn");

        runner.wait().await.expect("wait");
        drop(runner);

        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }

        let saw_hello = events.iter().any(|e| matches!(e, RunnerEvent::Stdout(l) if l == "hello"));
        let saw_checkpoint = events.iter().any(
            |e| matches!(e, RunnerEvent::Checkpoint { session_id, ts }
                if session_id == "s1" && ts == "2026-04-21T00:00:00Z"),
        );
        let saw_exit = events
            .iter()
            .any(|e| matches!(e, RunnerEvent::Exited { code: Some(0), .. }));

        assert!(saw_hello, "expected Stdout(\"hello\") in {events:?}");
        assert!(saw_checkpoint, "expected Checkpoint event in {events:?}");
        assert!(saw_exit, "expected Exited(code=0) in {events:?}");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn vil_dev_kill_sigterm_then_sigkill() {
        let (tx, mut rx) = mpsc::channel(64);
        let mut runner = VilDevRunner::new(tx);

        // Ignore SIGTERM so kill_graceful must fall through to SIGKILL. The
        // `echo READY` marker lets the test block until the trap is actually
        // installed — otherwise kill_graceful might race `dash` before it has
        // parsed the trap statement and SIGTERM would kill the shell outright.
        runner
            .spawn(
                "trap '' TERM; echo READY; while true; do sleep 1; done",
                std::env::temp_dir().as_path(),
            )
            .await
            .expect("spawn");
        assert!(runner.is_running());

        // Wait for the READY marker so the trap is guaranteed in effect.
        let ready_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = ready_deadline.saturating_duration_since(tokio::time::Instant::now());
            let ev = timeout(remaining, rx.recv())
                .await
                .expect("timeout waiting for READY")
                .expect("runner channel closed before READY");
            if matches!(ev, RunnerEvent::Stdout(ref l) if l == "READY") {
                break;
            }
        }

        runner
            .kill_graceful(Duration::from_millis(120))
            .await
            .expect("kill_graceful");
        assert!(!runner.is_running());

        drop(runner);

        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        let saw_sigkill = events.iter().any(
            |e| matches!(e, RunnerEvent::Exited { signal: Some(s), .. } if *s == libc::SIGKILL),
        );
        assert!(
            saw_sigkill,
            "expected SIGKILL exit signal after SIGTERM timeout; events: {events:?}"
        );
    }
}
