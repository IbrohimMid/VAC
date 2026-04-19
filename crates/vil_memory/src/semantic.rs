use crate::error::{MemoryError, MemoryResult};
use crate::store::MemoryEntry;
use redb::{Database, MultimapTableDefinition, TableDefinition};
use std::collections::BinaryHeap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

const FACTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("facts");
/// Inverted index: term → set of fact ids.  Uses MultimapTable so multiple
/// facts sharing a term are all preserved (single-value TableDefinition would
/// overwrite on each insert — the original bug).
const INDEX_TABLE: MultimapTableDefinition<&str, &str> = MultimapTableDefinition::new("fact_index");

pub struct SemanticMemory {
    db: Arc<RwLock<Database>>,
}

impl SemanticMemory {
    pub async fn new(path: &Path) -> MemoryResult<Self> {
        let db_path = path.with_extension("semantic.db");
        let db = Database::create(&db_path).map_err(|e| MemoryError::Storage(e.to_string()))?;

        {
            let tx = db
                .begin_write()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            tx.open_table(FACTS_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            tx.open_multimap_table(INDEX_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            tx.commit()
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        Ok(Self {
            db: Arc::new(RwLock::new(db)),
        })
    }

    /// Store a semantic fact and index all its content terms.
    pub async fn store_fact(&self, entry: MemoryEntry) -> MemoryResult<()> {
        let key = entry.id.clone();
        let value =
            serde_json::to_string(&entry).map_err(|e| MemoryError::Storage(e.to_string()))?;
        let terms = extract_terms(&entry.content);

        let db = self.db.write().await;
        let tx = db
            .begin_write()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        {
            let mut facts = tx
                .open_table(FACTS_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            facts
                .insert(key.as_str(), value.as_str())
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        {
            let mut index = tx
                .open_multimap_table(INDEX_TABLE)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            for term in &terms {
                index
                    .insert(term.as_str(), key.as_str())
                    .map_err(|e| MemoryError::Storage(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        debug!("Stored semantic fact: {}", key);
        Ok(())
    }

    /// Retrieve facts matching `query` by inverted-index lookup, scored by
    /// number of matching terms. Returns at most `limit` results. Reads from
    /// disk — correct after process restart, unlike a volatile in-memory cache.
    pub async fn retrieve_facts(
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

        // Count matching terms per candidate fact.
        let mut scores: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        {
            let index = tx
                .open_multimap_table(INDEX_TABLE)
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

        // Top-`limit` by score.
        let mut heap: BinaryHeap<(usize, String)> =
            scores.into_iter().map(|(id, score)| (score, id)).collect();

        let facts_table = tx
            .open_table(FACTS_TABLE)
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        let mut results = Vec::with_capacity(limit);
        while results.len() < limit {
            let Some((_, id)) = heap.pop() else { break };
            if let Ok(Some(v)) = facts_table.get(id.as_str()) {
                if let Ok(entry) = serde_json::from_str::<MemoryEntry>(v.value()) {
                    results.push(entry);
                }
            }
        }

        Ok(results)
    }

    pub async fn search_by_concept(&self, concept: &str) -> MemoryResult<Vec<MemoryEntry>> {
        self.retrieve_facts(concept, 20).await
    }
}

/// Extract indexable terms: lowercase alphanumeric tokens ≥ 2 chars,
/// excluding common English stop-words.
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
            memory_type: crate::store::MemoryType::Semantic,
            timestamp: chrono::Utc::now(),
            importance: 0.5,
            access_count: 0,
        }
    }

    #[tokio::test]
    async fn two_facts_with_shared_term_both_retrievable() {
        let dir = tempfile::tempdir().unwrap();
        let mem = SemanticMemory::new(&dir.path().join("mem.db"))
            .await
            .unwrap();

        mem.store_fact(make_entry("f1", "rust lifetime annotation"))
            .await
            .unwrap();
        mem.store_fact(make_entry("f2", "rust borrow checker"))
            .await
            .unwrap();

        let results = mem.retrieve_facts("rust", 10).await.unwrap();
        let ids: Vec<&str> = results.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"f1"), "f1 should be retrievable via 'rust'");
        assert!(ids.contains(&"f2"), "f2 should be retrievable via 'rust'");
    }

    #[tokio::test]
    async fn retrieve_works_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mem.db");
        {
            let mem = SemanticMemory::new(&path).await.unwrap();
            mem.store_fact(make_entry("persisted", "borrow checker lifetime"))
                .await
                .unwrap();
        }
        // Reopen — no in-memory cache; must read from disk.
        let mem2 = SemanticMemory::new(&path).await.unwrap();
        let results = mem2.retrieve_facts("lifetime", 10).await.unwrap();
        assert!(
            results.iter().any(|e| e.id == "persisted"),
            "fact must survive a process restart"
        );
    }

    #[tokio::test]
    async fn limit_truncates_results() {
        let dir = tempfile::tempdir().unwrap();
        let mem = SemanticMemory::new(&dir.path().join("mem.db"))
            .await
            .unwrap();
        for i in 0..10 {
            mem.store_fact(make_entry(&format!("f{i}"), "rust memory management"))
                .await
                .unwrap();
        }
        let results = mem.retrieve_facts("rust", 3).await.unwrap();
        assert_eq!(results.len(), 3, "limit must be honoured");
    }
}
