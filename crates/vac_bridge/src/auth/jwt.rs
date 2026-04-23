//! W7.2 — HMAC-SHA256 JWT minter + verifier with `kid` rotation.
//!
//! Scoped to **HS256 only**. Library-crate justification: bridge
//! peers are trusted subordinates of the same operator — we need a
//! signed session token, not a full asymmetric PKI. RS256/EdDSA is
//! a follow-up when remote-companion multi-tenancy lands.
//!
//! Key set model: [`JwtKeySet`] carries one or more `(kid, secret)`
//! pairs. `mint` signs with a caller-chosen kid; `verify` looks up
//! the kid from the token header and replies with an explicit
//! `Expired` / `UnknownKid` / `BadSignature` / `Malformed` error
//! so the transport can surface the right `InboundEvent::Detach`
//! reason.

use std::collections::HashMap;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Header {
    alg: String,
    kid: String,
    #[serde(default = "typ_jwt")]
    typ: String,
}

fn typ_jwt() -> String {
    "JWT".into()
}

/// Standard JWT claims + a free-form `extra` bag so callers attach
/// their own fields (session id, capabilities).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub sub: String,
    pub iss: String,
    /// Unix seconds.
    pub iat: i64,
    pub exp: i64,
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl Claims {
    pub fn new(sub: impl Into<String>, iss: impl Into<String>) -> Self {
        let now = now_unix();
        Self {
            sub: sub.into(),
            iss: iss.into(),
            iat: now,
            exp: now + 3600, // 1h default
            extra: serde_json::Value::Null,
        }
    }

    pub fn with_exp(mut self, exp_unix: i64) -> Self {
        self.exp = exp_unix;
        self
    }

    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = extra;
        self
    }
}

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Named HMAC secret — wrap `Vec<u8>` so debug printing can't leak.
#[derive(Clone)]
pub struct KeyMaterial(Vec<u8>);

impl KeyMaterial {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self(secret.into())
    }
    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KeyMaterial(<{} bytes>)", self.0.len())
    }
}

/// Rotation-aware key set. Keyed on `kid` string.
#[derive(Debug, Default, Clone)]
pub struct JwtKeySet {
    keys: HashMap<String, KeyMaterial>,
}

impl JwtKeySet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, kid: impl Into<String>, secret: KeyMaterial) {
        self.keys.insert(kid.into(), secret);
    }

    pub fn remove(&mut self, kid: &str) -> bool {
        self.keys.remove(kid).is_some()
    }

    pub fn contains(&self, kid: &str) -> bool {
        self.keys.contains_key(kid)
    }

    fn get(&self, kid: &str) -> Option<&KeyMaterial> {
        self.keys.get(kid)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("malformed token: {0}")]
    Malformed(String),
    #[error("unknown kid: {0}")]
    UnknownKid(String),
    #[error("expired: exp={exp} now={now}")]
    Expired { exp: i64, now: i64 },
    #[error("bad signature")]
    BadSignature,
    #[error("unsupported alg: {0}")]
    UnsupportedAlg(String),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("base64: {0}")]
    Base64(#[from] base64::DecodeError),
}

/// Mint a token signed by the key named `kid`.
pub fn mint(
    keys: &JwtKeySet,
    kid: &str,
    claims: &Claims,
) -> Result<String, JwtError> {
    let secret = keys
        .get(kid)
        .ok_or_else(|| JwtError::UnknownKid(kid.to_string()))?;
    let header = Header {
        alg: "HS256".into(),
        kid: kid.to_string(),
        typ: "JWT".into(),
    };
    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
    let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?);
    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = HmacSha256::new_from_slice(secret.bytes())
        .expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok(format!("{signing_input}.{sig}"))
}

