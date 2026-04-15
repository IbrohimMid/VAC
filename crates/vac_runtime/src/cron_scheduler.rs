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
