//! Phase B3 + D1 — idle-time maintenance ticks.
//!
//! Single module that hosts the handful of background jobs the TUI
//! runs when the operator is idle:
//!
//! - `prune_spill_dir` (B3): bound disk under `.vac/tool-results/`
//!   to a 24h retention.
//! - `auto_dream_tick` (D1): call `AutoDreamService::tick` on the
//!   configured cadence.
//! - `away_summary_probe` (D1): call `AwaySummaryService::on_resume`
//!   once at startup + emit the report if the gap exceeds threshold.
//!
//! Everything here is spawnable via `tokio::spawn`. Failures are
//! traced through the A1 bridge (`tracing::warn!` on subsystem
//! targets) so operators see them in the activity panel.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::Mutex;

use crate::app::AppState;

/// Interval between prune sweeps. 1 hour is gentle enough that the
/// maintenance cost is negligible and short enough to reclaim a
/// tool-result directory that suddenly grew.
pub const PRUNE_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Retention: delete spill files older than 24h. Matches the
/// default expiry assumption elsewhere in the runtime.
pub const PRUNE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Spawn the spill-prune loop. Takes ownership of the project_root
/// so the caller doesn't have to re-derive it per tick.
pub fn spawn_prune_spill_loop(project_root: PathBuf) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let root = project_root.join(".vac").join("tool-results");
        loop {
            match vac_tools::prune_spill_dir(&root, PRUNE_RETENTION).await {
                Ok(n) if n > 0 => {
                    tracing::info!(
                        target: "vac_tools::result_spill",
                        removed = n,
                        "idle-prune swept spill dir",
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        target: "vac_tools::result_spill",
                        error = %e,
                        "idle-prune failed",
                    );
                }
            }
            tokio::time::sleep(PRUNE_INTERVAL).await;
        }
    })
}

/// Interval for AutoDream tick polls. The service itself gates on
/// 5-min idle + activity delta, so a per-minute poll is cheap.
pub const AUTO_DREAM_POLL: Duration = Duration::from_secs(60);

/// Spawn the AutoDream tick loop.
pub fn spawn_auto_dream_loop(
    project_root: PathBuf,
    last_activity: Arc<Mutex<SystemTime>>,
) -> tokio::task::JoinHandle<()> {
    use crate::services::auto_dream::AutoDreamService;
    tokio::spawn(async move {
        let svc = AutoDreamService::new(project_root);
        loop {
            let activity_at = *last_activity.lock().await;
            match svc.tick(activity_at).await {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        target: "vac_tui_runtime::auto_dream",
                        error = %e,
                        "auto-dream tick failed",
                    );
                }
            }
            tokio::time::sleep(AUTO_DREAM_POLL).await;
        }
    })
}

/// Interval for PassiveFeedback polls. W5.2 latency budget is
/// 500 ms; a 2 s poll keeps the tick cost negligible while still
/// surfacing fresh diagnostics quickly.
pub const PASSIVE_FEEDBACK_POLL: Duration = Duration::from_secs(2);

/// G — spawn the PassiveFeedback tick loop. Updates the
/// `AppState.execution.lsp` snapshot on every tick so the `lsp`
/// SystemFacet has a fresh counter, and routes each new toast via
/// NotifyRouter. Failures emit `warn!` on
/// `vac_tui_runtime::passive_feedback` — allowlisted so the A1
/// bridge surfaces them with subsystem label `lsp`.
pub fn spawn_passive_feedback_loop(
    state: Arc<Mutex<AppState>>,
    driver: Arc<crate::services::passive_feedback::PassiveFeedbackDriver>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let toasts = driver.tick().await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            {
                let mut guard = state.lock().await;
                guard.execution.lsp.last_tick_unix = now;
                guard.execution.lsp.total_ticks =
                    guard.execution.lsp.total_ticks.saturating_add(1);
                guard.execution.lsp.recent_toasts = toasts.len();
            }
            if !toasts.is_empty() {
                // Surfacing individual diagnostics via the regular
                // toast lane is the PassiveFeedbackDriver's contract;
                // we don't double-dispatch here — the facet counter
                // is the pulse-level hook.
                tracing::info!(
                    target: "vac_tui_runtime::passive_feedback",
                    count = toasts.len(),
                    "passive-feedback tick surfaced new diagnostics",
                );
            }
            tokio::time::sleep(PASSIVE_FEEDBACK_POLL).await;
        }
    })
}

/// Interval for PolicyTracker snapshot refreshes. 5 s is long
/// enough to be negligible cost, short enough to keep the `policy`
/// facet's counters visibly responsive when the operator is
/// approaching a cap.
pub const POLICY_SNAPSHOT_POLL: Duration = Duration::from_secs(5);

