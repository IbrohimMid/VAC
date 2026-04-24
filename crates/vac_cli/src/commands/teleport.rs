//! NS.3 — `vac teleport` HTTP/SSE transport.
//!
//! Two modes:
//!
//! - **host** (`--serve`) — Mints a teleport JWT, prints it, then
//!   starts an axum server on `--bind` with two routes:
//!   - `GET /events` (SSE) — requires `Authorization: Bearer <jwt>`.
//!     Emits `data: <json>\n\n` frames for each broadcast payload
//!     plus a `: heartbeat\n\n` comment every `heartbeat_secs`.
//!   - `POST /input` — requires the same bearer. Body is a JSON
//!     `InboundEvent`; the host appends it to the in-memory queue.
//!
//! - **attach** (`--attach <token>`) — reqwest SSE client against
//!   `--url`, prints each `data:` line to stdout, reads JSON lines
//!   from stdin and POSTs each to `/input`.
//!
//! Session integration is stub-shaped for now: the host broadcasts
//! test events + anything posted to `/input`. Wiring to a running
//! session's outbound stream is a follow-up (needs the TUI session
//! to expose a broadcast sink, which NS.2's stream migration sets
//! up but does not yet publish outside the process).
//!
//! Security posture:
//! - Default bind is `127.0.0.1:9042`. Non-loopback binds require
//!   `--insecure` (HTTP only today; TLS is TODO).
//! - JWT TTL is enforced by `vac_bridge::remote::validate_teleport_token`.
//! - No jti replay guard yet — relies on short TTL (15 min default).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use futures::StreamExt;
use tokio::sync::broadcast;
use vac_bridge::auth::jwt::{JwtKeySet, KeyMaterial};
use vac_bridge::remote::{
    DEFAULT_TELEPORT_TTL, RemoteSessionConfig, TeleportClaims, issue_teleport_token,
    validate_teleport_token,
};

/// Broadcast channel size. Slow subscribers that fall behind this
/// many events get `RecvError::Lagged` and reconnect — we prefer
/// that over unbounded memory growth.
const BROADCAST_BUF: usize = 256;

#[derive(Clone)]
struct AppState {
    keys: Arc<JwtKeySet>,
    events: broadcast::Sender<serde_json::Value>,
}

fn validate_bearer(
    keys: &JwtKeySet,
    headers: &HeaderMap,
) -> Result<TeleportClaims, (StatusCode, String)> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing authorization".into()))?;
    let token = auth
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "expected Bearer token".into()))?;
    validate_teleport_token(keys, token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("invalid token: {e}")))
}

async fn sse_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>, (StatusCode, String)>
{
    let claims = validate_bearer(&state.keys, &headers)?;
    tracing::info!(
        target: "vac_cli::teleport",
        session = %claims.session_id,
        label = %claims.label,
        "teleport attach connected",
    );
    let rx = state.events.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(v) => {
                let data = serde_json::to_string(&v).unwrap_or_else(|_| "{}".into());
                Some(Ok(Event::default().data(data)))
            }
            // Lagged: let the client reconnect; one frame loss is
            // cheaper than a silent hang.
            Err(_) => None,
        }
    });
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}

async fn input_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let _claims = validate_bearer(&state.keys, &headers)?;
    // Echo-broadcast the input so subscribers see it (MVP stub).
    let _ = state.events.send(serde_json::json!({
        "kind": "input",
        "payload": payload,
    }));
    Ok(Json(serde_json::json!({ "accepted": true })))
}

