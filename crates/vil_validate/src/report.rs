//! Validation report generation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub file: String,
    pub overall_score: f64,
    pub pass_results: Vec<PassResult>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassResult {
    pub name: String,
    pub score: f64,
    pub details: String,
}
