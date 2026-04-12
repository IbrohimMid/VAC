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

    /// Sign the envelope with COSE_Sign1 using Ed25519.
    /// Creates proper COSE_Sign1 structure: [protected, unprotected, payload, signature]
    pub fn sign(&self, keypair: &SigningKeyPair) -> TraceResult<Vec<u8>> {
        let payload = self.to_cbor()?;

        // Build protected header: map with algorithm (-8 = EdDSA)
        let protected_map: Vec<(i64, Vec<u8>)> = vec![
            (1, vec![0x26]), // Algorithm: -8 (EdDSA) as CBOR tag
        ];
        let protected_bytes = serde_cbor::to_vec(&protected_map)
            .map_err(|e| TraceError::Export(format!("Protected header encoding: {e}")))?;

        // Build unprotected header: map with kid
        let kid_bytes = self.session_id.as_bytes();
        let unprotected_map: Vec<(String, Vec<u8>)> = vec![("kid".to_string(), kid_bytes.to_vec())];
        let unprotected_bytes = serde_cbor::to_vec(&unprotected_map)
            .map_err(|e| TraceError::Export(format!("Unprotected header encoding: {e}")))?;

        // Create signature payload: sign(protected || payload)
        let mut sig_payload = protected_bytes.clone();
        sig_payload.extend_from_slice(&payload);
        let signature = keypair.key.sign(&sig_payload);
        let sig_bytes = signature.to_bytes().to_vec();

        // Build COSE_Sign1: tag(18) || protected || unprotected || payload || signature
        let mut cose_sign1 = Vec::new();
        cose_sign1.push(0xd8); // tag
        cose_sign1.push(0x12); // tag 18 (COSE_Sign1)
        cose_sign1.extend_from_slice(&protected_bytes); // protected headers
        cose_sign1.extend_from_slice(&unprotected_bytes); // unprotected headers
        cose_sign1.extend_from_slice(&payload); // payload
        cose_sign1.extend_from_slice(&sig_bytes); // signature

        tracing::info!(
            session_id = %self.session_id,
            size = cose_sign1.len(),
            "Envelope signed with Ed25519 COSE_Sign1"
        );

        Ok(cose_sign1)
    }
}
