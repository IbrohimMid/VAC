use crate::chunking::SemanticChunker;
use crate::error::{ContextError, ContextResult};
use crate::shm::{Allocation, ShmArena};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub id: String,
    pub source_id: Option<String>,
    pub content: String,
    pub chunk_index: usize,
    pub attention_weight: f32,
    pub embedding: Option<Vec<f32>>,
    pub shm_alloc: Option<Allocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub entries: Vec<ContextEntry>,
    pub total_tokens: usize,
    pub max_tokens: usize,
}

pub struct ContextEngine {
    shm: Arc<ShmArena>,
    chunker: Arc<RwLock<SemanticChunker>>,
    index: Arc<RwLock<context_index::ContextIndex>>,
    config: ContextConfig,
}

#[derive(Debug, Clone)]
pub struct ContextConfig {
    pub max_context_tokens: usize,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub shm_path: PathBuf,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 8192,
            chunk_size: 512,
            chunk_overlap: 50,
            shm_path: PathBuf::from("/tmp/vil_context.shm"),
        }
    }
}

impl ContextEngine {
    pub async fn new(config: ContextConfig) -> ContextResult<Self> {
        info!("Initializing context engine with config: {:?}", config);

        let shm = Arc::new(
            ShmArena::new(&config.shm_path, 64 * 1024 * 1024)
                .map_err(|e| ContextError::ShmAllocation(e.to_string()))?,
        );

        let chunker = Arc::new(RwLock::new(SemanticChunker::new(
            config.chunk_size,
            config.chunk_overlap,
        )));

        let index = Arc::new(RwLock::new(context_index::ContextIndex::new()));

        Ok(Self {
            shm,
            chunker,
            index,
            config,
        })
    }

    pub async fn ingest(&self, content: &str) -> ContextResult<Vec<ContextEntry>> {
        self.ingest_source(None, content).await
    }

    pub async fn ingest_source(&self, source_id: Option<String>, content: &str) -> ContextResult<Vec<ContextEntry>> {
        debug!("Ingesting content of length: {} with source_id: {:?}", content.len(), source_id);

        if let Some(ref sid) = source_id {
            let mut index = self.index.write().await;
            let mut to_evict = Vec::new();
            for entry in index.entries.values() {
                if entry.source_id.as_ref() == Some(sid) {
                    to_evict.push(entry.id.clone());
                }
            }
            for id in to_evict {
                if let Some(entry) = index.remove_entry(&id) {
                    if let Some(alloc) = entry.shm_alloc {
                        if let Err(e) = self.shm.free(alloc.offset).await {
                            warn!("Failed to free SHM allocation during re-ingestion: {}", e);
                        }
                    }
                }
            }
        }

        let chunker = self.chunker.read().await;
        let chunks = chunker
            .chunk(content)
            .map_err(|e| ContextError::Chunking(e.to_string()))?;

        let mut entries = Vec::new();
        for (i, chunk) in chunks.iter().enumerate() {
            let alloc = match self.shm.allocate_and_write(chunk.as_bytes()).await {
                Ok(a) => Some(a),
                Err(e) => {
                    warn!("SHM allocation failed, falling back to heap: {}", e);
                    None
                }
            };

            let entry = ContextEntry {
                id: uuid::Uuid::new_v4().to_string(),
                source_id: source_id.clone(),
                content: chunk.clone(),
                chunk_index: i,
                attention_weight: 1.0 / chunks.len() as f32,
                embedding: None,
                shm_alloc: alloc,
            };
            entries.push(entry);
        }

        let mut index = self.index.write().await;
        for entry in &entries {
            index
                .add_entry(entry.clone())
                .map_err(|e| ContextError::Indexing(e.to_string()))?;
        }

        debug!("Ingested {} chunks", entries.len());
        Ok(entries)
    }

    pub async fn retrieve(&self, query: &str, top_k: usize) -> ContextResult<ContextWindow> {
        debug!("Retrieving context for query: {}", query);

        let index = self.index.read().await;
        let results = index
            .search(query, top_k)
            .map_err(|e| ContextError::Retrieval(e.to_string()))?;

        let entries: Vec<ContextEntry> = results.into_iter().map(|(entry, _score)| entry).collect();

        let total_tokens = entries
            .iter()
            .map(|e| e.content.split_whitespace().count())
            .sum();

        Ok(ContextWindow {
            entries,
            total_tokens,
            max_tokens: self.config.max_context_tokens,
        })
    }

