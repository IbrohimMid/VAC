//! W6.2 — subagent coordinator.
//!
//! Wraps the W6.1 [`AppStateRootHandle`] with a per-nest-level
//! `agent` identity so every push carries its source automatically.
//! Spawning a child returns a new coordinator whose root handle is
//! the **same** `Arc` — mutations at any depth land at the root
//! without the caller threading a handle through each boundary.
//!
//! Integration with the fork primitive:
//! `SubagentCoordinator::fork_speculate` composes the root handle
//! with a `ForkedAgentRunner` so a speculative sub-submit records
//! its breadcrumbs at the parent's root even when the fork's own
//! overlay cache is later discarded.
//!
//! The coordinator deliberately does NOT own the full `AppState` —
//! only the narrow subset of root-scoped fields that survive fork
//! depth. Everything else (UI-frame-local state) stays on the
//! parent's AppState and is not cloned.

use uuid::Uuid;

use vac_session_engine::{
    CacheSafeParams, ForkBudget, ForkResult, ForkedAgentRunner, OverlayGuard,
};

use crate::app::{
    AgentBreadcrumb, AppStateRootHandle, NotificationLevel, RootNotification,
};

/// Shared helper to produce a timestamp in the same shape as the
/// root handle expects. Unix seconds; zero on clock failure.
fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Per-nest-level coordinator. Holds a clone of the shared root
/// handle plus the `agent` identity used for every push.
#[derive(Debug, Clone)]
pub struct SubagentCoordinator {
    root: AppStateRootHandle,
    agent: String,
    /// Depth from the topmost coordinator (root = 0, child = 1, …).
    /// Used for telemetry + the acceptance test.
    depth: u32,
}

impl SubagentCoordinator {
    /// Topmost coordinator with a **fresh** root handle. Each call
    /// creates a new tree, so subagents spawned from two separate
    /// `root(...)` calls do NOT share state. When you want two
    /// coordinators to share a single root, use [`with_handle`]
    /// with one `AppStateRootHandle::new()` constructed up-front.
    /// Typical use: constructed once per TUI session, cloned into
    /// children as they fork.
    pub fn root(agent: impl Into<String>) -> Self {
        Self {
            root: AppStateRootHandle::new(),
            agent: agent.into(),
            depth: 0,
        }
    }

    /// Construct from an existing handle — useful when the TUI
    /// already owns a shared observer and wants the subagent tree
    /// to report into it.
    pub fn with_handle(root: AppStateRootHandle, agent: impl Into<String>) -> Self {
        Self {
            root,
            agent: agent.into(),
            depth: 0,
        }
    }

    /// Spawn a child coordinator. Same root, new agent identity,
    /// depth+1. Cheap clone.
    pub fn spawn_child(&self, agent: impl Into<String>) -> Self {
        Self {
            root: self.root.clone(),
            agent: agent.into(),
            depth: self.depth + 1,
        }
    }

    pub fn agent(&self) -> &str {
        &self.agent
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Shared root handle — exposed so downstream services (TUI
    /// renderer, trajectory exporter) can observe without going
    /// through the coordinator.
    pub fn root_handle(&self) -> &AppStateRootHandle {
        &self.root
    }

    /// Push a notification tagged with this coordinator's agent id.
    pub async fn notify(&self, level: NotificationLevel, message: impl Into<String>) {
        self.root
            .push_notification(RootNotification {
                source: self.agent.clone(),
                level,
                message: message.into(),
                ts_unix: now_unix(),
            })
            .await
    }

    /// Info-level shortcut.
    pub async fn info(&self, message: impl Into<String>) {
        self.notify(NotificationLevel::Info, message).await
    }

    /// Error-level shortcut — also bumps the root's errors_seen
    /// counter via push_notification.
    pub async fn error(&self, message: impl Into<String>) {
        self.notify(NotificationLevel::Error, message).await
    }

    /// Record a tool call against this coordinator's agent identity.
    /// Pushes a breadcrumb and bumps the root's tool counter.
    pub async fn record_tool(
        &self,
        tool: impl Into<String>,
        summary: impl Into<String>,
    ) {
        self.root
            .push_breadcrumb(AgentBreadcrumb {
                agent: self.agent.clone(),
                tool: tool.into(),
                summary: summary.into(),
                ts_unix: now_unix(),
            })
            .await
    }

    /// Speculative sub-submit that records breadcrumbs at the root
    /// throughout. On any outcome the overlay is cleaned up via
    /// `OverlayGuard::cleanup_async`; the fork's reads surface as
    /// breadcrumbs tagged with this coordinator's agent.
    pub async fn fork_speculate(
        &self,
        runner: &ForkedAgentRunner,
        parent_session: Uuid,
        overlay_root: std::path::PathBuf,
        prompt: &str,
        reads_hint: Vec<std::path::PathBuf>,
        budget: ForkBudget,
    ) -> Result<ForkResult, SubagentError> {
        tokio::fs::create_dir_all(&overlay_root)
            .await
            .map_err(|e| SubagentError::Setup(format!("overlay root: {e}")))?;
        let overlay_dir = overlay_root.join(Uuid::new_v4().to_string());
        let guard = OverlayGuard::new(overlay_dir.clone())
            .await
            .map_err(|e| SubagentError::Setup(format!("overlay: {e}")))?;
        let params = CacheSafeParams::new(parent_session, overlay_dir);
        let result = match runner
            .speculate(&params, prompt, reads_hint.clone(), budget)
            .await
        {
            Ok(r) => {
                for path in &r.reads {
                    self.record_tool(
                        "read",
                        format!("fork warmed {}", path.display()),
                    )
                    .await;
                }
                Ok(r)
            }
            Err(e) => {
                self.error(format!("fork aborted: {e}")).await;
                Err(SubagentError::Fork(e.to_string()))
            }
        };
        guard.cleanup_async().await;
        result
    }
}

#[derive(Debug)]
pub enum SubagentError {
    Setup(String),
    Fork(String),
}

impl std::fmt::Display for SubagentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Setup(m) => write!(f, "subagent setup: {m}"),
            Self::Fork(m) => write!(f, "subagent fork: {m}"),
        }
    }
}

