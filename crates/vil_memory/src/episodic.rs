use crate::error::{MemoryError, MemoryResult};
use crate::store::MemoryEntry;
use redb::{Database, MultimapTableDefinition, ReadableTable, TableDefinition};
use std::collections::BinaryHeap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

const EPISODES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("episodes");
/// Inverted index: term → set of episode ids.
const EPISODE_INDEX: MultimapTableDefinition<&str, &str> =
    MultimapTableDefinition::new("episode_index");

pub struct EpisodicMemory {
    db: Arc<RwLock<Database>>,
}

impl EpisodicMemory {
    pub async fn new(path: &Path) -> MemoryResult<Self> {
        let db = Database::create(path).map_err(|e| MemoryError::Storage(e.to_string()))?;

        {
            let tx = db
                .begin_write()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            tx.open_table(EPISODES_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            tx.open_multimap_table(EPISODE_INDEX)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            tx.commit()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        Ok(Self {
            db: Arc::new(RwLock::new(db)),
        })
    }

    /// Store an episode and index its content terms for later retrieval.
    pub async fn store_episode(&self, entry: MemoryEntry) -> MemoryResult<()> {
        let key = entry.id.clone();
        let value =
            serde_json::to_string(&entry).map_err(|e| MemoryError::Storage(e.to_string()))?;
        let terms = extract_terms(&entry.content);

        let db = self.db.write().await;
        let tx = db
            .begin_write()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        {
            let mut table = tx
                .open_table(EPISODES_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            table
                .insert(key.as_str(), value.as_str())
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        {
            let mut index = tx
                .open_multimap_table(EPISODE_INDEX)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            for term in &terms {
                index
                    .insert(term.as_str(), key.as_str())
                    .map_err(|e| MemoryError::Storage(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        debug!("Stored episode: {}", key);
        Ok(())
    }

    /// Retrieve episodes matching `query` by inverted-index lookup, scored by
    /// number of matching terms. Returns at most `limit` results.
    pub async fn retrieve_episodes(
        &self,
        query: &str,
        limit: usize,
    ) -> MemoryResult<Vec<MemoryEntry>> {
        if limit == 0 {
            return Ok(vec![]);
        }

        let query_terms = extract_terms(query);
        if query_terms.is_empty() {
            return Ok(vec![]);
        }

        let db = self.db.read().await;
        let tx = db
            .begin_read()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        // Count how many query terms each candidate episode matches.
        let mut scores: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        {
            let index = tx
                .open_multimap_table(EPISODE_INDEX)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;

            for term in &query_terms {
                if let Ok(values) = index.get(term.as_str()) {
                    for v in values {
                        let id = v
                            .map_err(|e| MemoryError::Storage(e.to_string()))?
                            .value()
                            .to_string();
                        *scores.entry(id).or_insert(0) += 1;
                    }
                }
            }
        }

        // Pick top-`limit` candidates by score via a max-heap.
        // Each element: (score, id) — Ord on tuples is lexicographic, so higher
        // score wins.
        let mut heap: BinaryHeap<(usize, String)> =
            scores.into_iter().map(|(id, score)| (score, id)).collect();

        let episodes_table = tx
            .open_table(EPISODES_TABLE)
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        let mut results = Vec::with_capacity(limit);
        while results.len() < limit {
            let Some((_, id)) = heap.pop() else { break };
            if let Ok(Some(v)) = episodes_table.get(id.as_str()) {
                if let Ok(entry) = serde_json::from_str::<MemoryEntry>(v.value()) {
                    results.push(entry);
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
            if let Ok(memory_entry) = serde_json::from_str::<MemoryEntry>(value.value()) {
                results.push(memory_entry);
            }
        }

        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results.truncate(count);

        Ok(results)
    }
}

/// Extract indexable terms from text: lowercase tokens ≥ 2 chars, split on
/// non-alphanumeric boundaries, excluding common stop-words.
fn extract_terms(content: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "and", "with", "for", "that", "this", "are", "was", "from", "have", "not",
    ];
    content
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 2 && !STOP_WORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn make_entry(id: &str, content: &str) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            content: content.to_string(),
            memory_type: crate::store::MemoryType::Episodic,
            timestamp: chrono::Utc::now(),
            importance: 0.5,
            access_count: 0,
        }
    }

    #[tokio::test]
    async fn retrieve_respects_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mem = EpisodicMemory::new(&dir.path().join("ep.db"))
            .await
            .unwrap();
        for i in 0..10 {
            mem.store_episode(make_entry(&format!("e{i}"), "rust memory test"))
                .await
                .unwrap();
        }
        let results = mem.retrieve_episodes("rust", 3).await.unwrap();
        assert_eq!(results.len(), 3, "limit must be respected");
    }

    #[tokio::test]
    async fn retrieve_works_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ep.db");
        {
            let mem = EpisodicMemory::new(&path).await.unwrap();
            mem.store_episode(make_entry("abc", "rust borrow checker"))
                .await
                .unwrap();
        }
        // Reopen — cache gone, data must survive on disk.
        let mem2 = EpisodicMemory::new(&path).await.unwrap();
        let results = mem2.retrieve_episodes("borrow", 10).await.unwrap();
        assert!(
            results.iter().any(|e| e.id == "abc"),
            "episode must be recoverable after reopen"
        );
    }

    #[test]
    fn extract_terms_handles_code_and_short_tokens() {
        let terms = extract_terms("api rpc lsp rust_lifetime");
        // "api", "rpc", "lsp" are 3 chars but ≥ 2, must be included.
        assert!(terms.contains(&"api".to_string()));
        assert!(terms.contains(&"rpc".to_string()));
        assert!(terms.contains(&"lsp".to_string()));
        assert!(terms.contains(&"rust_lifetime".to_string()));
    }
}
