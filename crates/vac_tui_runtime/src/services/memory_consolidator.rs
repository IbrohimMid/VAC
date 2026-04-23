//! F7.1 — TUI wire-up for `vac_memory`'s consolidator.
//!
//! The consolidator itself lives in `vac_memory` (see F4.3). This
//! module gives the TUI a small helper that:
//!
//! 1. Evaluates whether the gate opens (without actually writing).
//! 2. Optionally runs a cycle and returns the report.
//! 3. Pushes the report onto the banner queue via
//!    [`super::memory_banner::push_consolidation_banner`].
//!
//! Kept decoupled from AppState so drivers can wire it into whatever
//! trigger they want (session-close, /memory slash command, periodic
//! tick).

use vac_memory::{
    ConsolidationReport, Consolidator, ConsolidatorGate, MemoryResult, PolicySet,
    policy::ConsolidationInput,
};

use crate::app::types::BannerState;
use crate::services::memory_banner::push_consolidation_banner;

/// Cheap inspection — does the gate allow a run right now? Drivers
/// use this to light up a "memory ready" footer pip without paying
/// the cost of the actual run.
pub async fn gate_status(
    consolidator: &Consolidator,
    input: &ConsolidationInput,
) -> MemoryResult<ConsolidatorGate> {
    consolidator.evaluate_gate(input).await
}

/// Run one consolidation cycle and surface the report via the banner
/// queue. The returned `ConsolidationReport` is what the consolidator
/// produced — `skipped_reason` carries the gate rationale on a skip.
pub async fn run_and_banner(
    consolidator: &Consolidator,
    policies: &PolicySet,
    input: &ConsolidationInput,
    banner: &mut BannerState,
) -> MemoryResult<ConsolidationReport> {
    let report = consolidator.run_phases(policies, input).await?;
    push_consolidation_banner(banner, &report);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_memory::{ConsolidatorConfig, MemoryScanner, policy::builtin_policy_set};

    fn policy_set() -> PolicySet {
        builtin_policy_set()
    }

    fn always_runnable_cfg() -> ConsolidatorConfig {
        ConsolidatorConfig {
            min_session_count: 0,
            min_interval: std::time::Duration::ZERO,
            stale_lock_after: std::time::Duration::from_secs(60),
        }
    }

    #[tokio::test]
    async fn gate_status_reports_run_when_inputs_are_ample() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let c = Consolidator::new(scanner, always_runnable_cfg());
        let input = ConsolidationInput {
            raw_lines: vec!["learn: x".into()],
            session_count: 100,
        };
        let gate = gate_status(&c, &input).await.unwrap();
        assert_eq!(gate, ConsolidatorGate::Run);
    }

    #[tokio::test]
    async fn run_and_banner_emits_success_banner_on_write() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let c = Consolidator::new(scanner, always_runnable_cfg());
        let mut banner = BannerState::default();
        let report = run_and_banner(
            &c,
            &policy_set(),
            &ConsolidationInput {
                raw_lines: vec!["learn: nextest not cargo test".into()],
                session_count: 10,
            },
            &mut banner,
        )
        .await
        .unwrap();
        assert!(!report.was_skipped());
        assert!(banner.queue.current().is_some());
    }

    #[tokio::test]
    async fn run_and_banner_emits_info_banner_on_skip() {
        let tmp = tempfile::tempdir().unwrap();
        let scanner = MemoryScanner::new(tmp.path().to_path_buf());
        scanner.ensure_layout().await.unwrap();
        let mut cfg = always_runnable_cfg();
        cfg.min_session_count = 99;
        let c = Consolidator::new(scanner, cfg);
        let mut banner = BannerState::default();
        let report = run_and_banner(
            &c,
            &policy_set(),
            &ConsolidationInput {
                raw_lines: vec!["learn: x".into()],
                session_count: 1,
            },
            &mut banner,
        )
        .await
        .unwrap();
        assert!(report.was_skipped());
        assert!(banner.queue.current().is_some());
    }
}
