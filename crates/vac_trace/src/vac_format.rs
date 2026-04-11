//! VAC CBOR format — serialization and COSE_Sign1 signing.

use crate::error::{TraceError, TraceResult};
use crate::recorder::TraceRecord;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// VAC artifact envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacEnvelope {
    pub version: String,
    pub session_id: String,
    pub created_at: String,
    pub records: Vec<TraceRecord>,
    pub checksum: String,
}

impl VacEnvelope {
    /// Create a new VAC envelope from trace records.
    pub fn new(session_id: &str, records: Vec<TraceRecord>) -> Self {
        let content = serde_json::to_string(&records).unwrap_or_default();
        let checksum = format!("{:x}", Sha256::digest(content.as_bytes()));

        Self {
            version: "0.1.0".to_string(),
            session_id: session_id.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            records,
            checksum,
        }
    }

    /// Serialize to CBOR bytes.
    pub fn to_cbor(&self) -> TraceResult<Vec<u8>> {
        serde_cbor::to_vec(self)
            .map_err(|e| TraceError::Export(format!("CBOR serialization failed: {e}")))
    }

    /// Sign the envelope with COSE_Sign1. (Stub — needs key material)
    pub fn sign(&self, _key: &[u8]) -> TraceResult<Vec<u8>> {
        tracing::warn!("COSE signing not yet fully implemented");
        self.to_cbor()
    }
}
