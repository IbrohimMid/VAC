//! `signal_tail` — recall tail from the session's rewind store.
//!
//! OMNI-inspired retrieval tool. Reads the per-session rewind database
//! that `vac_tui_runtime` maintains under `.vac/signal/<session_id>.db`,
//! and returns the most recent N lines for the requested stream.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

const DEFAULT_TAIL: i64 = 50;
const MAX_TAIL: i64 = 2_000;

#[derive(Debug, Deserialize)]
struct Input {
    stream_id: String,
    #[serde(default)]
    n: Option<i64>,
}

#[derive(Debug, Serialize)]
struct Output {
    stream_id: String,
    session_id: String,
    db_path: String,
    lines: Vec<LineOut>,
    /// `true` when no DB existed yet (e.g. session just started, feature
    /// disabled, or signal layer off).
    empty_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct LineOut {
    seq: u64,
    text: String,
}

pub struct SignalTailTool;

impl SignalTailTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SignalTailTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for SignalTailTool {
    fn name(&self) -> &str {
        "signal_tail"
    }

    fn description(&self) -> &str {
        "Recall the last N lines of a captured output stream (shell, vil_dev, runtime) from the session's signal rewind database. Use when you need raw context for a recently-run command without re-running it."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "stream_id": {
                    "type": "string",
                    "description": "Stream identifier, e.g. \"vil_dev\" or \"shell:0:<uuid>\". Use `signal_list` to discover available streams."
                },
                "n": {
                    "type": "integer",
                    "description": "Number of most-recent lines to return (default 50, max 2000).",
                    "minimum": 1,
                    "maximum": MAX_TAIL
                }
            },
            "required": ["stream_id"]
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
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: Input = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;

        let n = input.n.unwrap_or(DEFAULT_TAIL).clamp(1, MAX_TAIL);
        let db_path = context
            .working_dir
            .join(".vac")
            .join("signal")
            .join(format!("{}.db", context.session_id));

        if !db_path.exists() {
            let output = Output {
                stream_id: input.stream_id,
                session_id: context.session_id.to_string(),
                db_path: db_path.to_string_lossy().to_string(),
                lines: vec![],
                empty_reason: Some(
                    "session rewind DB not yet created; signal layer may be off or no output produced".to_string(),
                ),
            };
            return serde_json::to_value(output)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()));
        }

        let store = vac_signal::rewind::RewindStore::open(&db_path)
            .map_err(|e| ToolError::ExecutionFailed(format!("open rewind store: {e}")))?;
        let lines = store
            .recent(&input.stream_id, n)
            .map_err(|e| ToolError::ExecutionFailed(format!("query recent: {e}")))?;

        let output = Output {
            stream_id: input.stream_id,
            session_id: context.session_id.to_string(),
            db_path: db_path.to_string_lossy().to_string(),
            lines: lines
                .into_iter()
                .map(|l| LineOut { seq: l.seq, text: l.text })
                .collect(),
            empty_reason: None,
        };
        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn ctx(working_dir: std::path::PathBuf, session_id: uuid::Uuid) -> ToolContext {
        ToolContext {
            working_dir,
            env_vars: std::collections::HashMap::new(),
            session_id,
            shm: None,
            agent_zone: crate::registry::AgentZone::ParentAgent,
            environment_mode: "host".to_string(),
            privacy: Arc::new(RwLock::new(crate::PrivacyVault::new())),
        }
    }

    #[tokio::test]
    async fn returns_empty_reason_when_no_db() {
        let tmp = tempfile::tempdir().unwrap();
        let c = ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let tool = SignalTailTool::new();
        let out = tool
            .execute(serde_json::json!({ "stream_id": "vil_dev" }), &c)
            .await
            .unwrap();
        assert!(out["empty_reason"].is_string());
        assert_eq!(out["lines"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn reads_recent_lines_from_db() {
        let tmp = tempfile::tempdir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let signal_dir = tmp.path().join(".vac").join("signal");
        std::fs::create_dir_all(&signal_dir).unwrap();
        let db_path = signal_dir.join(format!("{}.db", session_id));
        let mut store = vac_signal::rewind::RewindStore::open(&db_path).unwrap();
        for i in 0..3u64 {
            store
                .append(
                    "vil_dev",
                    vac_signal::SignalStreamKind::VilDev,
                    &vac_signal::SignalLine { seq: i, text: format!("line-{i}") },
                    1_700_000_000,
                )
                .unwrap();
        }
        let c = ctx(tmp.path().to_path_buf(), session_id);
        let tool = SignalTailTool::new();
        let out = tool
            .execute(serde_json::json!({ "stream_id": "vil_dev", "n": 2 }), &c)
            .await
            .unwrap();
        let lines = out["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["text"], "line-2");
    }
}