/// Verify and parse a token. Returns the claims on success. The
/// `now_unix` parameter is injected so tests are deterministic.
pub fn verify(
    keys: &JwtKeySet,
    token: &str,
    now_unix: i64,
) -> Result<Claims, JwtError> {
    let mut parts = token.splitn(3, '.');
    let header_b64 = parts
        .next()
        .ok_or_else(|| JwtError::Malformed("missing header".into()))?;
    let payload_b64 = parts
        .next()
        .ok_or_else(|| JwtError::Malformed("missing payload".into()))?;
    let sig_b64 = parts
        .next()
        .ok_or_else(|| JwtError::Malformed("missing signature".into()))?;
    if parts.next().is_some() {
        return Err(JwtError::Malformed("too many segments".into()));
    }
    let header_raw = URL_SAFE_NO_PAD.decode(header_b64.as_bytes())?;
    let header: Header = serde_json::from_slice(&header_raw)?;
    if header.alg != "HS256" {
        return Err(JwtError::UnsupportedAlg(header.alg));
    }
    let secret = keys
        .get(&header.kid)
        .ok_or_else(|| JwtError::UnknownKid(header.kid.clone()))?;
    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = HmacSha256::new_from_slice(secret.bytes())
        .expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    let want = mac.finalize().into_bytes();
    let got = URL_SAFE_NO_PAD.decode(sig_b64.as_bytes())?;
    if got.as_slice() != want.as_slice() {
        return Err(JwtError::BadSignature);
    }
    let payload_raw = URL_SAFE_NO_PAD.decode(payload_b64.as_bytes())?;
    let claims: Claims = serde_json::from_slice(&payload_raw)?;
    if now_unix >= claims.exp {
        return Err(JwtError::Expired {
            exp: claims.exp,
            now: now_unix,
        });
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys_with(kid: &str, secret: &[u8]) -> JwtKeySet {
        let mut k = JwtKeySet::new();
        k.insert(kid, KeyMaterial::new(secret.to_vec()));
        k
    }

    #[test]
    fn mint_and_verify_roundtrip() {
        let keys = keys_with("k1", b"0123456789abcdef");
        let claims = Claims::new("user", "vac").with_exp(2_000_000_000);
        let tok = mint(&keys, "k1", &claims).unwrap();
        let back = verify(&keys, &tok, 1_000_000_000).unwrap();
        assert_eq!(back, claims);
    }

    #[test]
    fn verify_expired_reports_expiry() {
        let keys = keys_with("k1", b"s");
        let claims = Claims::new("u", "v").with_exp(100);
        let tok = mint(&keys, "k1", &claims).unwrap();
        let err = verify(&keys, &tok, 200).unwrap_err();
        match err {
            JwtError::Expired { exp, now } => {
                assert_eq!(exp, 100);
                assert_eq!(now, 200);
            }
            other => panic!("expected Expired, got {other:?}"),
        }
    }

    #[test]
    fn verify_unknown_kid() {
        let keys = keys_with("k1", b"s");
        let claims = Claims::new("u", "v").with_exp(9_000_000_000);
        let tok = mint(&keys, "k1", &claims).unwrap();
        // Rotate — drop k1, add k2.
        let mut keys2 = JwtKeySet::new();
        keys2.insert("k2", KeyMaterial::new(b"s2".to_vec()));
        let err = verify(&keys2, &tok, 0).unwrap_err();
        matches!(err, JwtError::UnknownKid(_));
    }

    #[test]
    fn verify_bad_signature() {
        let keys_sign = keys_with("k1", b"right");
        let keys_verify = keys_with("k1", b"wrong");
        let claims = Claims::new("u", "v").with_exp(9_000_000_000);
        let tok = mint(&keys_sign, "k1", &claims).unwrap();
        let err = verify(&keys_verify, &tok, 0).unwrap_err();
        matches!(err, JwtError::BadSignature);
    }

    #[test]
    fn mint_unknown_kid() {
        let keys = keys_with("k1", b"s");
        let claims = Claims::new("u", "v");
        let err = mint(&keys, "missing", &claims).unwrap_err();
        matches!(err, JwtError::UnknownKid(_));
    }

    #[test]
    fn malformed_segments_rejected() {
        let keys = keys_with("k1", b"s");
        let err = verify(&keys, "only.two", 0).unwrap_err();
        matches!(err, JwtError::Malformed(_));
    }

    #[test]
    fn kid_rotation_two_active_keys() {
        let mut keys = JwtKeySet::new();
        keys.insert("k1", KeyMaterial::new(b"secret1".to_vec()));
        keys.insert("k2", KeyMaterial::new(b"secret2".to_vec()));
        let claims = Claims::new("u", "v").with_exp(9_000_000_000);
        let t1 = mint(&keys, "k1", &claims).unwrap();
        let t2 = mint(&keys, "k2", &claims).unwrap();
        // Both verify successfully via the same key set.
        assert!(verify(&keys, &t1, 0).is_ok());
        assert!(verify(&keys, &t2, 0).is_ok());
        // After rotating k1 out, t1 must fail with UnknownKid.
        keys.remove("k1");
        matches!(
            verify(&keys, &t1, 0).unwrap_err(),
            JwtError::UnknownKid(_)
        );
        assert!(verify(&keys, &t2, 0).is_ok());
    }

    #[test]
    fn extra_claims_roundtrip() {
        let keys = keys_with("k1", b"s");
        let claims = Claims::new("u", "v")
            .with_exp(9_000_000_000)
            .with_extra(serde_json::json!({ "session": "abc", "caps": ["read"] }));
        let tok = mint(&keys, "k1", &claims).unwrap();
        let back = verify(&keys, &tok, 0).unwrap();
        assert_eq!(back.extra["session"], "abc");
    }

    #[test]
    fn unsupported_alg_in_header() {
        // Hand-craft a token with alg=none.
        let header = serde_json::json!({ "alg": "none", "kid": "k1", "typ": "JWT" });
        let h_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let p_b64 = URL_SAFE_NO_PAD.encode(b"{}");
        let s_b64 = URL_SAFE_NO_PAD.encode(b"nope");
        let tok = format!("{h_b64}.{p_b64}.{s_b64}");
        let keys = keys_with("k1", b"s");
        matches!(verify(&keys, &tok, 0).unwrap_err(), JwtError::UnsupportedAlg(_));
    }

    #[test]
    fn keymaterial_debug_hides_bytes() {
        let k = KeyMaterial::new(vec![1, 2, 3, 4]);
        let rendered = format!("{k:?}");
        assert!(rendered.contains("4 bytes"));
        assert!(!rendered.contains("1, 2, 3, 4"));
    }
}
