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
    episodic: Arc<RwLock<EpisodicMemory>>,
    semantic: Arc<RwLock<SemanticMemory>>,
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
        Self {
            working_capacity: 100,
            db_path: PathBuf::from("/tmp/vil_memory.db"),
        }
    }
}

impl MemoryStore {
    pub async fn new(config: MemoryConfig) -> MemoryResult<Self> {
        info!("Initializing memory store with config: {:?}", config);

        let working_memory = Arc::new(RwLock::new(WorkingMemory::new(config.working_capacity)));
        let episodic = Arc::new(RwLock::new(EpisodicMemory::new(&config.db_path).await?));
        let semantic = Arc::new(RwLock::new(SemanticMemory::new(&config.db_path).await?));

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
                let mut episodic = self.episodic.write().await;
                episodic.store_episode(entry).await?;
            }
            MemoryType::Semantic => {
                let mut semantic = self.semantic.write().await;
                semantic.store_fact(entry).await?;
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
            Some(MemoryType::Episodic) => {
                let episodic = self.episodic.read().await;
                episodic.retrieve_episodes(query, 10).await
            }
            Some(MemoryType::Semantic) => {
                let semantic = self.semantic.read().await;
                semantic.retrieve_facts(query, 10).await
            }
            None => {
                let mut results = Vec::new();

                let working = self.working_memory.read().await;
                results.extend(working.search(query));

                let episodic = self.episodic.read().await;
                results.extend(episodic.retrieve_episodes(query, 5).await?);

                let semantic = self.semantic.read().await;
                results.extend(semantic.retrieve_facts(query, 5).await?);

                Ok(results)
            }
        }
    }

    pub async fn consolidate(&self) -> MemoryResult<()> {
        let working = self.working_memory.read().await;
        let to_consolidate = working.get_low_priority_entries(10);

        for entry in to_consolidate {
            let mut episodic = self.episodic.write().await;
            episodic.store_episode(entry).await?;
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
            self.entries
                .sort_by(|a, b| a.importance.partial_cmp(&b.importance).unwrap());
            self.entries.pop();
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