    pub async fn update_attention(
        &self,
        entry_ids: &[String],
        weights: &[f32],
    ) -> ContextResult<()> {
        if entry_ids.len() != weights.len() {
            return Err(ContextError::Retrieval(
                "Entry IDs and weights length mismatch".to_string(),
            ));
        }

        let mut index = self.index.write().await;
        for (id, weight) in entry_ids.iter().zip(weights.iter()) {
            index
                .update_attention(id, *weight)
                .map_err(|e| ContextError::Retrieval(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn clear(&self) -> ContextResult<()> {
        let mut index = self.index.write().await;
        let entries = index.drain_entries();
        for entry in entries {
            if let Some(alloc) = entry.shm_alloc {
                if let Err(e) = self.shm.free(alloc.offset).await {
                    warn!("Failed to free SHM allocation during clear: {}", e);
                }
            }
        }
        Ok(())
    }

    pub async fn evict(&self, entry_id: &str) -> ContextResult<()> {
        let mut index = self.index.write().await;
        if let Some(entry) = index.remove_entry(entry_id) {
            if let Some(alloc) = entry.shm_alloc {
                self.shm.free(alloc.offset).await.map_err(|e| {
                    ContextError::ShmAllocation(format!("Failed to free evicting SHM: {}", e))
                })?;
            }
        }
        Ok(())
    }

    pub async fn shm_ptr(&self) -> *const u8 {
        self.shm.as_ptr_async().await
    }

    /// Return a cloned Arc to the SHM arena for wiring into ToolContext.
    pub fn shm_arc(&self) -> Option<Arc<crate::shm::ShmArena>> {
        Some(self.shm.clone())
    }
}

mod context_index {
    use super::ContextEntry;
    use std::collections::{HashMap, HashSet};

    pub struct ContextIndex {
        pub entries: HashMap<String, ContextEntry>,
        pub scores: HashMap<String, f32>,
    }

    impl ContextIndex {
        pub fn new() -> Self {
            Self {
                entries: HashMap::new(),
                scores: HashMap::new(),
            }
        }

        pub fn add_entry(&mut self, entry: ContextEntry) -> Result<(), String> {
            let id = entry.id.clone();
            let weight = entry.attention_weight;
            self.entries.insert(id.clone(), entry);
            self.scores.insert(id, weight);
            Ok(())
        }

        fn extract_keywords(text: &str) -> HashSet<String> {
            text.to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| s.len() > 2)
                .map(String::from)
                .collect()
        }

        pub fn search(
            &self,
            query: &str,
            top_k: usize,
        ) -> Result<Vec<(ContextEntry, f32)>, String> {
            let query_keywords = Self::extract_keywords(query);

            let mut scored: Vec<_> = self
                .entries
                .values()
                .map(|e| {
                    let mut score = self.scores.get(&e.id).copied().unwrap_or(0.0);
                    
                    if !query_keywords.is_empty() {
                        let entry_keywords = Self::extract_keywords(&e.content);
                        let overlap = query_keywords.intersection(&entry_keywords).count() as f32;
                        score += overlap;
                    }
                    
                    (e.clone(), score)
                })
                .collect();

            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(top_k);
            Ok(scored)
        }

        pub fn update_attention(&mut self, id: &str, weight: f32) -> Result<(), String> {
            self.scores.insert(id.to_string(), weight);
            Ok(())
        }

        pub fn drain_entries(&mut self) -> Vec<ContextEntry> {
            self.scores.clear();
            self.entries.drain().map(|(_, v)| v).collect()
        }

        pub fn remove_entry(&mut self, id: &str) -> Option<ContextEntry> {
            self.scores.remove(id);
            self.entries.remove(id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn test_config() -> ContextConfig {
        let temp = NamedTempFile::new().unwrap();
        ContextConfig {
            max_context_tokens: 1000,
            chunk_size: 50,
            chunk_overlap: 10,
            shm_path: temp.path().to_path_buf(),
        }
    }

    #[tokio::test]
    async fn test_ingest_and_retrieve_keywords() {
        let engine = ContextEngine::new(test_config()).await.unwrap();
        
        let content1 = "This is a test document about artificial intelligence and machine learning.";
        let content2 = "Another document entirely focused on gardening and planting trees.";
        
        engine.ingest(content1).await.unwrap();
        engine.ingest(content2).await.unwrap();

        let window = engine.retrieve("intelligence machine", 1).await.unwrap();
        assert_eq!(window.entries.len(), 1);
        assert!(window.entries[0].content.contains("artificial intelligence"));
    }

    #[tokio::test]
    async fn test_shm_memory_release_on_clear_and_evict() {
        let engine = ContextEngine::new(test_config()).await.unwrap();
        let content = "Some content to allocate in SHM";
        
        let entries = engine.ingest(content).await.unwrap();
        assert!(!entries.is_empty());
        let allocated_bytes = engine.shm.bytes_allocated().await;
        assert!(allocated_bytes > 0);

        // Test evict
        let entry_id = &entries[0].id;
        engine.evict(entry_id).await.unwrap();
        
        let after_evict_bytes = engine.shm.bytes_allocated().await;
        assert!(after_evict_bytes < allocated_bytes);

        // Test clear
        engine.ingest("More content").await.unwrap();
        assert!(engine.shm.bytes_allocated().await > 0);
        
        engine.clear().await.unwrap();
        assert_eq!(engine.shm.bytes_allocated().await, 0);
    }
    
    #[tokio::test]
    async fn test_reingest_source() {
        let engine = ContextEngine::new(test_config()).await.unwrap();
        let source_id = Some("source_1".to_string());
        
        engine.ingest_source(source_id.clone(), "Version 1 of the document").await.unwrap();
        let allocated_v1 = engine.shm.bytes_allocated().await;
        assert!(allocated_v1 > 0);
        
        // Re-ingest with the same source_id
        engine.ingest_source(source_id.clone(), "V2 doc").await.unwrap();
        
        // It should have freed v1 and allocated v2. 
        // We verify that total entries in index are for V2 only
        let window = engine.retrieve("doc", 10).await.unwrap();
        assert!(!window.entries.is_empty());
        for entry in window.entries {
            assert!(entry.content.contains("doc"));
            assert!(!entry.content.contains("Version 1"));
        }
    }
}
