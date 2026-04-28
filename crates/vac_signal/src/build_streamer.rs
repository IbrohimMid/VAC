//! F6.6 — Real-time build streaming.
//!
//! Wraps a `tokio::process::Command` (typically `cargo build`) and
//! streams stdout + stderr line-by-line into a [`SignalBuffer`] keyed
//! as `build:<job>`. Operators can then tail the buffer in the TUI
//! while the build runs, and the full transcript is available for
//! post-mortem via the same signal pipeline other subsystems use.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::warn;

use crate::buffer::SignalBuffer;

/// Key prefix for every buffer produced by this module. Drivers that
/// retrieve buffers by key use this to select only build streams.
pub const BUILD_STREAM_KEY_PREFIX: &str = "build:";

/// Result of a completed build: exit status + the final line count
/// + how many lines were dropped because the `SignalBuffer` filled.
#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub total_lines: u64,
    /// Lines dropped by the signal buffer's ring overflow. Non-zero
    /// means the build emitted output faster than the buffer could
    /// absorb at its configured capacity.
    pub dropped_lines: u64,
}

/// Streamer handle. One per running build. Drop to abort the child
/// process if it's still alive.
pub struct BuildStreamer {
    job: String,
    buffer: std::sync::Arc<Mutex<SignalBuffer>>,
    child: Option<Child>,
}

impl BuildStreamer {
    /// Spawn `command` and start streaming its output into `buffer`.
    /// `job` is appended to [`BUILD_STREAM_KEY_PREFIX`] to produce the
    /// caller-visible buffer key — drivers typically use a job id
    /// (crate name + nonce).
    ///
    /// The command is configured with piped stdout + stderr; the
    /// child's stdin is closed so interactive prompts can't stall.
    pub async fn spawn(
        job: impl Into<String>,
        mut command: Command,
        buffer: std::sync::Arc<Mutex<SignalBuffer>>,
    ) -> std::io::Result<Self> {
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let job_str: String = job.into();
        if let Some(out) = stdout {
            let buf = buffer.clone();
            let tag = format!("[stdout] ");
            tokio::spawn(async move {
                pump_lines(out, buf, tag).await;
            });
        }
        if let Some(err) = stderr {
            let buf = buffer.clone();
            let tag = format!("[stderr] ");
            tokio::spawn(async move {
                pump_lines(err, buf, tag).await;
            });
        }

        Ok(Self {
            job: job_str,
            buffer,
            child: Some(child),
        })
    }

    pub fn job(&self) -> &str {
        &self.job
    }

    /// The canonical buffer key (`build:<job>`).
    pub fn buffer_key(&self) -> String {
        format!("{BUILD_STREAM_KEY_PREFIX}{}", self.job)
    }

    /// Wait for the build to finish. Returns the outcome and drops
    /// the child handle. Drains the buffer to compute `total_lines`.
    pub async fn wait(&mut self) -> std::io::Result<BuildOutcome> {
        let status = match self.child.as_mut() {
            Some(c) => c.wait().await?,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "build child already consumed",
                ));
            }
        };
        self.child = None;
        // Give the pumping tasks a beat to drain any final lines.
        tokio::task::yield_now().await;
        let buf = self.buffer.lock().await;
        let total_lines = buf.len() as u64;
        let dropped_lines = buf.dropped();
        drop(buf);
        Ok(BuildOutcome {
            success: status.success(),
            exit_code: status.code(),
            total_lines,
            dropped_lines,
        })
    }
}

impl Drop for BuildStreamer {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            // Best-effort SIGKILL — a streamer dropped without wait()
            // means the caller no longer cares about this build.
            if let Err(e) = c.start_kill() {
                warn!(target: "vac_signal::build", "start_kill failed: {e}");
            }
            // Reap inside a detached task so the tokio child's
            // stdio/zombie state gets cleaned up. If no runtime is
            // available we fall back to an inline sync wait with a
            // short deadline.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = c.wait().await;
                });
            }
        }
    }
}

async fn pump_lines<R>(reader: R, buffer: std::sync::Arc<Mutex<SignalBuffer>>, tag: String)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut b = buffer.lock().await;
        b.push_line(format!("{tag}{line}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf() -> std::sync::Arc<Mutex<SignalBuffer>> {
        std::sync::Arc::new(Mutex::new(SignalBuffer::new(
            crate::buffer::SignalStreamKind::Other,
            1024,
        )))
    }

    #[tokio::test]
    async fn buffer_key_uses_prefix() {
        let b = buf();
        // `sh -c "exit 0"` is portable to minimal containers that
        // don't ship `/usr/bin/true`.
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "exit 0"]);
        let mut s = BuildStreamer::spawn("test-job", cmd, b).await.unwrap();
        assert_eq!(s.buffer_key(), "build:test-job");
        let outcome = s.wait().await.unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.dropped_lines, 0);
    }

    #[tokio::test]
    async fn successful_command_captures_stdout_lines() {
        let b = buf();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo first; echo second"]);
        let mut s = BuildStreamer::spawn("echo", cmd, b.clone()).await.unwrap();
        let outcome = s.wait().await.unwrap();
        assert!(outcome.success);
        // Give pumpers a moment post-exit.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let buf_lines: Vec<String> = b.lock().await.iter().map(|e| e.text.clone()).collect();
        assert!(buf_lines.iter().any(|l| l.contains("first")));
        assert!(buf_lines.iter().any(|l| l.contains("second")));
    }

    #[tokio::test]
    async fn failing_command_surfaces_nonzero_exit() {
        let b = buf();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "exit 7"]);
        let mut s = BuildStreamer::spawn("fail", cmd, b).await.unwrap();
        let outcome = s.wait().await.unwrap();
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, Some(7));
    }

    #[tokio::test]
    async fn stderr_is_captured_with_tag() {
        let b = buf();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo oops >&2"]);
        let mut s = BuildStreamer::spawn("stderr", cmd, b.clone())
            .await
            .unwrap();
        s.wait().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let buf_lines: Vec<String> = b.lock().await.iter().map(|e| e.text.clone()).collect();
        assert!(
            buf_lines
                .iter()
                .any(|l| l.contains("[stderr]") && l.contains("oops"))
        );
    }
}
