use std::path::PathBuf;
use tracing::{debug, info};

use crate::error::ToolError;

pub struct SkillLoader {
    skills_dir: PathBuf,
}

impl SkillLoader {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self { skills_dir }
    }

    pub async fn load_skills(&self) -> Result<Vec<Skill>, ToolError> {
        info!("Loading skills from {:?}", self.skills_dir);
        Ok(vec![])
    }

    pub async fn find_skill(&self, name: &str) -> Option<Skill> {
        debug!("Looking for skill: {}", name);
        None
    }
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub command: String,
    pub working_dir: Option<PathBuf>,
}

impl Skill {
    pub fn new(name: String, description: String, command: String) -> Self {
        Self {
            name,
            description,
            command,
            working_dir: None,
        }
    }

    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }
}
