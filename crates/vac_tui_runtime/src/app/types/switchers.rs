use std::collections::HashSet;

use crate::types::{ListRuleBook, Model};

/// Grouped state for all four switchers (isolation, profile, rulebook, model).
#[derive(Debug, Clone)]
pub struct SwitchersState {
    // Isolation
    pub isolation_selected: usize,
    pub isolation_modes: Vec<String>,
    pub active_isolation_mode: String,

    // Profile
    pub profile_selected: usize,
    pub profile_search: String,
    pub available_profiles: Vec<String>,
    pub filtered_profiles: Vec<String>,
    pub active_profile: String,

    // Rulebook
    pub rulebook_selected: usize,
    pub rulebook_search: String,
    pub available_rulebooks: Vec<ListRuleBook>,
    pub filtered_rulebooks: Vec<ListRuleBook>,
    pub selected_rulebooks: HashSet<String>,

    // Model
    pub available_models: Vec<Model>,
    pub model_filter: String,
    pub model_selected: usize,
}

impl Default for SwitchersState {
    fn default() -> Self {
        Self {
            isolation_selected: 0,
            isolation_modes: vec![
                "host".to_string(),
                "isolated".to_string(),
                "isolated (Rust)".to_string(),
                "isolated (Node)".to_string(),
                "isolated (Python)".to_string(),
            ],
            active_isolation_mode: "host".to_string(),
            profile_selected: 0,
            profile_search: String::new(),
            available_profiles: Vec::new(),
            filtered_profiles: Vec::new(),
            active_profile: "default".to_string(),
            rulebook_selected: 0,
            rulebook_search: String::new(),
            available_rulebooks: Vec::new(),
            filtered_rulebooks: Vec::new(),
            selected_rulebooks: HashSet::new(),
            available_models: Vec::new(),
            model_filter: String::new(),
            model_selected: 0,
        }
    }
}
