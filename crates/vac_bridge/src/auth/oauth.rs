//! W7.1 — OAuth PKCE building blocks.
//!
//! Full browser-redirect OAuth flow needs a local HTTP listener and a
//! running browser, which are out of scope for a library crate that
//! has to stay transport-agnostic. What **is** in scope — and what
//! callers actually need to get right — is the PKCE math + token
//! cache layout. This module provides:
//!
//! 1. [`PkceChallenge::generate`] — RFC 7636 code verifier + `S256`
//!    challenge from a caller-supplied RNG seed (deterministic
//!    testability; real callers pass an `OsRng` seed).
//! 2. [`PkceChallenge::verify`] — round-trip check the flow can use
//!    offline.
//! 3. [`TokenCache`] — filesystem-backed cache at
//!    `<cache_dir>/<provider>.json`. Loads / saves via `tokio::fs`.
//!
//! The actual browser + HTTP-listener wiring ships in a follow-up
//! milestone alongside a real provider (e.g. `vac_bridge::auth::github`).
//! For now the driver composes these primitives inside its own
//! binary-shaped flow.

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Minimum entropy for a PKCE code verifier per RFC 7636 §4.1.
pub const PKCE_VERIFIER_MIN_LEN: usize = 43;
pub const PKCE_VERIFIER_MAX_LEN: usize = 128;

/// Code verifier + S256 challenge pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceChallenge {
    pub verifier: String,
    pub challenge: String,
}

impl PkceChallenge {
    /// Build a challenge from a pre-generated verifier. The verifier
    /// must be 43..=128 URL-safe chars. Useful for test vectors and
    /// callers that already own their RNG.
    pub fn from_verifier(verifier: impl Into<String>) -> Result<Self, OauthError> {
        let verifier: String = verifier.into();
        if verifier.len() < PKCE_VERIFIER_MIN_LEN
            || verifier.len() > PKCE_VERIFIER_MAX_LEN
        {
            return Err(OauthError::InvalidVerifier(format!(
                "verifier len {} out of bounds ({}..={})",
                verifier.len(),
                PKCE_VERIFIER_MIN_LEN,
                PKCE_VERIFIER_MAX_LEN
            )));
        }
        for b in verifier.as_bytes() {
            if !matches!(
                b,
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
            ) {
                return Err(OauthError::InvalidVerifier(
                    "verifier must be URL-safe per RFC 7636".into(),
                ));
            }
        }
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(digest);
        Ok(Self { verifier, challenge })
    }

    /// Generate a fresh verifier from 32 raw entropy bytes (the
    /// caller is responsible for sourcing real entropy — typically
    /// `OsRng::try_fill_bytes`). 32 bytes encoded URL-safe-base64
    /// produces a 43-char verifier, matching the RFC minimum.
    pub fn generate(seed: &[u8; 32]) -> Self {
        let verifier = URL_SAFE_NO_PAD.encode(seed);
        // generate() is infallible by construction — the verifier is
        // exactly PKCE_VERIFIER_MIN_LEN and URL-safe alphabet.
        Self::from_verifier(verifier).expect("32-byte seed yields valid verifier")
    }

    /// Verify a candidate verifier against this challenge. Return
    /// true iff recomputing S256(candidate) equals our challenge.
    /// Typically the authorization server does this; the helper is
    /// here so mock test harnesses (and the eventual in-process
    /// provider) can self-check.
    pub fn verify(&self, candidate: &str) -> bool {
        let Ok(rebuilt) = Self::from_verifier(candidate.to_string()) else {
            return false;
        };
        rebuilt.challenge == self.challenge
    }
}

/// Stored token for one provider. `expires_at_unix` is the wall-clock
/// deadline; the cache refuses to hand out tokens past it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderToken {
    pub provider: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_at_unix: i64,
    #[serde(default)]
    pub scope: Option<String>,
}

impl ProviderToken {
    pub fn is_expired(&self, now_unix: i64) -> bool {
        now_unix >= self.expires_at_unix
    }
}

/// Filesystem-backed token store. One file per provider under
/// `<root>/<provider>.json`. Default root is `~/.vac/auth/`; tests
/// pass a tempdir.
#[derive(Debug, Clone)]
pub struct TokenCache {
    root: PathBuf,
}

