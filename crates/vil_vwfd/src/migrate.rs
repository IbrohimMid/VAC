//! Forward migration harness for VWFD schema versions.
//!
//! Stub — will be populated as `vil.vastar.io/v2` lands.

use crate::{VwfdDocument, error::VwfdError};

/// Apply any pending forward migrations to `doc`, returning the migrated form.
/// Currently a no-op (only v1 exists).
pub fn migrate(doc: VwfdDocument) -> Result<VwfdDocument, VwfdError> {
    Ok(doc)
}
