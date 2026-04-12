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
        debug!("Ingesting content of length: {}", content.len());

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

    pub fn shm_ptr(&self) -> *const u8 {
        self.shm.as_ptr()
    }
}

mod context_index {
    use super::ContextEntry;
    use std::collections::HashMap;

    pub struct ContextIndex {
        entries: HashMap<String, ContextEntry>,
        scores: HashMap<String, f32>,
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
            self.entries.insert(id.clone(), entry);
            self.scores.insert(id, 1.0);
            Ok(())
        }

        pub fn search(
            &self,
            _query: &str,
            top_k: usize,
        ) -> Result<Vec<(ContextEntry, f32)>, String> {
            let mut scored: Vec<_> = self
                .entries
                .values()
                .map(|e| {
                    let score = self.scores.get(&e.id).copied().unwrap_or(0.0);
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
    }
}
