//! `vil_vwfd` — VIL Workflow Definition schema, loader, and validator.

pub mod error;
pub mod migrate;
pub mod schema;
pub mod version;

pub use error::VwfdError;
pub use schema::{
    VwfdDocument, VwfdExecutionMode, VwfdHandler, VwfdKind, VwfdMetadata, VwfdSpec, VwfdStep,
    VwfdTrigger, VwfdTriggerKind, VwfdWorkflow,
};
pub use version::SUPPORTED_API_VERSION;

/// Parse and validate a VWFD document from a YAML string.
pub fn from_yaml(input: &str) -> Result<VwfdDocument, VwfdError> {
    let doc: VwfdDocument = serde_yaml::from_str(input)?;
    let doc = doc.validate()?;
    migrate::migrate(doc)
}

/// Parse and validate a VWFD document from a file path.
pub fn from_file(path: &std::path::Path) -> Result<VwfdDocument, VwfdError> {
    let contents = std::fs::read_to_string(path)?;
    from_yaml(&contents)
}
