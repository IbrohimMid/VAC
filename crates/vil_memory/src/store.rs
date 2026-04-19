use crate::episodic::EpisodicMemory;
use crate::error::MemoryResult;
use crate::semantic::SemanticMemory;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

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
    // Arc (not Arc<RwLock>) because episodic/semantic use interior RwLock on
    // their redb Database; no outer lock needed.
    episodic: Arc<EpisodicMemory>,
    semantic: Arc<SemanticMemory>,
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
            .join("memory.db");
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
        let episodic = Arc::new(EpisodicMemory::new(&config.db_path).await?);
        let semantic = Arc::new(SemanticMemory::new(&config.db_path).await?);

        Ok(Self {
            working_memory,
            episodic,
            semantic,
            config,
        })
    }

    pub async fn store(&self, entry: MemoryEntry) -> MemoryResult<()> {
        match entry.memory_type {
            MemoryType::Working => {
                let mut working = self.working_memory.write().await;
                working.add(entry);
            }
            MemoryType::Episodic => {
                self.episodic.store_episode(entry).await?;
            }
            MemoryType::Semantic => {
                self.semantic.store_fact(entry).await?;
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
            Some(MemoryType::Episodic) => self.episodic.retrieve_episodes(query, 10).await,
            Some(MemoryType::Semantic) => self.semantic.retrieve_facts(query, 10).await,
            None => {
                let mut results = Vec::new();

                let working = self.working_memory.read().await;
                results.extend(working.search(query));

                results.extend(self.episodic.retrieve_episodes(query, 5).await?);
                results.extend(self.semantic.retrieve_facts(query, 5).await?);

                Ok(results)
            }
        }
    }

    /// Demote the least-important working memory entries to episodic storage
    /// and remove them from working memory.
    pub async fn consolidate(&self) -> MemoryResult<()> {
        let to_consolidate = {
            let working = self.working_memory.read().await;
            working.get_low_priority_entries(10)
        };

        let ids: Vec<String> = to_consolidate.iter().map(|e| e.id.clone()).collect();

        for entry in to_consolidate {
            self.episodic.store_episode(entry).await?;
        }

        // Remove consolidated entries from working memory.
        {
            let mut working = self.working_memory.write().await;
            working.entries.retain(|e| !ids.contains(&e.id));
        }

        Ok(())
    }
}

struct WorkingMemory {
    entries: Vec<MemoryEntry>,
    capacity: usize,
}

impl WorkingMemory {
    fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity,
        }
    }

    fn add(&mut self, entry: MemoryEntry) {
        if self.entries.len() >= self.capacity {
            // Evict the entry with the lowest importance to make room.
            // Sort ascending so index 0 = lowest importance, then remove it.
            self.entries
                .sort_by(|a, b| {
                    a.importance
                        .partial_cmp(&b.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    fn search(&self, query: &str) -> Vec<MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.content.to_lowercase().contains(&query.to_lowercase()))
            .cloned()
            .collect()
    }

    fn get_low_priority_entries(&self, count: usize) -> Vec<MemoryEntry> {
        let mut sorted = self.entries.clone();
        sorted.sort_by(|a, b| {
            a.importance
                .partial_cmp(&b.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.into_iter().take(count).collect()
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
