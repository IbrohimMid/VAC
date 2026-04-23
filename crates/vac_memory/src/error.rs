use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MemoryError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("frontmatter: {0}")]
    Frontmatter(String),
    #[error("lock held by another writer (pid {0})")]
    Locked(u32),
    #[error("policy '{0}' rejected input: {1}")]
    PolicyRejected(String, String),
    #[error("other: {0}")]
    Other(String),
}

pub type MemoryResult<T> = std::result::Result<T, MemoryError>;
