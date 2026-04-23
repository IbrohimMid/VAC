//! Skill errors. Kept independent of `anyhow` on the trait surface so
//! consumers can match on variants — bundled skills may still use
//! `anyhow` internally and translate at the boundary.

use std::fmt;

#[derive(Debug)]
pub enum SkillError {
    /// Registry lookup miss.
    NotFound(String),
    /// Two skills registered with the same name.
    Duplicate(String),
    /// Input failed schema validation.
    InvalidInput(String),
    /// Skill ran to termination but returned a domain error.
    Execution(String),
    /// Underlying I/O, serialization, or subsystem error.
    Other(anyhow::Error),
}

impl fmt::Display for SkillError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(name) => write!(f, "skill not found: {name}"),
            Self::Duplicate(name) => write!(f, "duplicate skill registration: {name}"),
            Self::InvalidInput(msg) => write!(f, "invalid skill input: {msg}"),
            Self::Execution(msg) => write!(f, "skill execution failed: {msg}"),
            Self::Other(e) => write!(f, "skill error: {e}"),
        }
    }
}

impl std::error::Error for SkillError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<anyhow::Error> for SkillError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

impl From<std::io::Error> for SkillError {
    fn from(e: std::io::Error) -> Self {
        Self::Other(anyhow::anyhow!(e))
    }
}

impl From<serde_json::Error> for SkillError {
    fn from(e: serde_json::Error) -> Self {
        Self::InvalidInput(e.to_string())
    }
}

pub type SkillResult<T> = Result<T, SkillError>;
