//! F4.6 — Bridge from `vac_memory::ConsolidationReport` to the TUI's
//! top-strip banner. Drivers call [`push_consolidation_banner`] once a
//! consolidator cycle finishes; the operator sees a one-line summary.

use vac_memory::ConsolidationReport;

use crate::app::types::BannerState;
use crate::services::banner::{BannerMessage, BannerStyle};

/// Translate a `ConsolidationReport` into a `BannerMessage` and push
/// it onto the banner queue. Skipped runs render as `Info`; successful
/// runs with ≥1 written file render as `Success`; a zero-writes run
/// that *wasn't* skipped is still `Info` (nothing to celebrate).
pub fn push_consolidation_banner(banner: &mut BannerState, report: &ConsolidationReport) {
    let style = if report.was_skipped() {
        BannerStyle::Info
    } else if report.files_written.is_empty() {
        BannerStyle::Info
    } else {
        BannerStyle::Success
    };
    let msg = BannerMessage::new(report.banner_line(), style);
    banner.queue.push(msg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_memory::report::WrittenFile;
    use vac_memory::{ConsolidationReport, MemoryKind};

    fn rep_with_files(n: usize) -> ConsolidationReport {
        let now = chrono::Utc::now();
        ConsolidationReport {
            started_at: now,
            finished_at: now,
            policies_fired: vec!["workflow_learnings".into()],
            files_written: (0..n)
                .map(|i| WrittenFile {
                    policy: "workflow_learnings".into(),
                    topic: format!("t{i}"),
                    kind: MemoryKind::Active,
                    path: std::path::PathBuf::from(format!("{i}.md")),
                })
                .collect(),
            skipped_reason: None,
        }
    }

    #[test]
    fn success_run_becomes_success_banner() {
        let mut state = BannerState::default();
        push_consolidation_banner(&mut state, &rep_with_files(2));
        let ev = state.queue.current().cloned();
        let msg = ev.expect("banner enqueued");
        assert!(matches!(msg.style, BannerStyle::Success));
        assert!(msg.text.contains("consolidated"));
    }

    #[test]
    fn skipped_run_becomes_info_banner() {
        let mut state = BannerState::default();
        let mut r = rep_with_files(0);
        r.skipped_reason = Some("cooldown not elapsed".into());
        push_consolidation_banner(&mut state, &r);
        let ev = state.queue.current().cloned().unwrap();
        assert!(matches!(ev.style, BannerStyle::Info));
        assert!(ev.text.contains("skipped"));
    }
}
