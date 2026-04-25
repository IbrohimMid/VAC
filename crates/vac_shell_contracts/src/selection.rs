//! Model-selection snapshot DTO. Carries the operator-owned bits
//! (`active`, `recents`) that need to survive across boots; deliberately
//! excludes registry-owned bits (`providers`, `models`, credentials)
//! since those are derived from a host source on every load.
//!
//! Lives in contracts rather than `vac_shell_bridge` so the persistor
//! trait can stay serde-free in the bridge — concrete persistor
//! impls (JSON file, future config adapter) handle serialisation
//! through this DTO.

use serde::{Deserialize, Serialize};

use crate::model::ProviderId;

/// One `(provider, id)` pair the operator picked at some point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelKey {
    pub provider: ProviderId,
    pub id: String,
}

impl ModelKey {
    pub fn new(provider: ProviderId, id: impl Into<String>) -> Self {
        Self {
            provider,
            id: id.into(),
        }
    }
}

/// Operator-side selection state. Persistor impls round-trip values
/// of this shape; the host registry is consulted separately to
/// validate that the keys still resolve at boot time.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModelSelectionSnapshot {
    pub active: Option<ModelKey>,
    pub recent: Vec<ModelKey>,
}
