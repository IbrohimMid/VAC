//! VIL Knowledge — bootstrap library for VIL patterns and best practices.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub patterns: HashMap<String, Pattern>,
    pub blueprints: Vec<Blueprint>,
    pub best_practices: Vec<BestPractice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub category: String,
    pub description: String,
    pub code_template: String,
    pub when_to_use: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    pub name: String,
    pub description: String,
    pub modules: Vec<String>,
    pub data_flow: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestPractice {
    pub rule: String,
    pub rationale: String,
    pub examples: Vec<String>,
}

impl KnowledgeBase {
    pub fn bootstrap() -> Self {
        tracing::info!("Bootstrapping VIL knowledge base");
        Self {
            patterns: HashMap::new(),
            blueprints: vec![],
            best_practices: vec![],
        }
    }
}
