use crate::error::MemoryResult;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use vac_memory::{
    MemoryScanner,
    memdir::{Memory, MemoryFrontmatter, MemoryKind},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub memory_type: MemoryType,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub importance: f32,
    pub access_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    Working,
    Episodic,
    Semantic,
}

pub struct MemoryStore {
    working_memory: Arc<RwLock<WorkingMemory>>,
    scanner: Arc<MemoryScanner>,
    #[allow(dead_code)]
    config: MemoryConfig,
}

#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub working_capacity: usize,
    pub db_path: PathBuf,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        let db_path = dirs_next::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("vac")
            .join("memory");
        Self {
            working_capacity: 100,
            db_path,
        }
    }
}

impl MemoryStore {
    pub async fn new(config: MemoryConfig) -> MemoryResult<Self> {
        info!("Initializing memory store with config: {:?}", config);

        let working_memory = Arc::new(RwLock::new(WorkingMemory::new(config.working_capacity)));
        let scanner = Arc::new(MemoryScanner::new(config.db_path.clone()));
        scanner
            .ensure_layout()
            .await
            .map_err(|e| crate::error::MemoryError::Storage(e.to_string()))?;

        Ok(Self {
            working_memory,
            scanner,
            config,
        })
    }

    pub async fn store(&self, entry: MemoryEntry) -> MemoryResult<()> {
        match entry.memory_type {
            MemoryType::Working => {
                let mut working = self.working_memory.write().await;
                working.add(entry);
            }
            MemoryType::Episodic | MemoryType::Semantic => {
                let kind = if entry.memory_type == MemoryType::Episodic {
                    MemoryKind::Active
                } else {
                    MemoryKind::Team
                };
                let mem = Memory {
                    kind,
                    path: PathBuf::new(),
                    frontmatter: MemoryFrontmatter {
                        topic: format!("vil-{}", entry.id),
                        title: None,
                        tags: vec![],
                        created_at: entry.timestamp,
                        updated_at: None,
                        importance: entry.importance,
                        source_policy: Some("vil_memory_bridge".to_string()),
                    },
                    body: entry.content,
                };
                self.scanner
                    .write(mem)
                    .await
                    .map_err(|e| crate::error::MemoryError::Storage(e.to_string()))?;
            }
        }
        Ok(())
    }