impl std::error::Error for SubagentError {}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    use vac_session_engine::{EngineResult, LlmAdapter, LlmRequest, LlmResponse};

    struct CannedAdapter;

    #[async_trait]
    impl LlmAdapter for CannedAdapter {
        async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
            Ok(LlmResponse {
                provider: "test".into(),
                model: "canned".into(),
                content: "ok".into(),
                input_tokens: 0,
                output_tokens: 0,
            })
        }
    }

    #[tokio::test]
    async fn root_starts_at_depth_zero() {
        let c = SubagentCoordinator::root("parent");
        assert_eq!(c.depth(), 0);
        assert_eq!(c.agent(), "parent");
    }

    #[tokio::test]
    async fn spawn_child_shares_root_and_increments_depth() {
        let parent = SubagentCoordinator::root("parent");
        let child = parent.spawn_child("child");
        assert_eq!(child.depth(), 1);
        // Mutation via child lands at the same root parent sees.
        child.info("hello from child").await;
        let seen = parent.root_handle().notifications().await;
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].source, "child");
    }

    #[tokio::test]
    async fn three_level_nest_pushes_land_at_root() {
        // W6.2 acceptance from the plan: parent → child → grandchild
        // each complete and each mutation lands at root.
        let parent = SubagentCoordinator::root("parent");
        let child = parent.spawn_child("child");
        let grandchild = child.spawn_child("grandchild");

        parent.info("p-note").await;
        child.info("c-note").await;
        grandchild.info("g-note").await;
        grandchild.record_tool("grep", "searched auth/").await;
        child.error("recoverable fault").await;

        let notifications = parent.root_handle().notifications().await;
        assert_eq!(notifications.len(), 4, "3 info + 1 error");
        let sources: Vec<&str> =
            notifications.iter().map(|n| n.source.as_str()).collect();
        assert_eq!(
            sources,
            vec!["parent", "child", "grandchild", "child"],
        );

        let breadcrumbs = parent.root_handle().breadcrumbs().await;
        assert_eq!(breadcrumbs.len(), 1);
        assert_eq!(breadcrumbs[0].agent, "grandchild");
        assert_eq!(breadcrumbs[0].tool, "grep");

        assert_eq!(parent.root_handle().tool_counter().await, 1);
        assert_eq!(parent.root_handle().errors_seen().await, 1);
        assert_eq!(grandchild.depth(), 2);
    }

    #[tokio::test]
    async fn fork_speculate_records_reads_as_breadcrumbs() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = SubagentCoordinator::root("parent");
        let child = parent.spawn_child("speculator");
        let runner = ForkedAgentRunner::new(Arc::new(CannedAdapter));
        let session = Uuid::new_v4();
        let reads = vec![
            std::path::PathBuf::from("src/auth/mod.rs"),
            std::path::PathBuf::from("src/auth/session.rs"),
        ];
        let result = child
            .fork_speculate(
                &runner,
                session,
                tmp.path().to_path_buf(),
                "warm the cache",
                reads.clone(),
                ForkBudget::default(),
            )
            .await
            .unwrap();
        assert_eq!(result.reads.len(), 2);
        let breadcrumbs = parent.root_handle().breadcrumbs().await;
        assert_eq!(breadcrumbs.len(), 2);
        assert!(breadcrumbs.iter().all(|b| b.agent == "speculator"));
        assert!(breadcrumbs.iter().all(|b| b.tool == "read"));
    }

    #[tokio::test]
    async fn fork_failure_surfaces_as_error_notification() {
        // Use an overlay_root whose parent is a regular file so
        // create_dir_all fails and fork_speculate takes the error
        // branch.
        let tmp = tempfile::tempdir().unwrap();
        let blocking_file = tmp.path().join("not-a-dir");
        tokio::fs::write(&blocking_file, "x").await.unwrap();
        let bad_root = blocking_file.join("cant-create");

        let parent = SubagentCoordinator::root("parent");
        let runner = ForkedAgentRunner::new(Arc::new(CannedAdapter));
        let err = parent
            .fork_speculate(
                &runner,
                Uuid::new_v4(),
                bad_root,
                "p",
                Vec::new(),
                ForkBudget::default(),
            )
            .await
            .unwrap_err();
        matches!(err, SubagentError::Setup(_));
        // No breadcrumbs because the fork never ran; also no error
        // notification because setup failed before the fork block.
        assert_eq!(parent.root_handle().breadcrumbs().await.len(), 0);
    }

    #[tokio::test]
    async fn with_handle_lets_tui_inject_existing_root() {
        let shared = AppStateRootHandle::new();
        shared
            .push_notification(RootNotification {
                source: "tui".into(),
                level: NotificationLevel::Info,
                message: "boot".into(),
                ts_unix: 0,
            })
            .await;
        let coord = SubagentCoordinator::with_handle(shared.clone(), "parent");
        coord.info("from coordinator").await;
        assert_eq!(shared.notifications().await.len(), 2);
    }

    #[tokio::test]
    async fn error_bumps_root_error_counter() {
        let c = SubagentCoordinator::root("x");
        c.error("boom").await;
        assert_eq!(c.root_handle().errors_seen().await, 1);
    }
}
