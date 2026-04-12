//! VAC CBOR format — serialization and COSE_Sign1 signing.

use crate::error::{TraceError, TraceResult};
use crate::recorder::TraceRecord;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Signing key pair for Ed25519
pub struct SigningKeyPair {
    key: SigningKey,
}

impl SigningKeyPair {
    /// Generate a new Ed25519 key pair
    pub fn generate() -> Self {
        let key = SigningKey::generate(&mut OsRng);
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
    /// Creates a simple COSE_Sign1 structure: [protected headers, unprotected headers, payload, signature]
    pub fn sign(&self, keypair: &SigningKeyPair) -> TraceResult<Vec<u8>> {
        let payload = self.to_cbor()?;

        let mut protected = Vec::new();
        protected.push(0x01);

        let mut header_map = Vec::new();
        header_map.push((1, vec![0x26]));
        header_map.push((4, self.session_id.as_bytes().to_vec()));

        let protected_encoded = serde_cbor::to_vec(&header_map)
            .map_err(|e| TraceError::Export(format!("Header encoding failed: {e}")))?;

        let mut message = protected_encoded.clone();
        message.extend_from_slice(&payload);

        let signature = keypair.key.sign(&message);

        let mut cose_sign1 = Vec::new();
        cose_sign1.push(0xD8);
        cose_sign1.push(0x18);
        cose_sign1.push(0x2F);
        cose_sign1.extend_from_slice(&protected_encoded);
        cose_sign1.push(0xA0);
        cose_sign1.extend_from_slice(&payload);
        cose_sign1.extend_from_slice(signature.to_bytes().as_slice());

        tracing::info!(
            session_id = %self.session_id,
            size = cose_sign1.len(),
            "Envelope signed with Ed25519 COSE_Sign1"
        );

        Ok(cose_sign1)
    }
}
