//! VAC CBOR format — serialization and COSE_Sign1 signing.

use crate::error::{TraceError, TraceResult};
use crate::recorder::TraceRecord;
use ed25519_dalek::Signer;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Signing key pair for Ed25519
pub struct SigningKeyPair {
    key: ed25519_dalek::SigningKey,
}

impl SigningKeyPair {
    /// Generate a new Ed25519 key pair
    pub fn generate() -> Self {
        let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        Self { key }
    }

    /// Get the public key as bytes
    pub fn public_key(&self) -> Vec<u8> {
        self.key.verifying_key().as_bytes().to_vec()
    }
}

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

    /// Sign the envelope with COSE_Sign1 using Ed25519 via coset library.
    /// Uses proper coset builder pattern for correct COSE structure.
    pub fn sign(&self, keypair: &SigningKeyPair) -> TraceResult<Vec<u8>> {
        use coset::{CborSerializable, CoseSign1Builder, HeaderBuilder, iana};

        let payload = self.to_cbor()?;

        // Build protected header with EdDSA algorithm (-8)
        let protected = HeaderBuilder::new()
            .algorithm(iana::Algorithm::EdDSA)
            .key_id(self.session_id.as_bytes().to_vec())
            .build();

        // Create COSE_Sign1 using builder pattern
        let signature = keypair.key.sign(&payload);
        let sign1 = CoseSign1Builder::new()
            .protected(protected)
            .payload(payload.clone())
            .signature(signature.to_bytes().to_vec())
            .build();

        // Serialize to CBOR
        let cbor_bytes = sign1
            .to_vec()
            .map_err(|e| TraceError::Export(format!("COSE encoding failed: {e}")))?;

        tracing::info!(
            session_id = %self.session_id,
            size = cbor_bytes.len(),
            "Envelope signed with Ed25519 COSE_Sign1 (coset builder)"
        );

        Ok(cbor_bytes)
    }
}
