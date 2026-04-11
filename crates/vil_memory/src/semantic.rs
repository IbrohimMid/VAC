use crate::error::{MemoryError, MemoryResult};
use crate::store::MemoryEntry;
use redb::{Database, TableDefinition};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

const FACTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("facts");
const INDEX_TABLE: TableDefinition<&str, &str> = TableDefinition::new("fact_index");

pub struct SemanticMemory {
    db: Arc<RwLock<Database>>,
    cache: Arc<RwLock<HashMap<String, MemoryEntry>>>,
}

impl SemanticMemory {
    pub async fn new(path: &Path) -> MemoryResult<Self> {
        let db_path = path.with_extension("semantic.db");
        let db = Database::create(&db_path).map_err(|e| MemoryError::Storage(e.to_string()))?;

        {
            let db = db
                .begin_write()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            db.open_table(FACTS_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            db.open_table(INDEX_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        Ok(Self {
            db: Arc::new(RwLock::new(db)),
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn store_fact(&mut self, entry: MemoryEntry) -> MemoryResult<()> {
        let key = entry.id.clone();
        let key_str = key.as_str();
        let key_for_cache = key.clone();
        let value =
            serde_json::to_string(&entry).map_err(|e| MemoryError::Storage(e.to_string()))?;

        {
            let db = self.db.write().await;
            let tx = db
                .begin_write()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;

            {
                let mut table = tx
                    .open_table(FACTS_TABLE)
                    .map_err(|e| MemoryError::Storage(e.to_string()))?;
                table
                    .insert(key_str, value.as_str())
                    .map_err(|e| MemoryError::Storage(e.to_string()))?;
            }

            for term in self.extract_terms(&entry.content) {
                let mut index_table = tx
                    .open_table(INDEX_TABLE)
                    .map_err(|e| MemoryError::Storage(e.to_string()))?;
                index_table
                    .insert(term.as_str(), key_str)
                    .map_err(|e| MemoryError::Storage(e.to_string()))?;
            }

            tx.commit()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        {
            let mut cache = self.cache.write().await;
            cache.insert(key_for_cache, entry);
        }

        debug!("Stored semantic fact: {}", key_str);
        Ok(())
    }

    pub async fn retrieve_facts(
        &self,
        query: &str,
        _limit: usize,
    ) -> MemoryResult<Vec<MemoryEntry>> {
        let terms = self.extract_terms(query);
        let mut candidate_ids = Vec::new();

        {
            let db = self.db.read().await;
            let tx = db
                .begin_read()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;

            for term in terms {
                if let Ok(index_table) = tx.open_table(INDEX_TABLE) {
                    if let Ok(Some(value)) = index_table.get(term.as_str()) {
                        let id = value.value().to_string();
                        candidate_ids.push(id);
                    }
                }
            }
        }

        let mut results = Vec::new();
        for id in candidate_ids {
            if let Some(entry) = self.cache.read().await.get(&id) {
                results.push(entry.clone());
            }
        }

        Ok(results)
    }

    fn extract_terms(&self, content: &str) -> Vec<String> {
        content
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(|w| w.to_string())
            .collect()
    }

    pub async fn search_by_concept(&self, concept: &str) -> MemoryResult<Vec<MemoryEntry>> {
        self.retrieve_facts(concept, 20).await
    }
}
