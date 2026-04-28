//! D.7 — SSE + JWT remote-session primitives.
//!
//! Operators run `vac teleport --attach <token>` on a remote
//! machine; the token is a short-lived JWT the host machine
//! issued (via [`crate::auth::jwt`]). The attached client
//! subscribes to an SSE stream of `OutboundEvent`s and POSTs
//! `InboundEvent`s back — minimal wire shape so the host agent
//! loop doesn't care where input came from.
//!
//! Today this module ships:
//! - `TeleportToken` (JWT wrapper with claims),
//! - `issue_teleport_token()` that signs one,
//! - `validate_teleport_token()` for the attach side,
//! - `RemoteSessionConfig` that bundles the transport settings.
//!
//! Actual HTTP server + SSE transport lives in the driver
//! binary (the engine crate stays transport-free).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::error::{BridgeError, BridgeResult};

/// B5 — event payload a live session publishes on the teleport
/// broadcast channel. Kept minimal: an opaque `kind` string + a
/// JSON `payload`. Attach clients just forward these frames over
/// SSE; they don't inspect the shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundEvent {
    pub kind: String,
    pub payload: serde_json::Value,
}

impl OutboundEvent {
    pub fn new(kind: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            kind: kind.into(),
            payload,
        }
    }
}

/// Broadcast channel size. Slow attach subscribers that fall
/// behind this many events hit `RecvError::Lagged` and reconnect;
/// we prefer that over unbounded memory growth.
pub const OUTBOUND_BROADCAST_BUF: usize = 256;

/// Shared handle a running session writes to and the teleport
/// host's `/events` SSE subscribes to. One handle per session;
/// construct via `SessionBroadcast::new()` in the session wiring
/// path, hand the sender into the submit loop and the `Arc<Self>`
/// into the teleport server.
#[derive(Clone)]
pub struct SessionBroadcast {
    tx: broadcast::Sender<OutboundEvent>,
}

impl SessionBroadcast {
    pub fn new() -> Arc<Self> {
        let (tx, _rx0) = broadcast::channel(OUTBOUND_BROADCAST_BUF);
        Arc::new(Self { tx })
    }

    /// Publish an event. Lost sends (no active subscribers) are
    /// intentional silent no-ops — not every session has a
    /// teleport listener.
    pub fn publish(&self, event: OutboundEvent) {
        let _ = self.tx.send(event);
    }

    /// Subscribe a new receiver. Teleport `/events` handler calls
    /// this per attach.
    pub fn subscribe(&self) -> broadcast::Receiver<OutboundEvent> {
        self.tx.subscribe()
    }

    /// Active subscriber count. For diagnostics / future rate
    /// limiting.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }

    pub fn sender(&self) -> broadcast::Sender<OutboundEvent> {
        self.tx.clone()
    }
}

impl Default for SessionBroadcast {
    fn default() -> Self {
        let (tx, _rx0) = broadcast::channel(OUTBOUND_BROADCAST_BUF);
        Self { tx }
    }
}

/// Claims baked into a teleport JWT. Minimal — no permissions
/// grammar here; the host side composes PermissionMatcher +
/// TrustGate the same way it does locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeleportClaims {
    /// Host session id the remote attaches to.
    pub session_id: String,
    /// Unix issuance timestamp.
    pub iat: i64,
    /// Unix expiry.
    pub exp: i64,
    /// Short operator label — surfaces in the activity row when
    /// the attach happens.
    pub label: String,
}

/// Default TTL for a teleport token (15 min). Short enough that
/// a leaked token isn't a weeks-long problem; long enough to
/// survive a flaky reconnection.
pub const DEFAULT_TELEPORT_TTL: Duration = Duration::from_secs(15 * 60);

/// Minimum TTL the issuer accepts. A token with TTL=0 races its
/// own issuance against the wall clock — the attach side nearly
/// always sees "expired" before it can attach. Require at least
/// 30 s so the token has room to travel + validate.
pub const MIN_TELEPORT_TTL: Duration = Duration::from_secs(30);

/// Config bundle the remote transport reads.
#[derive(Debug, Clone)]
pub struct RemoteSessionConfig {
    /// SSE endpoint the attach client subscribes to.
    pub sse_url: String,
    /// POST endpoint the attach client sends InboundEvents to.
    pub inbound_url: String,
    /// Heartbeat interval sent over SSE so intermediaries don't
    /// time the connection out.
    pub heartbeat: Duration,
    /// Default trust class enforced on remote calls until the
    /// operator elevates. `RestrictedOffline` per the plan.
    pub default_trust: RemoteTrustClass,
}

