use crate::engine::ContextEntry;
use std::collections::HashMap;

pub struct AttentionRouter {
    attention_weights: HashMap<String, f32>,
}

#[allow(clippy::new_without_default)]
impl AttentionRouter {
    pub fn new() -> Self {
        Self {
            attention_weights: HashMap::new(),
        }
    }

    pub fn compute_attention(&mut self, entries: &[ContextEntry], query: &str) -> Vec<f32> {
        let query_terms: Vec<&str> = query.split_whitespace().collect();

        entries
            .iter()
            .map(|entry| {
                let content_terms: Vec<&str> = entry.content.split_whitespace().collect();
                let overlap = query_terms
                    .iter()
                    .filter(|q| content_terms.contains(q))
                    .count();

                let base_weight = entry.attention_weight;
                let relevance = overlap as f32 / query_terms.len().max(1) as f32;

                base_weight * (1.0 + relevance)
            })
            .collect()
    }

    pub fn update_weights(&mut self, entry_id: &str, weight: f32) {
        self.attention_weights.insert(entry_id.to_string(), weight);
    }

    pub fn get_weight(&self, entry_id: &str) -> f32 {
        self.attention_weights.get(entry_id).copied().unwrap_or(1.0)
    }
}
