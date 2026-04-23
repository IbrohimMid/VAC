//! F1.7 — Small utility tools: `sleep` and `send_message`.
//!
//! Claude Code pattern: small composable primitives that make agent
//! loops more expressive without inflating the tool framework.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

// ── sleep ─────────────────────────────────────────────────────────

/// Sleep tool. Agent can explicitly pause between tool calls — useful
/// for polling patterns, waiting for external state to change, or
/// rate-limit respect.
#[derive(Debug, Deserialize)]
struct SleepInput {
    /// Duration in milliseconds. Capped at 60_000 ms (1 minute) to
    /// avoid locking the agent indefinitely.
    ms: u64,
    /// Optional reason (for trace observability).
    #[serde(default)]
    reason: Option<String>,
}

const MAX_SLEEP_MS: u64 = 60_000;

pub struct SleepTool;

impl SleepTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SleepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for SleepTool {
    fn name(&self) -> &str {
        "sleep"
    }

    fn description(&self) -> &str {
        "Pause the agent for the specified milliseconds (max 60000). Use for polling or rate-limit respect. Logs the reason to trace."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "ms": { "type": "integer", "minimum": 0, "maximum": MAX_SLEEP_MS },
                "reason": { "type": "string" }
            },
            "required": ["ms"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "safe"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: SleepInput = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        let ms = input.ms.min(MAX_SLEEP_MS);
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        Ok(serde_json::json!({
            "slept_ms": ms,
            "reason": input.reason,
        }))
    }
}

// ── send_message ──────────────────────────────────────────────────

/// Message channel — target recipients.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MessageChannel {
    /// Write to `.vac/messages/<session>.log` — visible via
    /// `task_output`-style readers.
    SessionLog,
    /// Append to `.vac/messages/inbox.log` — operator-visible inbox.
    OperatorInbox,
}

#[derive(Debug, Deserialize)]
struct MessageInput {
    channel: MessageChannel,
    content: String,
    #[serde(default)]
    topic: Option<String>,
}

pub struct SendMessageTool;

impl SendMessageTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SendMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for SendMessageTool {
    fn name(&self) -> &str {
        "send_message"
    }

    fn description(&self) -> &str {
        "Append a message to a named channel. Use 'session_log' for per-session notes or 'operator_inbox' for messages the operator should see."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "enum": ["session_log", "operator_inbox"] },
                "content": { "type": "string" },
                "topic": { "type": "string" }
            },
            "required": ["channel", "content"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "safe"
    }

    fn risk_level(&self) -> &str {
        "mutating"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: MessageInput = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;

        // Bound content size to keep the append log from becoming an
        // exfiltration channel or filling disk.
        if input.content.len() > 16 * 1024 {
            return Err(ToolError::ExecutionFailed(format!(
                "content too long ({} > 16384 chars)",
                input.content.len()
            )));
        }
        // Reject newlines/control chars in topic — we inline it into
        // the log format string; a newline would break log format.
        if let Some(t) = &input.topic {
            if t.len() > 64 || t.chars().any(|c| c.is_control()) {
                return Err(ToolError::ExecutionFailed(
                    "topic must be <=64 chars, no control characters".into(),
                ));
            }
        }

        let dir = context.working_dir.join(".vac").join("messages");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("mkdir: {e}")))?;
        let file = match input.channel {
            MessageChannel::SessionLog => {
                dir.join(format!("{}.log", context.session_id))
            }
            MessageChannel::OperatorInbox => dir.join("inbox.log"),
        };

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let topic = input.topic.as_deref().unwrap_or("-");
        let line = format!("[{ts}] [{topic}] {}\n", input.content);

        use tokio::io::AsyncWriteExt;
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("open channel: {e}")))?;
        f.write_all(line.as_bytes())
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("write: {e}")))?;

        Ok(serde_json::json!({
            "channel": match input.channel {
                MessageChannel::SessionLog => "session_log",
                MessageChannel::OperatorInbox => "operator_inbox",
            },
            "file": file.to_string_lossy(),
            "bytes_written": line.len(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;

    #[tokio::test(start_paused = true)]
    async fn sleep_caps_at_max() {
        // tokio paused-time prevents this test from taking 60s wall-clock
        // while still exercising the real clamp + sleep path.
        let tool = SleepTool::new();
        let ctx = make_ctx(std::env::temp_dir(), uuid::Uuid::new_v4());
        let out = tool
            .execute(serde_json::json!({ "ms": 999_999 }), &ctx)
            .await
            .unwrap();
        assert_eq!(out["slept_ms"], MAX_SLEEP_MS);
    }

    #[tokio::test]
    async fn sleep_zero_is_noop() {
        let tool = SleepTool::new();
        let ctx = make_ctx(std::env::temp_dir(), uuid::Uuid::new_v4());
        let start = std::time::Instant::now();
        tool.execute(serde_json::json!({ "ms": 0 }), &ctx)
            .await
            .unwrap();
        assert!(start.elapsed().as_millis() < 50);
    }

    #[tokio::test]
    async fn send_message_operator_inbox_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let tool = SendMessageTool::new();
        tool.execute(
            serde_json::json!({
                "channel": "operator_inbox",
                "content": "found a race condition in runtime",
                "topic": "bug",
            }),
            &ctx,
        )
        .await
        .unwrap();

        let inbox = tmp.path().join(".vac/messages/inbox.log");
        let content = std::fs::read_to_string(inbox).unwrap();
        assert!(content.contains("[bug]"));
        assert!(content.contains("race condition"));
    }

    #[tokio::test]
    async fn send_message_session_log_uses_session_id() {
        let tmp = tempfile::tempdir().unwrap();
        let sid = uuid::Uuid::new_v4();
        let ctx = make_ctx(tmp.path().to_path_buf(), sid);
        let tool = SendMessageTool::new();
        tool.execute(
            serde_json::json!({
                "channel": "session_log",
                "content": "note",
            }),
            &ctx,
        )
        .await
        .unwrap();

        let file = tmp.path().join(format!(".vac/messages/{sid}.log"));
        assert!(file.exists());
    }
}
