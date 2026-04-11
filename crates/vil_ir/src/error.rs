use thiserror::Error;

#[derive(Error, Debug)]
pub enum IrError {
    #[error("Parse error in {file}: {message}")]
    Parse { file: String, message: String },

    #[error("Type resolution error: {0}")]
    TypeResolution(String),

    #[error("Borrow analysis error: {0}")]
    BorrowAnalysis(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type IrResult<T> = Result<T, IrError>;