/// Derive a deterministic per-project key set from
/// `~/.vac/teleport.key` (created on first use with 32 random
/// bytes). Keeps the signing secret on the host only.
fn load_or_create_keyset(project_root: &std::path::Path) -> anyhow::Result<(JwtKeySet, String)> {
    let key_path = project_root.join(".vac").join("teleport.key");
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let secret = if key_path.exists() {
        std::fs::read(&key_path)?
    } else {
        use std::io::Write;
        let mut bytes = [0u8; 32];
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((seed.wrapping_mul(6364136223846793005u64).wrapping_add(i as u64)) & 0xff) as u8;
        }
        // Defense: mix in uuid v4 so two sessions starting in the
        // same nanosecond still diverge.
        let u = uuid::Uuid::new_v4();
        for (i, b) in u.as_bytes().iter().enumerate() {
            bytes[i] ^= *b;
        }
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&key_path)?;
        f.write_all(&bytes)?;
        bytes.to_vec()
    };
    let mut ks = JwtKeySet::new();
    let kid = "teleport-default";
    ks.insert(kid, KeyMaterial::new(secret));
    Ok((ks, kid.to_string()))
}

/// Host mode: mint a JWT and start the SSE server. Blocks until
/// SIGINT or the server exits.
pub async fn teleport_serve(
    project_root: PathBuf,
    bind: String,
    label: String,
    insecure: bool,
) -> anyhow::Result<()> {
    let addr: SocketAddr = bind
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid --bind address '{bind}': {e}"))?;
    if !addr.ip().is_loopback() && !insecure {
        anyhow::bail!(
            "refusing to bind {addr} — non-loopback bind requires --insecure (HTTP only today; TLS TODO)",
        );
    }
    let (keys, kid) = load_or_create_keyset(&project_root)?;
    let session_id = uuid::Uuid::new_v4().simple().to_string();
    let token =
        issue_teleport_token(&keys, &kid, &session_id, &label, DEFAULT_TELEPORT_TTL)
            .map_err(|e| anyhow::anyhow!("mint teleport token: {e}"))?;

    let (tx, _rx0) = broadcast::channel::<serde_json::Value>(BROADCAST_BUF);
    let state = AppState {
        keys: Arc::new(keys),
        events: tx.clone(),
    };

    let cfg = RemoteSessionConfig::default();
    let sse_url = format!("http://{addr}/events");
    let input_url = format!("http://{addr}/input");

    println!("── vac teleport (serve) ──────────────────────");
    println!("session_id    {session_id}");
    println!("label         {label}");
    println!("bind          {addr}");
    println!("sse_url       {sse_url}");
    println!("input_url     {input_url}");
    println!("default_trust {}", cfg.default_trust.label());
    println!("ttl_seconds   {}", DEFAULT_TELEPORT_TTL.as_secs());
    println!();
    println!("Bearer token (15-min TTL — give this to the attach side):");
    println!("{token}");
    println!();
    println!("Attach with:");
    println!("  vac teleport --attach {token} --url http://{addr}");
    println!();

    // MVP event source: emit a startup frame so attach clients see
    // the connection immediately. Session integration lands later.
    let _ = tx.send(serde_json::json!({
        "kind": "session_started",
        "session_id": session_id,
        "label": label,
    }));

    let app = Router::new()
        .route("/events", get(sse_handler))
        .route("/input", post(input_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("bind {addr}: {e}"))?;
    tracing::info!(target: "vac_cli::teleport", %addr, "teleport server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| anyhow::anyhow!("axum serve: {e}"))?;
    Ok(())
}

/// Attach mode: SSE client + stdin pump.
pub async fn teleport_attach(token: String, url: String) -> anyhow::Result<()> {
    let sse_url = if url.ends_with("/events") {
        url.clone()
    } else {
        format!("{}/events", url.trim_end_matches('/'))
    };
    let input_url = if url.ends_with("/input") {
        url.clone()
    } else {
        format!("{}/input", url.trim_end_matches('/'))
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(0))
        .build()?;

    println!("── vac teleport (attach) ─────────────────────");
    println!("sse_url   {sse_url}");
    println!("input_url {input_url}");
    println!("(stdin lines posted as JSON to /input; ctrl-C to exit)");
    println!();

    // Spawn stdin pump.
    let stdin_client = client.clone();
    let stdin_url = input_url.clone();
    let stdin_token = token.clone();
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stdin));
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            let payload: serde_json::Value = serde_json::from_str(&line)
                .unwrap_or_else(|_| serde_json::json!({ "text": line }));
            let resp = stdin_client
                .post(&stdin_url)
                .bearer_auth(&stdin_token)
                .json(&payload)
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => eprintln!("POST /input → {}", r.status()),
                Err(e) => eprintln!("POST /input error: {e}"),
            }
        }
    });

    // SSE consumer: parse `data:` lines out of the chunked body.
    let resp = client
        .get(&sse_url)
        .bearer_auth(&token)
        .header("accept", "text/event-stream")
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("SSE connect {} → {}", sse_url, resp.status());
    }
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(idx) = buf.find("\n\n") {
            let frame: String = buf.drain(..idx + 2).collect();
            for raw in frame.lines() {
                if let Some(data) = raw.strip_prefix("data: ") {
                    println!("{}", data);
                } else if raw.starts_with(':') {
                    // SSE comment / heartbeat — silent.
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trip_post_reaches_sse_subscriber() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().to_path_buf();

        // Bind an ephemeral loopback port so tests can run in parallel.
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (keys, kid) = load_or_create_keyset(&project).unwrap();
        let token = issue_teleport_token(
            &keys,
            &kid,
            "session-test",
            "test",
            DEFAULT_TELEPORT_TTL,
        )
        .unwrap();
        let (tx, _rx0) = broadcast::channel::<serde_json::Value>(BROADCAST_BUF);
        let state = AppState {
            keys: Arc::new(keys),
            events: tx.clone(),
        };
        let app = Router::new()
            .route("/events", get(sse_handler))
            .route("/input", post(input_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let bound = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        // Subscribe to events via raw reqwest SSE.
        let client = reqwest::Client::new();
        let url_events = format!("http://{bound}/events");
        let url_input = format!("http://{bound}/input");
        let resp = client
            .get(&url_events)
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let mut body = resp.bytes_stream();

        // Give the server a beat to attach subscriber, then POST.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let post_resp = client
            .post(&url_input)
            .bearer_auth(&token)
            .json(&serde_json::json!({ "text": "hello" }))
            .send()
            .await
            .unwrap();
        assert!(post_resp.status().is_success());

        // Read one SSE frame.
        let mut buf = String::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            let chunk = match tokio::time::timeout(
                Duration::from_secs(1),
                body.next(),
            )
            .await
            {
                Ok(Some(Ok(c))) => c,
                _ => continue,
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));
            if buf.contains("\"kind\":\"input\"") {
                break;
            }
        }
        assert!(
            buf.contains("\"kind\":\"input\""),
            "expected input frame in SSE stream, got: {buf}",
        );
    }

    #[tokio::test]
    async fn sse_rejects_missing_bearer() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().to_path_buf();
        let (keys, _kid) = load_or_create_keyset(&project).unwrap();
        let (tx, _) = broadcast::channel::<serde_json::Value>(BROADCAST_BUF);
        let state = AppState {
            keys: Arc::new(keys),
            events: tx,
        };
        let app = Router::new()
            .route("/events", get(sse_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bound = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let resp = reqwest::Client::new()
            .get(format!("http://{bound}/events"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sse_rejects_wrong_bearer() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().to_path_buf();
        let (keys, _kid) = load_or_create_keyset(&project).unwrap();
        let (tx, _) = broadcast::channel::<serde_json::Value>(BROADCAST_BUF);
        let state = AppState {
            keys: Arc::new(keys),
            events: tx,
        };
        let app = Router::new()
            .route("/events", get(sse_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bound = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let resp = reqwest::Client::new()
            .get(format!("http://{bound}/events"))
            .bearer_auth("not-a-real-token")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn load_or_create_keyset_roundtrips_through_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().to_path_buf();
        let (k1, kid1) = load_or_create_keyset(&p).unwrap();
        let (k2, kid2) = load_or_create_keyset(&p).unwrap();
        assert_eq!(kid1, kid2);
        let tok = issue_teleport_token(&k1, &kid1, "s", "l", DEFAULT_TELEPORT_TTL)
            .unwrap();
        // Second keyset must validate tokens issued by the first —
        // proves the on-disk secret is the source of truth.
        assert!(validate_teleport_token(&k2, &tok).is_ok());
    }
}
