use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct SpeculationCache {
    pub predicted_submit: Option<String>,
    pub precomputed_context: HashMap<String, String>,
}
