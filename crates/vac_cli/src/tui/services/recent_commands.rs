use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecentCommands {
    pub history: Vec<String>,
    pub frequencies: HashMap<String, usize>,
    #[serde(default)]
    pub recent_models: Vec<String>,
}

impl RecentCommands {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load() -> Self {
        if let Some(path) = Self::get_path() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(data) = serde_json::from_str(&content) {
                    return data;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::get_path() {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(content) = serde_json::to_string_pretty(self) {
                let _ = fs::write(path, content);
            }
        }
    }

    pub fn add_command(&mut self, command: String) {
        self.history.retain(|c| c != &command);
        self.history.insert(0, command.clone());
        if self.history.len() > 100 {
            self.history.truncate(100);
        }
        *self.frequencies.entry(command).or_insert(0) += 1;
        self.save();
    }

    pub fn add_model(&mut self, model_id: String) {
        self.recent_models.retain(|m| m != &model_id);
        self.recent_models.insert(0, model_id);
        if self.recent_models.len() > 10 {
            self.recent_models.truncate(10);
        }
        self.save();
    }

    pub fn get_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".vac").join("recent_commands.json"))
    }
}
