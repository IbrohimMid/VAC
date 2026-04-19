use chrono::{DateTime, Utc};
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, mpsc};
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
pub enum ApprovalError {
    #[error("task error: {0}")]
    Task(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("approval already resolved")]
    AlreadyResolved,
}

pub type ApprovalResult<T> = Result<T, ApprovalError>;

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
    pending_records: Arc<Mutex<HashMap<String, ApprovalRecord>>>,
    record_notifiers: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl ApprovalStore {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            approvals_dir: project_root.join(".vac").join("approvals"),
            pending_records: Arc::new(Mutex::new(HashMap::new())),
            record_notifiers: Arc::new(Mutex::new(HashMap::new())),
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
    ) -> ApprovalResult<ApprovalRecord> {
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
            Err(e) => return Err(ApprovalError::Task(e.to_string())),
        };

        let record = sm.clone().into_record();
        match self.write(&record) {
            Ok(()) => {
                self.cache_record(record.clone());
                self.notify_record_ready(&record.tool_call_id);
            }
            Err(err) => {
                self.remove_cached_record(&record.tool_call_id);
                self.notify_record_ready(&record.tool_call_id);
                return Err(err);
            }
        }
        Ok(record)
    }

    pub fn record_decision(
        &self,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    ) -> ApprovalResult<ApprovalRecord> {
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
                // Safe: we just loaded this record successfully above; it must still exist.
                #[allow(clippy::unwrap_used)]
                return Ok(self.load(&tool_call_id)?.unwrap());
            }
        };

        let record = sm.clone().into_record();
        self.write(&record)?;
        self.cache_record(record.clone());
        self.notify_record_ready(&record.tool_call_id);
        Ok(record)
    }

    pub fn record_intent(
        &self,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    ) -> ApprovalResult<ApprovalRecord> {
        std::fs::create_dir_all(&self.approvals_dir)?;

        let mut record = self.load(&tool_call_id)?.ok_or_else(|| {
            ApprovalError::Task(format!(
                "Unknown tool_call_id (no approval record found): {tool_call_id}"
            ))
        })?;

        if record.state != ApprovalState::Pending {
            return Err(ApprovalError::Task(format!(
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
        self.cache_record(record.clone());
        self.notify_record_ready(&record.tool_call_id);
        Ok(record)
    }

    pub fn clear_intent(&self, tool_call_id: &str) -> ApprovalResult<()> {
        let Some(mut record) = self.load_from_fs(tool_call_id)? else {
            return Ok(());
        };
        if record.intent.is_some() {
            record.intent = None;
            self.write(&record)?;
            self.cache_record(record.clone());
            self.notify_record_ready(&record.tool_call_id);
        }
        Ok(())
    }

    pub fn load_from_fs(&self, tool_call_id: &str) -> ApprovalResult<Option<ApprovalRecord>> {
        let path = self.approval_path(tool_call_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    fn approval_watch_events(
        &self,
    ) -> ApprovalResult<(
        notify::RecommendedWatcher,
        mpsc::UnboundedReceiver<notify::Result<notify::Event>>,
    )> {
        std::fs::create_dir_all(&self.approvals_dir)?;

        let (tx, rx) = mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .map_err(|e| ApprovalError::Task(format!("approval watcher error: {e}")))?;
        watcher
            .watch(&self.approvals_dir, RecursiveMode::NonRecursive)
            .map_err(|e| ApprovalError::Task(format!("approval watch error: {e}")))?;

        Ok((watcher, rx))
    }

    pub fn load(&self, tool_call_id: &str) -> ApprovalResult<Option<ApprovalRecord>> {
        if let Some(record) = self.cached_record(tool_call_id) {
            return Ok(Some(record));
        }
        let path = self.approval_path(tool_call_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    pub async fn wait_for_record(
        &self,
        tool_call_id: &str,
        timeout: std::time::Duration,
    ) -> ApprovalResult<Option<ApprovalRecord>> {
        let notify = self.record_notifier(tool_call_id);
        let notified = notify.notified();

        if let Some(record) = self.load_from_fs(tool_call_id)? {
            self.record_notifiers
                .lock()
                .expect("approval notifier cache poisoned")
                .remove(tool_call_id);
            return Ok(Some(record));
        }

        let _ = tokio::time::timeout(timeout, notified).await;
        self.record_notifiers
            .lock()
            .expect("approval notifier cache poisoned")
            .remove(tool_call_id);
        self.load_from_fs(tool_call_id)
    }

    pub async fn wait_for_intent(
        &self,
        tool_call_id: &str,
        total_timeout: std::time::Duration,
    ) -> ApprovalResult<Option<ApprovalIntent>> {
        let deadline = tokio::time::Instant::now() + total_timeout;

        if let Some(record) = self.load_from_fs(tool_call_id)? {
            if let Some(intent) = record.intent {
                return Ok(Some(intent));
            }
            if record.state != ApprovalState::Pending {
                return Err(ApprovalError::Task(format!(
                    "Approval is not pending (tool_call_id={}, state={:?})",
                    tool_call_id, record.state
                )));
            }
        }

        let (_watcher, mut events) = self.approval_watch_events()?;

        if let Some(record) = self.load_from_fs(tool_call_id)? {
            if let Some(intent) = record.intent {
                return Ok(Some(intent));
            }
            if record.state != ApprovalState::Pending {
                return Err(ApprovalError::Task(format!(
                    "Approval is not pending (tool_call_id={}, state={:?})",
                    tool_call_id, record.state
                )));
            }
        }

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }

            match tokio::time::timeout(remaining, events.recv()).await {
                Ok(Some(Ok(_event))) => {
                    if let Some(record) = self.load_from_fs(tool_call_id)? {
                        if let Some(intent) = record.intent {
                            return Ok(Some(intent));
                        }
                        if record.state != ApprovalState::Pending {
                            return Err(ApprovalError::Task(format!(
                                "Approval is not pending (tool_call_id={}, state={:?})",
                                tool_call_id, record.state
                            )));
                        }
                    }
                }
                Ok(Some(Err(err))) => {
                    warn!(tool_call_id = %tool_call_id, error = %err, "approval watcher event error");
                }
                Ok(None) => return Ok(None),
                Err(_) => return Ok(None),
            }
        }
    }

    pub fn write_record(&self, record: &ApprovalRecord) -> ApprovalResult<()> {
        std::fs::create_dir_all(&self.approvals_dir)?;
        self.write(record)
    }

    pub fn list_by_session(&self, session_id: Uuid) -> ApprovalResult<Vec<ApprovalRecord>> {
        let entries = match std::fs::read_dir(&self.approvals_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        let mut out = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!(?path, error = %e, "approval store: skipping unreadable record");
                    continue;
                }
            };
            let record = match serde_json::from_str::<ApprovalRecord>(&content) {
                Ok(r) => r,
                Err(e) => {
                    warn!(?path, error = %e, "approval store: skipping malformed record");
                    continue;
                }
            };
            if record.session_id == Some(session_id) {
                out.push(record);
            }
        }
        out.sort_by_key(|r| r.created_at);
        Ok(out)
    }

    pub fn remove_by_session(&self, session_id: Uuid) -> ApprovalResult<usize> {
        let records = self.list_by_session(session_id)?;
        let mut removed = 0usize;

        for record in records {
            let path = self.approval_path(&record.tool_call_id);
            if path.exists() {
                std::fs::remove_file(&path)?;
                removed += 1;
            }
            self.record_notifiers
                .lock()
                .expect("approval notifier cache poisoned")
                .remove(&record.tool_call_id);
        }

        Ok(removed)
    }

    /// Reject all pending approvals for a session that are older than `max_age`.
    ///
    /// Returns the list of tool_call_ids that were rejected as stale.
    pub fn reject_stale(
        &self,
        session_id: Uuid,
        max_age: chrono::Duration,
    ) -> ApprovalResult<Vec<String>> {
        let now = Utc::now();
        let records = self.list_by_session(session_id)?;
        let mut rejected_ids = Vec::new();

        for record in records {
            if record.state != ApprovalState::Pending {
                continue;
            }
            let age = now.signed_duration_since(record.created_at);
            if age > max_age {
                self.record_decision(
                    record.tool_call_id.clone(),
                    false,
                    Some("Stale approval auto-rejected".to_string()),
                )?;
                rejected_ids.push(record.tool_call_id);
            }
        }

        Ok(rejected_ids)
    }

    fn write(&self, record: &ApprovalRecord) -> ApprovalResult<()> {
        let path = self.approval_path(&record.tool_call_id);
        let tmp = path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(record)?;
        std::fs::write(&tmp, content)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    fn cache_record(&self, record: ApprovalRecord) {
        self.pending_records
            .lock()
            .expect("approval cache poisoned")
            .insert(record.tool_call_id.clone(), record);
    }

    fn cached_record(&self, tool_call_id: &str) -> Option<ApprovalRecord> {
        self.pending_records
            .lock()
            .expect("approval cache poisoned")
            .get(tool_call_id)
            .cloned()
    }

    fn remove_cached_record(&self, tool_call_id: &str) {
        self.pending_records
            .lock()
            .expect("approval cache poisoned")
            .remove(tool_call_id);
    }

    fn record_notifier(&self, tool_call_id: &str) -> Arc<Notify> {
        let mut notifiers = self
            .record_notifiers
            .lock()
            .expect("approval notifier cache poisoned");
        notifiers
            .entry(tool_call_id.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    fn notify_record_ready(&self, tool_call_id: &str) {
        let notify = self
            .record_notifiers
            .lock()
            .expect("approval notifier cache poisoned")
            .remove(tool_call_id);
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
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
    pub fn new(project_root: PathBuf, active: ActiveApprovalRegistry) -> Self {
        Self {
            active,
            store: ApprovalStore::new(project_root),
        }
    }

    pub fn store(&self) -> &ApprovalStore {
        &self.store
    }

    pub async fn approve(&self, tool_call_id: String) -> ApprovalResult<()> {
        self.send(tool_call_id, true, None).await
    }

    pub async fn reject(&self, tool_call_id: String, reason: Option<String>) -> ApprovalResult<()> {
        self.send(tool_call_id, false, reason).await
    }

    async fn send(
        &self,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    ) -> ApprovalResult<()> {
        let record = self
            .store
            .wait_for_record(&tool_call_id, std::time::Duration::from_millis(500))
            .await?
            .ok_or_else(|| {
                ApprovalError::Task(format!(
                    "Unknown tool_call_id (no approval record found): {tool_call_id}"
                ))
            })?;

        if record.state != ApprovalState::Pending {
            return Err(ApprovalError::Task(format!(
                "Approval is not pending (tool_call_id={tool_call_id}, state={:?})",
                record.state
            )));
        }

        let task_id = record.task_id.ok_or_else(|| {
            ApprovalError::Task(format!(
                "Approval record missing task_id (tool_call_id={tool_call_id})"
            ))
        })?;
        let session_id = record.session_id.ok_or_else(|| {
            ApprovalError::Task(format!(
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
            .map_err(|e| ApprovalError::Task(format!("Approval store task failed: {}", e)))??;

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
            map.insert(
                task_id,
                ActiveApprovalEntry {
                    session_id,
                    tx: tx.clone(),
                },
            );
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
    ) -> ApprovalResult<()> {
        let tx = {
            let map = self.inner.lock().await;
            let Some(entry) = map.get(&task_id) else {
                return Err(ApprovalError::Task(format!(
                    "No active approval channel for task_id={task_id} (stale or wrong target)"
                )));
            };
            if entry.session_id != session_id {
                return Err(ApprovalError::Task(format!(
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
            ApprovalError::Task(format!(
                "Approval channel closed for task_id={task_id}: {e}"
            ))
        })
    }

    pub async fn is_active(&self, task_id: Uuid) -> bool {
        self.inner.lock().await.contains_key(&task_id)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_store() -> (TempDir, ApprovalStore) {
        let tmp = TempDir::new().unwrap();
        let store = ApprovalStore::new(tmp.path().to_path_buf());
        (tmp, store)
    }

    #[test]
    fn test_batch_ordering_deterministic() {
        let (_tmp, store) = make_store();
        let session_id = Uuid::new_v4();

        // Create 5 approvals with known ordering
        let mut ids = Vec::new();
        for i in 0..5 {
            let tool_call_id = format!("tc-{}", i);
            let _record = store
                .record_request(
                    tool_call_id.clone(),
                    format!("tool_{}", i),
                    serde_json::json!({}),
                    None,
                    Some(session_id),
                    Some(Uuid::new_v4()),
                )
                .unwrap();
            ids.push(tool_call_id);
            // Small sleep to ensure distinct timestamps
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Retrieve twice to verify determinism
        let first = store.list_by_session(session_id).unwrap();
        let second = store.list_by_session(session_id).unwrap();

        assert_eq!(first.len(), 5);
        assert_eq!(second.len(), 5);

        // Both should be in created_at ascending order
        let first_ids: Vec<String> = first.iter().map(|r| r.tool_call_id.clone()).collect();
        let second_ids: Vec<String> = second.iter().map(|r| r.tool_call_id.clone()).collect();
        assert_eq!(first_ids, second_ids);

        // Verify ascending order: each created_at <= next
        for window in first.windows(2) {
            assert!(window[0].created_at <= window[1].created_at);
        }
    }

    #[test]
    fn test_stale_approval_rejection() {
        let (_tmp, store) = make_store();
        let session_id = Uuid::new_v4();

        // Create a fresh approval
        let fresh_id = "tc-fresh".to_string();
        store
            .record_request(
                fresh_id.clone(),
                "tool_fresh".to_string(),
                serde_json::json!({}),
                None,
                Some(session_id),
                Some(Uuid::new_v4()),
            )
            .unwrap();

        // Create an old approval by manually writing a backdated record
        let old_id = "tc-old".to_string();
        let old_record = ApprovalRecord {
            version: 1,
            tool_call_id: old_id.clone(),
            tool_name: "tool_old".to_string(),
            scope: "tool_old".to_string(),
            arguments: serde_json::json!({}),
            explanation: None,
            state: ApprovalState::Pending,
            created_at: Utc::now() - chrono::Duration::hours(2),
            resolved_at: None,
            reason: None,
            session_id: Some(session_id),
            task_id: Some(Uuid::new_v4()),
            intent: None,
        };
        store.write_record(&old_record).unwrap();

        // Reject stale approvals older than 1 hour
        let rejected = store
            .reject_stale(session_id, chrono::Duration::hours(1))
            .unwrap();

        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0], old_id);

        // Verify the old one is now rejected
        let old = store.load(&old_id).unwrap().unwrap();
        assert_eq!(old.state, ApprovalState::Rejected);
        assert_eq!(old.reason.as_deref(), Some("Stale approval auto-rejected"));

        // Verify the fresh one is still pending
        let fresh = store.load(&fresh_id).unwrap().unwrap();
        assert_eq!(fresh.state, ApprovalState::Pending);
    }

    #[tokio::test]
    async fn test_approve_waits_for_record_request() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        let active = ActiveApprovalRegistry::new();
        let approvals = ApprovalHandle::new(root.clone(), active.clone());
        let store = approvals.store().clone();

        let session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let tool_call_id = format!("tc-{task_id}");

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        active.register(task_id, session_id, tx).await;

        let approvals_for_task = approvals.clone();
        let tool_call_id_for_task = tool_call_id.clone();
        let approve_task =
            tokio::spawn(async move { approvals_for_task.approve(tool_call_id_for_task).await });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        store
            .record_request(
                tool_call_id.clone(),
                "file_write".to_string(),
                serde_json::json!({}),
                Some("needs approval".to_string()),
                Some(session_id),
                Some(task_id),
            )
            .unwrap();

        approve_task.await.unwrap().unwrap();

        let response = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(response.approved);
        assert_eq!(response.tool_call_id, tool_call_id);
    }

    #[tokio::test]
    async fn test_wait_for_intent_resolves() {
        let (_tmp, store) = make_store();
        let session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let tool_call_id = "tc-intent-test".to_string();

        let store_clone = store.clone();
        let tc_id = tool_call_id.clone();

        let wait_task = tokio::spawn(async move {
            store_clone
                .wait_for_intent(&tc_id, std::time::Duration::from_secs(5))
                .await
        });

        // Small sleep to ensure waiter is waiting
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Record request
        store
            .record_request(
                tool_call_id.clone(),
                "test_tool".to_string(),
                serde_json::json!({}),
                None,
                Some(session_id),
                Some(task_id),
            )
            .unwrap();

        // Small sleep again, it should still be waiting since no intent
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Provide intent
        store
            .record_intent(tool_call_id.clone(), true, Some("approved".to_string()))
            .unwrap();

        let intent = wait_task
            .await
            .unwrap()
            .unwrap()
            .expect("Should have intent");
        assert!(intent.approved);
        assert_eq!(intent.reason.as_deref(), Some("approved"));
    }

    #[tokio::test]
    async fn test_wait_for_intent_detects_external_file_update() {
        let (_tmp, store) = make_store();
        let session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let tool_call_id = "tc-intent-filewatch-test".to_string();

        store
            .record_request(
                tool_call_id.clone(),
                "test_tool".to_string(),
                serde_json::json!({}),
                None,
                Some(session_id),
                Some(task_id),
            )
            .unwrap();

        let store_clone = store.clone();
        let tc_id = tool_call_id.clone();
        let wait_task = tokio::spawn(async move {
            store_clone
                .wait_for_intent(&tc_id, std::time::Duration::from_secs(5))
                .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut record = store.load_from_fs(&tool_call_id).unwrap().unwrap();
        record.intent = Some(ApprovalIntent {
            intent_id: Uuid::new_v4(),
            approved: true,
            reason: Some("external update".to_string()),
            created_at: Utc::now(),
        });
        std::fs::write(
            store.approval_path(&tool_call_id),
            serde_json::to_string_pretty(&record).unwrap(),
        )
        .unwrap();

        let intent = wait_task.await.unwrap().unwrap().unwrap();
        assert!(intent.approved);
        assert_eq!(intent.reason.as_deref(), Some("external update"));
    }

    #[tokio::test]
    async fn test_wait_for_intent_timeout() {
        let (_tmp, store) = make_store();
        let session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let tool_call_id = "tc-timeout-test".to_string();

        // Record request but no intent
        store
            .record_request(
                tool_call_id.clone(),
                "test_tool".to_string(),
                serde_json::json!({}),
                None,
                Some(session_id),
                Some(task_id),
            )
            .unwrap();

        let start = std::time::Instant::now();
        let res = store
            .wait_for_intent(&tool_call_id, std::time::Duration::from_millis(100))
            .await
            .unwrap();

        assert!(res.is_none());
        assert!(start.elapsed() >= std::time::Duration::from_millis(100));
    }
}
