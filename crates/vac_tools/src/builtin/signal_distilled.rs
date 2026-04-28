//! NS.6 — `signal_distilled` tool. Loads recent lines from the
//! session's rewind store into an in-memory `SignalBuffer` and
//! returns the scored/distilled view (key lines, tail, noise
//! dropped). Complements `signal_tail` which returns raw lines;
//! this one returns the distiller's verdict on which lines matter.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

const DEFAULT_N: i64 = 500;
const MAX_N: i64 = 10_000;
const DEFAULT_TAIL: usize = 20;

#[derive(Debug, Deserialize)]
struct Input {
    stream_id: String,
    /// Max number of lines to pull from the rewind store before
    /// distilling. Default 500.
    #[serde(default)]
    n: Option<i64>,
    /// Tail size in the distilled view. Default 20.
    #[serde(default)]
    tail_size: Option<usize>,
}

pub struct SignalDistilledTool;

impl SignalDistilledTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SignalDistilledTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for SignalDistilledTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "signal_distilled"
    }

    fn description(&self) -> &str {
        "Return the scored/distilled view of a captured output stream (key lines + recent tail + noise-dropped count). Cheaper than signal_tail when you only need the signal, not every raw line."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "stream_id": { "type": "string", "description": "Stream id (e.g. `shell`, `vil_dev`, `build-abcd`)." },
                "n": { "type": "integer", "minimum": 1, "maximum": MAX_N, "description": "Max lines pulled from rewind before distilling (default 500)." },
                "tail_size": { "type": "integer", "minimum": 1, "maximum": 200, "description": "Tail size in the distilled view (default 20)." }
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
        let n = input.n.unwrap_or(DEFAULT_N).clamp(1, MAX_N);
        let tail_size = input.tail_size.unwrap_or(DEFAULT_TAIL).clamp(1, 200);
        let db_path = context
            .working_dir
            .join(".vac")
            .join("signal")
            .join(format!("{}.db", context.session_id));

        if !db_path.exists() {
            return Ok(serde_json::json!({
                "stream_id": input.stream_id,
                "session_id": context.session_id.to_string(),
                "db_path": db_path.to_string_lossy(),
                "key_lines": Vec::<String>::new(),
                "tail": Vec::<String>::new(),
                "dropped_noise": 0,
                "dropped_before_key": 0,
                "empty_reason": "session rewind DB not yet created",
            }));
        }
        let store = vac_signal::rewind::RewindStore::open(&db_path)
            .map_err(|e| ToolError::ExecutionFailed(format!("open rewind store: {e}")))?;
        let lines = store
            .recent(&input.stream_id, n)
            .map_err(|e| ToolError::ExecutionFailed(format!("query: {e}")))?;
        let total = lines.len();
        let mut buf =
            vac_signal::SignalBuffer::new(vac_signal::SignalStreamKind::Other, (n as usize).max(1));
        // NB: seq numbers on the transient buffer are fresh 0..N
        // (assigned by `push_line`). The original per-session seqs
        // from the rewind DB are discarded here. Current
        // `TailDistiller` is seq-agnostic so this is invisible
        // today; revisit if a distiller grows seq-gap heuristics.
        for l in lines {
            buf.push_line(l.text);
        }
        let view = buf.distilled_default(tail_size);
        Ok(serde_json::json!({
            "stream_id": input.stream_id,
            "session_id": context.session_id.to_string(),
            "db_path": db_path.to_string_lossy(),
            "pulled_lines": total,
            "key_lines": view.key_lines,
            "tail": view.tail,
            "dropped_noise": view.dropped_noise,
            "dropped_before_key": view.dropped_before_key,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;

    #[tokio::test]
    async fn empty_reason_when_no_db() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = SignalDistilledTool::new()
            .execute(serde_json::json!({ "stream_id": "shell" }), &ctx)
            .await
            .unwrap();
        assert!(out["empty_reason"].is_string());
        assert!(out["tail"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn distills_lines_from_rewind_store() {
        let tmp = tempfile::tempdir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let signal_dir = tmp.path().join(".vac/signal");
        std::fs::create_dir_all(&signal_dir).unwrap();
        let db_path = signal_dir.join(format!("{}.db", session_id));
        let mut store = vac_signal::rewind::RewindStore::open(&db_path).unwrap();
        let kind = vac_signal::SignalStreamKind::Shell;
        for (seq, text) in [(1u64, "hello"), (2, "ERROR boom"), (3, "tail line")] {
            let line = vac_signal::SignalLine {
                seq,
                text: text.into(),
            };
            store.append("shell", kind, &line, 0).unwrap();
        }

        let ctx = make_ctx(tmp.path().to_path_buf(), session_id);
        let out = SignalDistilledTool::new()
            .execute(
                serde_json::json!({ "stream_id": "shell", "tail_size": 5 }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(out["pulled_lines"], 3);
        // Tail carries the recent lines verbatim.
        let tail = out["tail"].as_array().unwrap();
        assert!(tail.len() >= 1);
    }
}
