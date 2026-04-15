use crate::error::{VacError, VacResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
pub struct ApprovalIntent {
    pub intent_id: Uuid,
    pub approved: bool,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
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
    #[serde(default)]
    pub intent: Option<ApprovalIntent>,
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
                intent: None,
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
                self.record.intent = None;
            }
            ApprovalEvent::Approve { reason } => {
                if self.record.resolved_at.is_some() {
                    return Err(ApprovalTransitionError::AlreadyResolved);
                }
                self.record.state = ApprovalState::Approved;
                self.record.resolved_at = Some(Utc::now());
                self.record.reason = reason;
                self.record.intent = None;
            }
            ApprovalEvent::Reject { reason } => {
                if self.record.resolved_at.is_some() {
                    return Err(ApprovalTransitionError::AlreadyResolved);
                }
                self.record.state = ApprovalState::Rejected;
                self.record.resolved_at = Some(Utc::now());
                self.record.reason = reason;
                self.record.intent = None;
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

    pub fn record_intent(
        &self,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    ) -> VacResult<ApprovalRecord> {
        std::fs::create_dir_all(&self.approvals_dir)?;

        let mut record = self.load(&tool_call_id)?.ok_or_else(|| {
            VacError::Task(format!(
                "Unknown tool_call_id (no approval record found): {tool_call_id}"
            ))
        })?;

        if record.state != ApprovalState::Pending {
            return Err(VacError::Task(format!(
                "Approval is not pending (tool_call_id={tool_call_id}, state={:?})",
                record.state
            )));
        }

        record.intent = Some(ApprovalIntent {
            intent_id: Uuid::new_v4(),
            approved,
            reason,
            created_at: Utc::now(),
        });
        self.write(&record)?;
        Ok(record)
    }

    pub fn clear_intent(&self, tool_call_id: &str) -> VacResult<()> {
        let Some(mut record) = self.load(tool_call_id)? else {
            return Ok(());
        };
        if record.intent.is_some() {
            record.intent = None;
            self.write(&record)?;
        }
        Ok(())
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
    active: ActiveApprovalRegistry,
    store: ApprovalStore,
}

impl ApprovalHandle {
    pub fn new(
        project_root: PathBuf,
        active: ActiveApprovalRegistry,
    ) -> Self {
        Self {
            active,
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
        let mut record = None;
        for attempt in 0..5 {
            let store = self.store.clone();
            let id = tool_call_id.clone();
            record = tokio::task::spawn_blocking(move || store.load(&id))
                .await
                .map_err(|e| VacError::Task(format!("Approval store task failed: {e}")))??;
            if record.is_some() {
                break;
            }
            if attempt < 4 {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        }
        let record = record.ok_or_else(|| {
            VacError::Task(format!(
                "Unknown tool_call_id (no approval record found): {tool_call_id}"
            ))
        })?;

        if record.state != ApprovalState::Pending {
            return Err(VacError::Task(format!(
                "Approval is not pending (tool_call_id={tool_call_id}, state={:?})",
                record.state
            )));
        }

        let task_id = record.task_id.ok_or_else(|| {
            VacError::Task(format!(
                "Approval record missing task_id (tool_call_id={tool_call_id})"
            ))
        })?;
        let session_id = record.session_id.ok_or_else(|| {
            VacError::Task(format!(
                "Approval record missing session_id (tool_call_id={tool_call_id})"
            ))
        })?;

        self.active
            .send(
                task_id,
                session_id,
                vil_swarm::ApprovalResponse {
                    tool_call_id: tool_call_id.clone(),
                    approved,
                    reason: reason.clone(),
                },
            )
            .await?;

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

#[derive(Debug, Clone)]
pub struct ActiveApprovalEntry {
    pub session_id: Uuid,
    pub tx: mpsc::UnboundedSender<vil_swarm::ApprovalResponse>,
}

#[derive(Debug, Clone, Default)]
pub struct ActiveApprovalRegistry {
    inner: Arc<tokio::sync::Mutex<HashMap<Uuid, ActiveApprovalEntry>>>,
}

impl ActiveApprovalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(
        &self,
        task_id: Uuid,
        session_id: Uuid,
        tx: mpsc::UnboundedSender<vil_swarm::ApprovalResponse>,
    ) {
        {
            let mut map = self.inner.lock().await;
            map.insert(task_id, ActiveApprovalEntry { session_id, tx: tx.clone() });
        }

        let registry = self.clone();
        tokio::spawn(async move {
            tx.closed().await;
            let mut map = registry.inner.lock().await;
            map.remove(&task_id);
        });
    }

    pub async fn send(
        &self,
        task_id: Uuid,
        session_id: Uuid,
        response: vil_swarm::ApprovalResponse,
    ) -> VacResult<()> {
        let tx = {
            let map = self.inner.lock().await;
            let Some(entry) = map.get(&task_id) else {
                return Err(VacError::Task(format!(
                    "No active approval channel for task_id={task_id} (stale or wrong target)"
                )));
            };
            if entry.session_id != session_id {
                return Err(VacError::Task(format!(
                    "Approval target session mismatch for task_id={task_id} (expected {}, got {})",
                    entry.session_id, session_id
                )));
            }
            entry.tx.clone()
        };

        tx.send(response).map_err(|e| {
            let registry = self.clone();
            tokio::spawn(async move {
                let mut map = registry.inner.lock().await;
                map.remove(&task_id);
            });
            VacError::Task(format!(
                "Approval channel closed for task_id={task_id}: {e}"
            ))
        })
    }

    pub async fn is_active(&self, task_id: Uuid) -> bool {
        self.inner.lock().await.contains_key(&task_id)
    }
}