    pub async fn retrieve(
        &self,
        query: &str,
        memory_type: Option<MemoryType>,
    ) -> MemoryResult<Vec<MemoryEntry>> {
        match memory_type {
            Some(MemoryType::Working) => {
                let working = self.working_memory.read().await;
                Ok(working.search(query))
            }
            Some(MemoryType::Episodic) | Some(MemoryType::Semantic) => {
                self.search_vac_memory(query, memory_type).await
            }
            None => {
                let working = {
                    let w = self.working_memory.read().await;
                    w.search(query)
                };
                let mut results = working;
                results.extend(self.search_vac_memory(query, None).await?);
                results.sort_by(|a, b| {
                    b.importance
                        .partial_cmp(&a.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                results.truncate(10);
                Ok(results)
            }
        }
    }

    async fn search_vac_memory(
        &self,
        query: &str,
        mt: Option<MemoryType>,
    ) -> MemoryResult<Vec<MemoryEntry>> {
        let mems = self.scanner.scan_all().await.unwrap_or_default();
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        for m in mems {
            let matches_type = match mt {
                Some(MemoryType::Episodic) => m.kind == MemoryKind::Active,
                Some(MemoryType::Semantic) => m.kind == MemoryKind::Team,
                Some(MemoryType::Working) => false,
                None => true,
            };
            if !matches_type {
                continue;
            }
            if m.body.to_lowercase().contains(&query_lower)
                || m.frontmatter.topic.to_lowercase().contains(&query_lower)
            {
                results.push(MemoryEntry {
                    id: m.frontmatter.topic,
                    content: m.body,
                    memory_type: if m.kind == MemoryKind::Team {
                        MemoryType::Semantic
                    } else {
                        MemoryType::Episodic
                    },
                    timestamp: m.frontmatter.created_at,
                    importance: m.frontmatter.importance,
                    access_count: 0,
                });
            }
        }
        results.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(10);
        Ok(results)
    }

    pub async fn update_importance(&self, id: &str, importance: f32) -> MemoryResult<()> {
        // Find it in working memory
        {
            let mut working = self.working_memory.write().await;
            if let Some(entry) = working.entries.iter_mut().find(|e| e.id == id) {
                entry.importance = importance;
                return Ok(());
            }
        }
        // In vac_memory, we'd need to load, mutate frontmatter, and save.
        // For simplicity, we just do a linear scan and rewrite.
        let mems = self.scanner.scan_all().await.unwrap_or_default();
        for mut m in mems {
            if m.frontmatter.topic == id {
                m.frontmatter.importance = importance;
                self.scanner
                    .write(m)
                    .await
                    .map_err(|e| crate::error::MemoryError::Storage(e.to_string()))?;
                return Ok(());
            }
        }
        Ok(())
    }

    pub async fn record_access(&self, id: &str) -> MemoryResult<()> {
        // Just increment in working memory
        {
            let mut working = self.working_memory.write().await;
            if let Some(entry) = working.entries.iter_mut().find(|e| e.id == id) {
                entry.access_count += 1;
            }
        }
        Ok(())
    }

    pub async fn clear(&self) -> MemoryResult<()> {
        {
            let mut working = self.working_memory.write().await;
            working.entries.clear();
        }
        // vac_memory cannot be cleared this easily safely, but we can delete the directory.
        let _ = tokio::fs::remove_dir_all(&self.config.db_path).await;
        self.scanner
            .ensure_layout()
            .await
            .map_err(|e| crate::error::MemoryError::Storage(e.to_string()))?;
        Ok(())
    }
}

pub struct WorkingMemory {
    entries: std::collections::VecDeque<MemoryEntry>,
    capacity: usize,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn add(&mut self, entry: MemoryEntry) {
        if self.entries.len() >= self.capacity {
            // Find lowest importance or oldest
            let mut lowest_idx = 0;
            let mut lowest_score = f32::MAX;

            for (i, e) in self.entries.iter().enumerate() {
                let score = e.importance + (e.access_count as f32 * 0.1);
                if score < lowest_score {
                    lowest_score = score;
                    lowest_idx = i;
                }
            }
            self.entries.remove(lowest_idx);
        }
        self.entries.push_back(entry);
    }

    pub fn search(&self, query: &str) -> Vec<MemoryEntry> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.content.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    pub fn update_importance(&mut self, id: &str, importance: f32) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.importance = importance;
            true
        } else {
            false
        }
    }

    pub fn record_access(&mut self, id: &str) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.access_count += 1;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn make_entry(id: &str, importance: f32) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            content: format!("content of {id}"),
            memory_type: MemoryType::Working,
            timestamp: chrono::Utc::now(),
            importance,
            access_count: 0,
        }
    }

    #[test]
    fn add_over_capacity_evicts_lowest_importance() {
        let mut wm = WorkingMemory::new(3);
        wm.add(make_entry("low", 0.1));
        wm.add(make_entry("high", 0.9));
        wm.add(make_entry("mid", 0.5));
        // capacity reached; next add should evict "low" (0.1)
        wm.add(make_entry("new", 0.4));

        assert_eq!(wm.entries.len(), 3);
        assert!(
            !wm.entries.iter().any(|e| e.id == "low"),
            "lowest importance entry should have been evicted"
        );
        assert!(wm.entries.iter().any(|e| e.id == "high"));
        assert!(wm.entries.iter().any(|e| e.id == "new"));
    }

    #[test]
    fn add_handles_nan_importance_without_panic() {
        let mut wm = WorkingMemory::new(2);
        let mut e1 = make_entry("nan1", 0.5);
        e1.importance = f32::NAN;
        let mut e2 = make_entry("nan2", 0.5);
        e2.importance = f32::NAN;
        let e3 = make_entry("normal", 0.8);
        wm.add(e1);
        wm.add(e2);
        // This must not panic despite NaN comparisons.
        wm.add(e3);
        assert_eq!(wm.entries.len(), 2);
    }
}
