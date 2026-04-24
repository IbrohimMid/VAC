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
