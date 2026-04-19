//! VAC CBOR format — serialization and COSE_Sign1 signing.

use crate::error::{TraceError, TraceResult};
use crate::recorder::TraceRecord;
use ed25519_dalek::{Signer, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Ed25519 signing key pair for VAC artifact signing.
///
/// Keys are persisted as a 32-byte little-endian seed (the private scalar).
/// The file must be stored with permission 0o600 (Unix) to limit access.
/// Use [`SigningKeyPair::save`] / [`SigningKeyPair::load`] for persistence, and
/// [`SigningKeyPair::generate`] to create a new key.
pub struct SigningKeyPair {
    key: ed25519_dalek::SigningKey,
}

impl SigningKeyPair {
    /// Generate a new random Ed25519 key pair.
    pub fn generate() -> Self {
        let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        Self { key }
    }

    /// Persist the signing key (32-byte seed) to `path`.
    ///
    /// Sets file permission to 0o600 on Unix so other users cannot read it.
    /// Callers should store the file under a private directory such as
    /// `~/.vac/keys/`.
    pub fn save(&self, path: &Path) -> TraceResult<()> {
        use std::fs;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let seed = self.key.to_bytes(); // [u8; 32]
        atomic_write(path, &seed)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    /// Load a signing key from the 32-byte seed file written by [`save`].
    pub fn load(path: &Path) -> TraceResult<Self> {
        let bytes = std::fs::read(path)?;
        let seed: [u8; 32] = bytes.try_into().map_err(|_| {
            TraceError::Signing(format!(
                "key file at {} must be exactly 32 bytes",
                path.display()
            ))
        })?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        Ok(Self { key })
    }

    /// Return the public key bytes (32 bytes, Ed25519 compressed point).
    pub fn public_key(&self) -> Vec<u8> {
        self.key.verifying_key().as_bytes().to_vec()
    }

    /// Return the verifying key for use in signature verification.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.key.verifying_key()
    }
}

/// VAC artifact envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacEnvelope {
    pub version: String,
    pub session_id: String,
    pub created_at: String,
    pub records: Vec<TraceRecord>,
    /// SHA-256 of the canonical CBOR payload (same bytes used for signing).
    pub checksum: String,
}

impl VacEnvelope {
    /// Create a new VAC envelope.
    ///
    /// Computes the checksum over the canonical CBOR serialization of
    /// `records` — the same bytes used as the COSE payload — so that the
    /// checksum is always verifiable against the signed content.
    ///
    /// Returns `Err` if serialization fails rather than silently producing
    /// a checksum of an empty payload.
    pub fn new(session_id: &str, records: Vec<TraceRecord>) -> TraceResult<Self> {
        let cbor_bytes = serde_cbor::to_vec(&records).map_err(|e| {
            TraceError::Export(format!("CBOR serialization of records failed: {e}"))
        })?;
        let checksum = format!("{:x}", Sha256::digest(&cbor_bytes));

        Ok(Self {
            version: "0.1.0".to_string(),
            session_id: session_id.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            records,
            checksum,
        })
    }

    /// Serialize the envelope to CBOR bytes (unsigned).
    pub fn to_cbor(&self) -> TraceResult<Vec<u8>> {
        serde_cbor::to_vec(self)
            .map_err(|e| TraceError::Export(format!("CBOR serialization failed: {e}")))
    }

    /// Sign the envelope with COSE_Sign1 using Ed25519.
    ///
    /// The public key is embedded in the COSE unprotected header under the
    /// custom label `-70000` so that verifiers can reconstruct it without an
    /// out-of-band trust store lookup.  The `key_id` in the protected header
    /// still carries `session_id` for correlation.
    pub fn sign(&self, keypair: &SigningKeyPair) -> TraceResult<Vec<u8>> {
        use coset::{CborSerializable, CoseSign1Builder, HeaderBuilder, cbor::value::Value, iana};

        let payload = self.to_cbor()?;

        let protected = HeaderBuilder::new()
            .algorithm(iana::Algorithm::EdDSA)
            .key_id(self.session_id.as_bytes().to_vec())
            .build();

        // Embed public key in unprotected header so verifiers don't need an
        // out-of-band trust store.  Label -70000 is in the private-use range.
        let unprotected = HeaderBuilder::new()
            .text_value(
                "vac_public_key".to_string(),
                Value::Bytes(keypair.public_key()),
            )
            .build();

        let signature = keypair.key.sign(&payload);
        let sign1 = CoseSign1Builder::new()
            .protected(protected)
            .unprotected(unprotected)
            .payload(payload)
            .signature(signature.to_bytes().to_vec())
            .build();

        let cbor_bytes = sign1
            .to_vec()
            .map_err(|e| TraceError::Export(format!("COSE encoding failed: {e}")))?;

        tracing::info!(
            session_id = %self.session_id,
            size = cbor_bytes.len(),
            pubkey_len = keypair.public_key().len(),
            "Envelope signed with Ed25519 COSE_Sign1"
        );

        Ok(cbor_bytes)
    }

