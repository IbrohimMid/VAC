use crate::error::{MemoryError, MemoryResult};
use crate::store::MemoryEntry;
use redb::{Database, ReadableTable, TableDefinition};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

const EPISODES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("episodes");

pub struct EpisodicMemory {
    db: Arc<RwLock<Database>>,
}

impl EpisodicMemory {
    pub async fn new(path: &Path) -> MemoryResult<Self> {
        let db = Database::create(path).map_err(|e| MemoryError::Storage(e.to_string()))?;

        {
            let db = db
                .begin_write()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            db.open_table(EPISODES_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        Ok(Self {
            db: Arc::new(RwLock::new(db)),
        })
    }

    pub async fn store_episode(&mut self, entry: MemoryEntry) -> MemoryResult<()> {
        let key = entry.id.as_str();
        let value =
            serde_json::to_string(&entry).map_err(|e| MemoryError::Storage(e.to_string()))?;

        let db = self.db.write().await;
        let tx = db
            .begin_write()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        {
            let mut table = tx
                .open_table(EPISODES_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            table
                .insert(key, value.as_str())
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        tx.commit()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        debug!("Stored episode: {}", entry.id);
        Ok(())
    }

    pub async fn retrieve_episodes(
        &self,
        query: &str,
        _limit: usize,
    ) -> MemoryResult<Vec<MemoryEntry>> {
        let db = self.db.read().await;
        let tx = db
            .begin_read()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        let table = tx
            .open_table(EPISODES_TABLE)
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        let mut results = Vec::new();

        for entry in table
            .iter()
            .map_err(|e| MemoryError::Storage(e.to_string()))?
        {
            let (_, value) = entry.map_err(|e| MemoryError::Storage(e.to_string()))?;
            let value_str = value.value();

            if let Ok(memory_entry) = serde_json::from_str::<MemoryEntry>(value_str) {
                if memory_entry
                    .content
                    .to_lowercase()
                    .contains(&query.to_lowercase())
                {
                    results.push(memory_entry);
                }
            }
        }

        Ok(results)
    }

    pub async fn get_recent(&self, count: usize) -> MemoryResult<Vec<MemoryEntry>> {
        let db = self.db.read().await;
        let tx = db
            .begin_read()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        let table = tx
            .open_table(EPISODES_TABLE)
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        let mut results = Vec::new();

        for entry in table
            .iter()
            .map_err(|e| MemoryError::Storage(e.to_string()))?
        {
            let (_, value) = entry.map_err(|e| MemoryError::Storage(e.to_string()))?;
            let value_str = value.value();

            if let Ok(memory_entry) = serde_json::from_str::<MemoryEntry>(value_str) {
                results.push(memory_entry);
            }
        }

        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results.truncate(count);

        Ok(results)
    }
}
