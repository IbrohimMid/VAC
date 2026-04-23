//! Cron scheduler — schedules jobs based on cron expressions.

use crate::jobs::{Job, JobKind, JobTrigger};
use crate::queue::TaskQueue;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    pub expression: String,
    pub task: String,
}

impl From<&vac_core::config::ScheduleEntry> for CronEntry {
    fn from(s: &vac_core::config::ScheduleEntry) -> Self {
        Self {
            expression: s.cron.clone(),
            task: s.task.clone(),
        }
    }
}

/// Convert `AutopilotConfig.schedules` into runtime `CronEntry`s,
/// dropping entries marked `disabled`. Callers feed the result into
/// `CronScheduler::new`.
pub fn entries_from_autopilot(
    project_root: &std::path::Path,
    cfg: &vac_core::config::AutopilotConfig,
) -> Vec<CronEntry> {
    let mut entries: Vec<CronEntry> = cfg
        .schedules
        .iter()
        .filter(|e| !e.disabled)
        .map(CronEntry::from)
        .collect();

    let schedules_file = project_root.join(".vac/autopilot.schedules.toml");
    if let Ok(content) = std::fs::read_to_string(&schedules_file) {
        #[derive(Deserialize)]
        struct ScheduleDoc {
            #[serde(default)]
            schedules: Vec<vac_core::config::ScheduleEntry>,
        }
        if let Ok(doc) = toml::from_str::<ScheduleDoc>(&content) {
            for entry in doc.schedules {
                if !entry.disabled {
                    entries.push(CronEntry::from(&entry));
                }
            }
        }
    }
    entries
}

#[cfg(test)]
mod bridge_tests {
    use super::*;
    use vac_core::config::{AutopilotConfig, ScheduleEntry};

    fn entry(id: &str, disabled: bool) -> ScheduleEntry {
        ScheduleEntry {
            id: id.into(),
            cron: "@hourly".into(),
            task: format!("task-{id}"),
            profile: None,
            disabled,
        }
    }

    #[test]
    fn from_schedule_entry_carries_cron_and_task() {
        let s = entry("nightly", false);
        let c = CronEntry::from(&s);
        assert_eq!(c.expression, "@hourly");
        assert_eq!(c.task, "task-nightly");
    }

    #[test]
    fn entries_from_autopilot_skips_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = AutopilotConfig {
            schedules: vec![entry("a", false), entry("b", true), entry("c", false)],
            ..Default::default()
        };
        let entries = entries_from_autopilot(tmp.path(), &cfg);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.task == "task-a"));
        assert!(entries.iter().any(|e| e.task == "task-c"));
    }
}

pub struct CronScheduler {
    entries: Vec<CronEntry>,
    queue: Arc<TaskQueue>,
}

impl CronScheduler {
    pub fn new(entries: Vec<CronEntry>, queue: Arc<TaskQueue>) -> Self {
        Self { entries, queue }
    }

    pub fn start(&self) {
        for entry in &self.entries {
            let expr = entry.expression.clone();
            let task = entry.task.clone();
            let queue = self.queue.clone();
            tokio::spawn(async move {
                let schedule = match Schedule::from_str(&expr) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(expr = %expr, error = %e, "Invalid cron expression");
                        return;
                    }
                };
                loop {
                    let now = chrono::Utc::now();
                    let next = match schedule.upcoming(chrono::Utc).next() {
                        Some(t) => t,
                        None => break,
                    };
                    let delay = (next - now).to_std().unwrap_or_default();
                    tokio::time::sleep(delay).await;
                    let job = Job::new(JobKind::RunTask {
                        description: task.clone(),
                    })
                    .with_trigger(JobTrigger::Cron(expr.clone()));
                    info!(task = %task, "Cron job fired");
                    queue.enqueue(job).await;
                }
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cron_fires_quickly() {
        let entries = vec![CronEntry {
            expression: "* * * * * *".to_string(),
            task: "t".to_string(),
        }];
        let queue = Arc::new(TaskQueue::new());
        CronScheduler::new(entries, queue.clone()).start();
        for _ in 0..15 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if !queue.is_empty().await {
                break;
            }
        }
        assert!(!queue.is_empty().await);
    }
}