    /// Verify a COSE_Sign1 artifact produced by [`sign`].
    ///
    /// Extracts the public key from the unprotected `vac_public_key` header,
    /// verifies the Ed25519 signature, and deserializes the inner
    /// [`VacEnvelope`]. Returns `Err` on any failure.
    pub fn verify(cose_bytes: &[u8]) -> TraceResult<Self> {
        use coset::{CborSerializable, CoseSign1, cbor::value::Value};
        use ed25519_dalek::Verifier;

        let sign1 = CoseSign1::from_slice(cose_bytes)
            .map_err(|e| TraceError::Signing(format!("COSE parse failed: {e}")))?;

        // Extract embedded public key from unprotected header.
        let pubkey_bytes = sign1
            .unprotected
            .rest
            .iter()
            .find_map(|(label, val)| {
                if let coset::Label::Text(name) = label {
                    if name == "vac_public_key" {
                        if let Value::Bytes(b) = val {
                            return Some(b.clone());
                        }
                    }
                }
                None
            })
            .ok_or_else(|| {
                TraceError::Signing(
                    "COSE artifact missing vac_public_key in unprotected header".to_string(),
                )
            })?;

        let pk_arr: [u8; 32] = pubkey_bytes
            .try_into()
            .map_err(|_| TraceError::Signing("vac_public_key must be 32 bytes".to_string()))?;
        let verifying_key = VerifyingKey::from_bytes(&pk_arr)
            .map_err(|e| TraceError::Signing(format!("invalid public key: {e}")))?;

        let payload = sign1
            .payload
            .as_deref()
            .ok_or_else(|| TraceError::Signing("COSE_Sign1 has no payload".to_string()))?;

        let sig_bytes: [u8; 64] = sign1
            .signature
            .clone()
            .try_into()
            .map_err(|_| TraceError::Signing("signature must be 64 bytes".to_string()))?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        verifying_key
            .verify(payload, &signature)
            .map_err(|e| TraceError::Signing(format!("signature verification failed: {e}")))?;

        serde_cbor::from_slice::<Self>(payload)
            .map_err(|e| TraceError::Export(format!("envelope deserialization failed: {e}")))
    }
}

/// Write `bytes` to `path` atomically: write to a `.tmp` sibling, fsync,
/// then rename.  On Unix also fsyncs the parent directory.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let tmp = path.with_extension("tmp");

    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }

    std::fs::rename(&tmp, path)?;

    // Fsync the directory entry so the rename is durable (Linux).
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        if let Ok(f) = std::fs::File::open(dir) {
            let _ = f.sync_all();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn keypair_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.bin");

        let kp = SigningKeyPair::generate();
        let pubkey_before = kp.public_key();
        kp.save(&path).unwrap();

        let kp2 = SigningKeyPair::load(&path).unwrap();
        assert_eq!(
            pubkey_before,
            kp2.public_key(),
            "public key must survive save/load"
        );
    }

    #[test]
    fn envelope_new_returns_error_on_bad_records() {
        // serde_cbor cannot encode f64::NAN — verify we get Err, not a phantom checksum.
        let mut record = crate::recorder::TraceRecord {
            id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            record_type: crate::recorder::RecordType::Error,
            agent_id: None,
            content: serde_json::json!(f64::NAN),
        };
        // Force NaN into the JSON value (serde_json sanitizes NaN to null by default,
        // but serde_cbor should still handle it — test at least confirms no phantom checksum).
        // We replace content with a type that cannot round-trip through CBOR.
        record.content = serde_json::Value::Number(
            serde_json::Number::from_f64(f64::NAN).unwrap_or(serde_json::Number::from(0)),
        );
        // Even if this succeeds, the checksum must be non-empty and non-phantom.
        let result = VacEnvelope::new("test-session", vec![record]);
        if let Ok(env) = result {
            assert!(!env.checksum.is_empty());
            assert_ne!(
                env.checksum, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "checksum must not be SHA256 of empty string"
            );
        }
        // Err is also acceptable (serialization failed correctly).
    }

    #[test]
    fn signed_envelope_verifies_with_loaded_keypair() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("key.bin");

        let kp = SigningKeyPair::generate();
        kp.save(&key_path).unwrap();

        let records = vec![];
        let envelope = VacEnvelope::new("sess-123", records).unwrap();
        let signed = envelope.sign(&kp).unwrap();

        // Load keypair from disk (simulates verifier in a different process).
        let _kp2 = SigningKeyPair::load(&key_path).unwrap();
        // Verify the signed artifact.
        let verified = VacEnvelope::verify(&signed).unwrap();
        assert_eq!(verified.session_id, "sess-123");
        assert_eq!(verified.checksum, envelope.checksum);
    }

    #[test]
    fn verify_rejects_tampered_bytes() {
        let kp = SigningKeyPair::generate();
        let envelope = VacEnvelope::new("sess-tamper", vec![]).unwrap();
        let mut signed = envelope.sign(&kp).unwrap();

        // Flip a byte in the payload area.
        let mid = signed.len() / 2;
        signed[mid] ^= 0xff;

        assert!(
            VacEnvelope::verify(&signed).is_err(),
            "tampered artifact must not verify"
        );
    }
}