/// C1 — spawn a periodic `PolicyTracker::snapshot` → AppState.policy
/// refresher. Without this producer the `policy` SystemFacet reads
/// `None` forever even when the engine is threading deny decisions
/// through `PolicyTracker::check` — the tracker counts independently
/// of what the pulse sees. This loop closes that gap.
pub fn spawn_policy_snapshot_loop(
    state: Arc<Mutex<AppState>>,
    tracker: Arc<vac_core::policy_limits::PolicyTracker>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let snap = tracker.snapshot().await;
            {
                let mut guard = state.lock().await;
                guard.execution.policy = Some(snap);
            }
            tokio::time::sleep(POLICY_SNAPSHOT_POLL).await;
        }
    })
}

/// One-shot on-startup probe of `AwaySummaryService`. Called from
/// the interactive bootstrap. If the gap is ≥ 1h, pushes a
/// `NotifyEvent::info` so the operator sees a welcome-back summary
/// via the activity panel — no banner, no modal.
pub async fn away_summary_probe(state: Arc<Mutex<AppState>>, project_root: PathBuf) {
    use crate::services::away_summary::{AwaySummaryService, ResumeOutcome};
    use crate::services::notify_router::{route, NotifyEvent};
    let svc = AwaySummaryService::new(project_root);
    match svc.on_resume().await {
        Ok(ResumeOutcome::Summary(report)) => {
            let mut guard = state.lock().await;
            route(
                &mut guard,
                NotifyEvent::info("resume", report.one_liner.clone())
                    .with_detail(report.last_transcript_snippet),
            );
        }
        Ok(ResumeOutcome::NoSummary { .. }) => {}
        Err(e) => {
            tracing::warn!(
                target: "vac_tui_runtime::away_summary",
                error = %e,
                "away-summary probe failed",
            );
        }
    }
    let _ = svc.touch().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_sane() {
        assert!(PRUNE_INTERVAL <= PRUNE_RETENTION);
        assert!(AUTO_DREAM_POLL <= Duration::from_secs(300));
        assert!(PASSIVE_FEEDBACK_POLL <= Duration::from_secs(10));
        assert!(POLICY_SNAPSHOT_POLL <= Duration::from_secs(30));
    }

    #[tokio::test]
    async fn passive_feedback_loop_updates_lsp_snapshot() {
        use crate::services::passive_feedback::PassiveFeedbackDriver;
        use vac_tools::rust_analysis::LspDiagnosticRegistry;

        let state = Arc::new(Mutex::new(AppState::default()));
        let driver = Arc::new(PassiveFeedbackDriver::new(
            Arc::new(LspDiagnosticRegistry::new()),
        ));
        let handle = spawn_passive_feedback_loop(state.clone(), driver);
        // Give the loop one tick to run; the poll period is 2s,
        // but the first body runs before the first sleep.
        tokio::time::sleep(Duration::from_millis(100)).await;
        {
            let guard = state.lock().await;
            assert!(
                guard.execution.lsp.total_ticks >= 1,
                "expected ≥ 1 tick, got {}",
                guard.execution.lsp.total_ticks,
            );
            assert!(guard.execution.lsp.last_tick_unix > 0);
        }
        handle.abort();
    }

    #[tokio::test]
    async fn policy_snapshot_loop_populates_execution_policy() {
        use std::sync::Arc as SArc;
        use vac_core::policy_limits::{PolicyLimits, PolicyTracker};

        let state = Arc::new(Mutex::new(AppState::default()));
        let tracker = SArc::new(PolicyTracker::new(PolicyLimits {
            max_submits_per_hour: Some(3),
            max_tokens_per_session: Some(10_000),
            ..Default::default()
        }));
        let handle = spawn_policy_snapshot_loop(state.clone(), tracker.clone());
        tokio::time::sleep(Duration::from_millis(100)).await;
        {
            let guard = state.lock().await;
            let snap = guard
                .execution
                .policy
                .as_ref()
                .expect("policy snapshot populated");
            assert_eq!(snap.policy.max_submits_per_hour, Some(3));
            assert_eq!(snap.policy.max_tokens_per_session, Some(10_000));
        }
        handle.abort();
    }

    #[tokio::test]
    async fn away_summary_probe_on_empty_project_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(AppState::default()));
        away_summary_probe(state.clone(), tmp.path().to_path_buf()).await;
        // No summary expected on empty project; activity stays empty
        // and touch() has persisted last-seen.json so next probe has
        // a baseline.
        let guard = state.lock().await;
        assert!(guard.execution.activity.is_empty());
    }

    #[tokio::test]
    async fn away_summary_probe_with_gap_posts_info() {
        let tmp = tempfile::tempdir().unwrap();
        // Seed a last-seen timestamp > 1 hour in the past.
        let vac_dir = tmp.path().join(".vac");
        tokio::fs::create_dir_all(&vac_dir).await.unwrap();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let old = now - 7_200; // 2 hours
        let content = serde_json::json!({ "unix_secs": old });
        tokio::fs::write(vac_dir.join("last-seen.json"), content.to_string())
            .await
            .unwrap();
        let state = Arc::new(Mutex::new(AppState::default()));
        away_summary_probe(state.clone(), tmp.path().to_path_buf()).await;
        let guard = state.lock().await;
        assert_eq!(guard.execution.activity.len(), 1);
        assert!(
            guard.execution.activity[0].message.starts_with("[resume]")
        );
    }
}
