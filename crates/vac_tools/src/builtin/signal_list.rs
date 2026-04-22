//! `signal_list` — enumerate session rewind databases and their streams.
//!
//! Companion to `signal_tail`: helps the agent discover which stream ids
//! are available before tailing. Reads `.vac/signal/*.db` under the
//! project root and queries each DB for distinct stream ids.

use async_trait::async_trait;
use serde::Serialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Serialize)]
struct Output {
    project_root: String,
    current_session_id: String,
    databases: Vec<DbEntry>,
}

#[derive(Debug, Serialize)]
struct DbEntry {
    path: String,
    session_id: String,
    is_current: bool,
    streams: Vec<String>,
}

pub struct SignalListTool;

impl SignalListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SignalListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for SignalListTool {
    fn name(&self) -> &str {
        "signal_list"
    }

    fn description(&self) -> &str {
        "List available signal rewind databases under .vac/signal/ and the stream ids inside each. Use first to discover what to pass to signal_tail."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
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
        _args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let dir = context.working_dir.join(".vac").join("signal");
        let mut databases: Vec<DbEntry> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("db") {
                    continue;
                }
                let session_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let is_current = session_id == context.session_id.to_string();
                let streams = query_streams(&path).unwrap_or_default();
                databases.push(DbEntry {
                    path: path.to_string_lossy().to_string(),
                    session_id,
                    is_current,
                    streams,
                });
            }
        }
        databases.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        let output = Output {
            project_root: context.working_dir.to_string_lossy().to_string(),
            current_session_id: context.session_id.to_string(),
            databases,
        };
        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

fn query_streams(db_path: &std::path::Path) -> Result<Vec<String>, ToolError> {
    let store = vac_signal::rewind::RewindStore::open(db_path)
        .map_err(|e| ToolError::ExecutionFailed(format!("open db: {e}")))?;
    store
        .list_streams()
        .map_err(|e| ToolError::ExecutionFailed(format!("list streams: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;

    #[tokio::test]
    async fn empty_when_no_signal_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let c = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = SignalListTool::new()
            .execute(serde_json::json!({}), &c)
            .await
            .unwrap();
        assert_eq!(out["databases"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn enumerates_databases_and_streams() {
        let tmp = tempfile::tempdir().unwrap();
        let current = uuid::Uuid::new_v4();
        let other = uuid::Uuid::new_v4();
        let dir = tmp.path().join(".vac").join("signal");
        std::fs::create_dir_all(&dir).unwrap();
        for sid in [current, other] {
            let p = dir.join(format!("{}.db", sid));
            let mut store = vac_signal::rewind::RewindStore::open(&p).unwrap();
            store
                .append(
                    "vil_dev",
                    vac_signal::SignalStreamKind::VilDev,
                    &vac_signal::SignalLine { seq: 0, text: "x".into() },
                    0,
                )
                .unwrap();
        }
        let c = make_ctx(tmp.path().to_path_buf(), current);
        let out = SignalListTool::new()
            .execute(serde_json::json!({}), &c)
            .await
            .unwrap();
        let dbs = out["databases"].as_array().unwrap();
        assert_eq!(dbs.len(), 2);
        let current_entry = dbs.iter().find(|d| d["is_current"] == true).unwrap();
        assert_eq!(current_entry["streams"].as_array().unwrap().len(), 1);
    }
}
