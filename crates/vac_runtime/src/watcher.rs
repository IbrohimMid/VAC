//! File watcher — triggers jobs on file system changes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};
use crate::jobs::{Job, JobKind, JobTrigger};
use crate::queue::TaskQueue;

fn default_debounce_ms() -> u64 {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchEntry {
    pub path: String,
    pub task: String,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

pub struct FileWatcher {
    entries: Vec<WatchEntry>,
    queue: Arc<TaskQueue>,
}

impl FileWatcher {
    pub fn new(entries: Vec<WatchEntry>, queue: Arc<TaskQueue>) -> Self {
        Self { entries, queue }
    }

    pub fn start(&self) -> Result<()> {
        if self.entries.is_empty() {
            return Ok(());
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<(PathBuf, String, u64)>();
        let mut path_map: HashMap<PathBuf, (String, u64)> = HashMap::new();
        for e in &self.entries {
            path_map.insert(PathBuf::from(&e.path), (e.task.clone(), e.debounce_ms));
        }
        let tx2 = tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    for path in &event.paths {
                        for (wp, (task, debounce)) in &path_map {
                            if path.starts_with(wp) || path == wp {
                                let _ = tx2.send((path.clone(), task.clone(), *debounce));
                                break;
                            }
                        }
                    }
                }
            }
        })?;
        for e in &self.entries {
            let p = PathBuf::from(&e.path);
            if p.exists() {
                watcher.watch(&p, RecursiveMode::Recursive)?;
            } else {
                warn!(path = %e.path, "Watch path does not exist");
            }
        }
        let queue = self.queue.clone();
        tokio::spawn(async move {
            let _watcher = watcher;
            let mut last_fire: HashMap<String, Instant> = HashMap::new();
            while let Some((path, task, debounce_ms)) = rx.recv().await {
                let now = Instant::now();
                let key = format!("{}:{}", path.display(), task);
                let debounce = Duration::from_millis(debounce_ms);
                if last_fire.get(&key).map_or(true, |t| now.duration_since(*t) >= debounce) {
                    last_fire.insert(key, now);
                    let job = Job::new(JobKind::RunTask { description: task.clone() })
                        .with_trigger(JobTrigger::FileWatch(path.display().to_string()));
                    info!(path = %path.display(), task = %task, "File-watch fired");
                    queue.enqueue(job).await;
                }
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn watch_fires_on_write() {
        let dir = tempdir().unwrap();
        let entries = vec![WatchEntry {
            path: dir.path().to_str().unwrap().to_string(),
            task: "t".to_string(),
            debounce_ms: 50,
        }];
        let queue = Arc::new(TaskQueue::new());
        FileWatcher::new(entries, queue.clone()).start().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::fs::write(dir.path().join("test.txt"), "hello").unwrap();
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if !queue.is_empty().await {
                break;
            }
        }
        assert!(!queue.is_empty().await);
    }
}