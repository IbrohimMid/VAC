use serde::{Deserialize, Serialize};

/// Typed envelope returned by every tool. Normalises over ad-hoc
/// `serde_json::Value` responses so UI/trace/transcript surfaces can
/// render uniformly without sniffing shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultEnvelope {
    pub kind: ToolResultKind,
    /// The structured payload (tool-specific).
    pub payload: serde_json::Value,
    /// Operator-readable summary (1–2 lines max). Rendered in the
    /// review pane even when payload is collapsed.
    pub summary: String,
    /// Duration the tool took to execute, in milliseconds.
    pub duration_ms: u64,
}

/// Coarse classification of the result — UIs use this to pick colour,
/// icon, and render behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolResultKind {
    /// Tool completed successfully.
    Ok,
    /// Tool completed but surfaced warnings/soft errors in the payload.
    Warning,
    /// Tool failed. Payload holds the error body.
    Error,
    /// Tool was cancelled (operator abort, timeout, policy denial).
    Cancelled,
}

impl ToolResultEnvelope {
    /// Convenience for the common success path.
    pub fn ok(summary: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            kind: ToolResultKind::Ok,
            payload,
            summary: summary.into(),
            duration_ms: 0,
        }
    }

    /// Convenience for error path.
    pub fn error(summary: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: ToolResultKind::Error,
            payload: serde_json::json!({ "error": message.into() }),
            summary: summary.into(),
            duration_ms: 0,
        }
    }

    /// Builder helper to record timing after execution.
    pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }
}
