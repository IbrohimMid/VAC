use crate::error::{VacError, VacResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::warn;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ApprovalEvent {
    CreatePending {
        tool_name: String,
        scope: String,
        arguments: serde_json::Value,
        explanation: Option<String>,
        session_id: Option<Uuid>,
        task_id: Option<Uuid>,
    },
    Approve {
        reason: Option<String>,
    },
    Reject {
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub version: u32,
    pub tool_call_id: String,
    pub tool_name: String,
    pub scope: String,
    pub arguments: serde_json::Value,
    pub explanation: Option<String>,
    pub state: ApprovalState,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub session_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
}

#[derive(thiserror::Error, Debug)]
pub enum ApprovalTransitionError {
    #[error("approval already resolved")]
    AlreadyResolved,
}

#[derive(Debug, Clone)]
pub struct ApprovalStateMachine {
    record: ApprovalRecord,
}

impl ApprovalStateMachine {
    pub fn record(&self) -> &ApprovalRecord {
        &self.record
    }

    pub fn into_record(self) -> ApprovalRecord {
        self.record
    }

    pub fn new(tool_call_id: String) -> Self {
        Self {
            record: ApprovalRecord {
                version: 1,
                tool_call_id,
                tool_name: "unknown".to_string(),
                scope: "unknown".to_string(),
                arguments: serde_json::Value::Null,
                explanation: None,
                state: ApprovalState::Pending,
                created_at: Utc::now(),
                resolved_at: None,
                reason: None,
                session_id: None,
                task_id: None,
            },
        }
    }

    pub fn apply(mut self, event: ApprovalEvent) -> Result<Self, ApprovalTransitionError> {
        match event {
            ApprovalEvent::CreatePending {
                tool_name,
                scope,
                arguments,
                explanation,
                session_id,
                task_id,
            } => {
                self.record.tool_name = tool_name;
                self.record.scope = scope;
                self.record.arguments = arguments;
                self.record.explanation = explanation;
                self.record.session_id = session_id;
                self.record.task_id = task_id;
                self.record.state = ApprovalState::Pending;
                self.record.created_at = Utc::now();
                self.record.resolved_at = None;
                self.record.reason = None;
            }
            ApprovalEvent::Approve { reason } => {
                if self.record.resolved_at.is_some() {
                    return Err(ApprovalTransitionError::AlreadyResolved);
                }
                self.record.state = ApprovalState::Approved;
                self.record.resolved_at = Some(Utc::now());
                self.record.reason = reason;
            }
            ApprovalEvent::Reject { reason } => {
                if self.record.resolved_at.is_some() {
                    return Err(ApprovalTransitionError::AlreadyResolved);
                }
                self.record.state = ApprovalState::Rejected;
                self.record.resolved_at = Some(Utc::now());
                self.record.reason = reason;
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct ApprovalStore {
    approvals_dir: PathBuf,
}

impl ApprovalStore {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            approvals_dir: project_root.join(".vac").join("approvals"),
        }
    }

    pub fn approvals_dir(&self) -> &Path {
        &self.approvals_dir
    }

    pub fn record_request(
        &self,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        explanation: Option<String>,
        session_id: Option<Uuid>,
        task_id: Option<Uuid>,
    ) -> VacResult<ApprovalRecord> {
        std::fs::create_dir_all(&self.approvals_dir)?;

        let scope = derive_scope(&tool_name, &arguments);
        let sm =
            ApprovalStateMachine::new(tool_call_id.clone()).apply(ApprovalEvent::CreatePending {
                tool_name,
                scope,
                arguments,
                explanation,
                session_id,
                task_id,
            });

        let sm = match sm {
            Ok(sm) => sm,
            Err(e) => return Err(VacError::Task(e.to_string())),
        };

        let record = sm.clone().into_record();
        self.write(&record)?;
        Ok(record)
    }

    pub fn record_decision(
        &self,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    ) -> VacResult<ApprovalRecord> {
        std::fs::create_dir_all(&self.approvals_dir)?;

        let current = self
            .load(&tool_call_id)?
            .unwrap_or_else(|| ApprovalStateMachine::new(tool_call_id.clone()).into_record());
        let sm = ApprovalStateMachine { record: current }.apply(if approved {
            ApprovalEvent::Approve { reason }
        } else {
            ApprovalEvent::Reject { reason }
        });

        let sm = match sm {
            Ok(sm) => sm,
            Err(ApprovalTransitionError::AlreadyResolved) => {
                warn!(tool_call_id = %tool_call_id, "Approval decision ignored: already resolved");
                return Ok(self.load(&tool_call_id)?.unwrap());
            }
        };

        let record = sm.clone().into_record();
        self.write(&record)?;
        Ok(record)
    }

    pub fn load(&self, tool_call_id: &str) -> VacResult<Option<ApprovalRecord>> {
        let path = self.approval_path(tool_call_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    fn write(&self, record: &ApprovalRecord) -> VacResult<()> {
        let path = self.approval_path(&record.tool_call_id);
        let tmp = path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(record)?;
        std::fs::write(&tmp, content)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    fn approval_path(&self, tool_call_id: &str) -> PathBuf {
        let base = safe_filename(tool_call_id);
        self.approvals_dir.join(format!("{base}.json"))
    }
}

#[derive(Clone)]
pub struct ApprovalHandle {
    active_approval_tx: std::sync::Arc<
        tokio::sync::Mutex<Option<mpsc::UnboundedSender<vil_swarm::ApprovalResponse>>>,
    >,
    store: ApprovalStore,
}

impl ApprovalHandle {
    pub fn new(
        project_root: PathBuf,
        active_approval_tx: std::sync::Arc<
            tokio::sync::Mutex<Option<mpsc::UnboundedSender<vil_swarm::ApprovalResponse>>>,
        >,
    ) -> Self {
        Self {
            active_approval_tx,
            store: ApprovalStore::new(project_root),
        }
    }

    pub fn store(&self) -> &ApprovalStore {
        &self.store
    }

    pub async fn approve(&self, tool_call_id: String) -> VacResult<()> {
        self.send(tool_call_id, true, None).await
    }

    pub async fn reject(&self, tool_call_id: String, reason: Option<String>) -> VacResult<()> {
        self.send(tool_call_id, false, reason).await
    }

    async fn send(
        &self,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    ) -> VacResult<()> {
        {
            let tx_guard = self.active_approval_tx.lock().await;
            if let Some(tx) = &*tx_guard {
                tx.send(vil_swarm::ApprovalResponse {
                    tool_call_id: tool_call_id.clone(),
                    approved,
                    reason: reason.clone(),
                })
                .map_err(|e| VacError::Task(format!("Failed to send approval response: {}", e)))?;
            } else {
                return Err(VacError::Task(
                    "No active task requires approval".to_string(),
                ));
            }
        }

        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.record_decision(tool_call_id, approved, reason))
            .await
            .map_err(|e| VacError::Task(format!("Approval store task failed: {}", e)))??;

        Ok(())
    }
}

fn safe_filename(id: &str) -> String {
    if !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return id.to_string();
    }
    format!("h_{}", blake3::hash(id.as_bytes()).to_hex())
}

fn derive_scope(tool_name: &str, arguments: &serde_json::Value) -> String {
    if tool_name == "bash" {
        if let Some(command) = arguments.get("command").and_then(|v| v.as_str()) {
            let scopes = vac_tools::approvals::parse_command_scopes(command);
            if let Some(most_specific) = scopes.last() {
                return most_specific.clone();
            }
        }
    }
    tool_name.to_string()
}