impl Default for RemoteSessionConfig {
    fn default() -> Self {
        Self {
            sse_url: "http://127.0.0.1:9042/events".into(),
            inbound_url: "http://127.0.0.1:9042/inbound".into(),
            heartbeat: Duration::from_secs(15),
            default_trust: RemoteTrustClass::RestrictedOffline,
        }
    }
}

/// Remote-only trust class default. Matches
/// `vac_runtime::isolation` but carried standalone so bridge
/// consumers don't have to depend on vac_runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteTrustClass {
    RestrictedOffline,
    Isolated,
    TrustedNetworked,
    Host,
}

impl RemoteTrustClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::RestrictedOffline => "restricted-offline",
            Self::Isolated => "isolated",
            Self::TrustedNetworked => "trusted-networked",
            Self::Host => "host",
        }
    }
}

/// Mint a teleport JWT using the shared-keyset signing primitive
/// from `crate::auth::jwt`.
pub fn issue_teleport_token(
    keys: &crate::auth::jwt::JwtKeySet,
    kid: &str,
    session_id: impl Into<String>,
    label: impl Into<String>,
    ttl: Duration,
) -> BridgeResult<String> {
    if ttl < MIN_TELEPORT_TTL {
        return Err(BridgeError::Protocol(format!(
            "teleport TTL {}s below minimum {}s — token would race its own expiry",
            ttl.as_secs(),
            MIN_TELEPORT_TTL.as_secs(),
        )));
    }
    let iat = unix_now();
    let exp = iat + ttl.as_secs() as i64;
    let session_id = session_id.into();
    let claims = crate::auth::jwt::Claims::new(session_id.clone(), "vac-teleport")
        .with_exp(exp)
        .with_extra(serde_json::json!({
            "label": label.into(),
            "iat": iat,
            "session_id": session_id,
        }));
    crate::auth::jwt::mint(keys, kid, &claims)
        .map_err(|e| BridgeError::Protocol(format!("teleport sign: {e}")))
}

/// Validate a teleport token against the shared keyset + wall-
/// clock. Returns the distilled [`TeleportClaims`] on success.
pub fn validate_teleport_token(
    keys: &crate::auth::jwt::JwtKeySet,
    token: &str,
) -> BridgeResult<TeleportClaims> {
    let now = unix_now();
    let claims = crate::auth::jwt::verify(keys, token, now)
        .map_err(|e| BridgeError::Protocol(format!("teleport verify: {e}")))?;
    let session_id = claims.sub.clone();
    let label = claims
        .extra
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(TeleportClaims {
        session_id,
        iat: claims.iat,
        exp: claims.exp,
        label,
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::auth::jwt::{JwtKeySet, KeyMaterial};

    fn keys(secret: &[u8]) -> JwtKeySet {
        let mut ks = JwtKeySet::new();
        ks.insert("kid-1", KeyMaterial::new(secret.to_vec()));
        ks
    }

    #[test]
    fn issue_then_validate_roundtrips() {
        let ks = keys(b"hush little secret");
        let token = issue_teleport_token(
            &ks,
            "kid-1",
            "session-123",
            "laptop attach",
            Duration::from_secs(60),
        )
        .unwrap();
        let claims = validate_teleport_token(&ks, &token).unwrap();
        assert_eq!(claims.session_id, "session-123");
        assert_eq!(claims.label, "laptop attach");
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn wrong_secret_fails() {
        let right = keys(b"right");
        let wrong = keys(b"wrong");
        let token =
            issue_teleport_token(&right, "kid-1", "s", "l", Duration::from_secs(60)).unwrap();
        assert!(validate_teleport_token(&wrong, &token).is_err());
    }

    #[test]
    fn short_ttl_is_rejected() {
        let ks = keys(b"k");
        let err = issue_teleport_token(&ks, "kid-1", "s", "l", Duration::from_secs(5)).unwrap_err();
        assert!(format!("{err}").contains("below minimum"), "{err}");
    }

    #[test]
    fn default_config_locks_to_restricted_offline() {
        let cfg = RemoteSessionConfig::default();
        assert_eq!(cfg.default_trust, RemoteTrustClass::RestrictedOffline);
    }
}