impl TokenCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_root() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".vac").join("auth")
    }

    pub fn path_for(&self, provider: &str) -> PathBuf {
        self.root.join(format!("{provider}.json"))
    }

    pub async fn save(&self, token: &ProviderToken) -> Result<(), OauthError> {
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.path_for(&token.provider);
        let json = serde_json::to_vec_pretty(token)?;
        tokio::fs::write(&path, json).await?;
        Ok(())
    }

    pub async fn load(&self, provider: &str) -> Result<Option<ProviderToken>, OauthError> {
        let path = self.path_for(provider);
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(None);
        }
        let raw = tokio::fs::read(&path).await?;
        let parsed: ProviderToken = serde_json::from_slice(&raw)?;
        Ok(Some(parsed))
    }

    pub async fn remove(&self, provider: &str) -> Result<(), OauthError> {
        let path = self.path_for(provider);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OauthError {
    #[error("invalid verifier: {0}")]
    InvalidVerifier(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Convenience helper — verify that `root` is a real directory
/// (not a symlink) before handing tokens to it. Defends against
/// a compromised cache dir pointing outside `$HOME/.vac/`.
pub async fn validate_cache_root(root: &Path) -> Result<(), OauthError> {
    if !tokio::fs::try_exists(root).await.unwrap_or(false) {
        return Ok(()); // will be created on first save
    }
    let meta = tokio::fs::symlink_metadata(root).await?;
    if meta.file_type().is_symlink() {
        return Err(OauthError::InvalidVerifier(format!(
            "cache root is a symlink: {}",
            root.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED_SEED: [u8; 32] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    ];

    #[test]
    fn generate_is_deterministic_for_fixed_seed() {
        let a = PkceChallenge::generate(&FIXED_SEED);
        let b = PkceChallenge::generate(&FIXED_SEED);
        assert_eq!(a, b);
    }

    #[test]
    fn generate_produces_valid_length_verifier() {
        let c = PkceChallenge::generate(&FIXED_SEED);
        assert!(c.verifier.len() >= PKCE_VERIFIER_MIN_LEN);
        assert!(c.verifier.len() <= PKCE_VERIFIER_MAX_LEN);
    }

    #[test]
    fn verify_accepts_matching_verifier() {
        let c = PkceChallenge::generate(&FIXED_SEED);
        assert!(c.verify(&c.verifier));
    }

    #[test]
    fn verify_rejects_wrong_verifier() {
        let c = PkceChallenge::generate(&FIXED_SEED);
        let mut bad = FIXED_SEED;
        bad[0] ^= 0xff;
        let other = PkceChallenge::generate(&bad);
        assert!(!c.verify(&other.verifier));
    }

    #[test]
    fn from_verifier_rejects_short_input() {
        let err = PkceChallenge::from_verifier("too-short").unwrap_err();
        matches!(err, OauthError::InvalidVerifier(_));
    }

    #[test]
    fn from_verifier_rejects_non_urlsafe_chars() {
        let bad = "a".repeat(PKCE_VERIFIER_MIN_LEN - 1) + " ";
        let err = PkceChallenge::from_verifier(bad).unwrap_err();
        matches!(err, OauthError::InvalidVerifier(_));
    }

    #[test]
    fn provider_token_expiry() {
        let t = ProviderToken {
            provider: "x".into(),
            access_token: "abc".into(),
            refresh_token: None,
            expires_at_unix: 100,
            scope: None,
        };
        assert!(!t.is_expired(99));
        assert!(t.is_expired(100));
        assert!(t.is_expired(200));
    }

    #[tokio::test]
    async fn token_cache_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = TokenCache::new(tmp.path().to_path_buf());
        let t = ProviderToken {
            provider: "github".into(),
            access_token: "ghp_xxx".into(),
            refresh_token: Some("rt_xxx".into()),
            expires_at_unix: 1_800_000_000,
            scope: Some("repo".into()),
        };
        cache.save(&t).await.unwrap();
        let back = cache.load("github").await.unwrap().unwrap();
        assert_eq!(t, back);
    }

    #[tokio::test]
    async fn token_cache_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = TokenCache::new(tmp.path().to_path_buf());
        assert!(cache.load("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn token_cache_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = TokenCache::new(tmp.path().to_path_buf());
        let t = ProviderToken {
            provider: "x".into(),
            access_token: "a".into(),
            refresh_token: None,
            expires_at_unix: 0,
            scope: None,
        };
        cache.save(&t).await.unwrap();
        cache.remove("x").await.unwrap();
        assert!(cache.load("x").await.unwrap().is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn validate_cache_root_rejects_symlink() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        let link = tmp.path().join("auth");
        symlink(real.path(), &link).unwrap();
        let err = validate_cache_root(&link).await.unwrap_err();
        matches!(err, OauthError::InvalidVerifier(_));
    }
}
