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
use vac_bridge::auth::jwt::{JwtKeySet, KeyMaterial};
use vac_bridge::remote::{
    DEFAULT_TELEPORT_TTL, OutboundEvent, RemoteSessionConfig, SessionBroadcast,
    TeleportClaims, issue_teleport_token, validate_teleport_token,
};

#[derive(Clone)]
struct AppState {
    keys: Arc<JwtKeySet>,
    events: Arc<SessionBroadcast>,
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
            Ok(ev) => {
                let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
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
    state
        .events
        .publish(OutboundEvent::new("input", payload));
    Ok(Json(serde_json::json!({ "accepted": true })))
}

/// Load or create the teleport HMAC secret under
/// `<project>/.vac/teleport.key`. 32 bytes drawn from the OS CSPRNG
/// on first use; unix-0600 enforced at create time and re-checked
/// on read (refusing world-readable secrets OpenSSH-style).
fn load_or_create_keyset(project_root: &std::path::Path) -> anyhow::Result<(JwtKeySet, String)> {
    let vac_dir = project_root.join(".vac");
    std::fs::create_dir_all(&vac_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&vac_dir)?.permissions();
        if perms.mode() & 0o077 != 0 {
            perms.set_mode(0o700);
            let _ = std::fs::set_permissions(&vac_dir, perms);
        }
    }
    let key_path = vac_dir.join("teleport.key");
    let secret = if key_path.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key_path)?.permissions().mode();
            if mode & 0o077 != 0 {
                anyhow::bail!(
                    "refusing to read {} — mode {:o} is too permissive (want 0600); \
                     chmod it or delete to regenerate",
                    key_path.display(),
                    mode & 0o777,
                );
            }
        }
        std::fs::read(&key_path)?
    } else {
        use rand::TryRngCore;
        use std::io::Write;
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|e| anyhow::anyhow!("OsRng: {e}"))?;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&key_path)?;
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
///
/// Standalone entry — constructs its own `SessionBroadcast`. For
/// integration with a live session's outbound stream call
/// [`teleport_serve_with_broadcast`] and pass the session's shared
/// broadcaster.
pub async fn teleport_serve(
    project_root: PathBuf,
    bind: String,
    label: String,
    insecure: bool,
) -> anyhow::Result<()> {
    teleport_serve_with_broadcast(
        project_root,
        bind,
        label,
        insecure,
        SessionBroadcast::new(),
    )
    .await
}

/// Audit P0.3 — spawn a teleport bridge bound to a caller-supplied
/// `SessionBroadcast` in the background so a live `vac run` /
/// TUI session can route its SubmitChunks out via SSE. Reads the
/// same env knobs as CLI flags (`VAC_TELEPORT_BIND`,
/// `VAC_TELEPORT_LABEL`, `VAC_TELEPORT_INSECURE`) so operators
/// opt in with a single variable. Returns the broadcaster the
/// caller hands to `run_via_session_engine_with_broadcast`.
pub async fn start_live_teleport_bridge(
    project_root: &std::path::Path,
) -> anyhow::Result<Arc<SessionBroadcast>> {
    let bind = std::env::var("VAC_TELEPORT_BIND")
        .unwrap_or_else(|_| "127.0.0.1:9042".into());
    let label = std::env::var("VAC_TELEPORT_LABEL")
        .unwrap_or_else(|_| "live".into());
    let insecure = std::env::var("VAC_TELEPORT_INSECURE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let broadcast = SessionBroadcast::new();
    let broadcast_for_srv = broadcast.clone();
    let project_root_buf = project_root.to_path_buf();
    tokio::spawn(async move {
        if let Err(e) = teleport_serve_with_broadcast(
            project_root_buf,
            bind,
            label,
            insecure,
            broadcast_for_srv,
        )
        .await
        {
            tracing::error!(
                target: "vac_cli::teleport_bridge",
                error = %e,
                "teleport bridge server exited",
            );
        }
    });
    Ok(broadcast)
}

/// B5 — host with a caller-provided `SessionBroadcast`. The caller
/// (typically the vac_cli session wiring that owns the active
/// submit) publishes `OutboundEvent`s via its own `Arc<SessionBroadcast>`;
/// the teleport SSE subscribes and fans to attach clients.
pub async fn teleport_serve_with_broadcast(
    project_root: PathBuf,
    bind: String,
    label: String,
    insecure: bool,
    broadcast: Arc<SessionBroadcast>,
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

    let state = AppState {
        keys: Arc::new(keys),
        events: broadcast.clone(),
    };

    let cfg = RemoteSessionConfig::default();
    let sse_url = format!("http://{addr}/events");
    let input_url = format!("http://{addr}/input");

    // Connection metadata to stdout (pipe-safe). Token + attach
    // command go to stderr so `vac teleport --serve | tee log.txt`
    // doesn't accidentally persist the bearer secret to disk.
    println!("── vac teleport (serve) ──────────────────────");
    // B5: if a caller passes a `SessionBroadcast` already attached
    // to a live `run_via_session_engine`, this host fans the
    // session's real SubmitChunks (text, tool.request, tool.result,
    // finished, aborted) out via SSE. Standalone invocation still
    // works; in that mode the only frames are the startup banner
    // and /input echoes.
    println!("session_id    {session_id}");
    println!("label         {label}");
    println!("bind          {addr}");
    println!("sse_url       {sse_url}");
    println!("input_url     {input_url}");
    println!("default_trust {}", cfg.default_trust.label());
    println!("ttl_seconds   {}", DEFAULT_TELEPORT_TTL.as_secs());
    eprintln!();
    eprintln!("!! SECRET — do not redirect / log / paste in chat !!");
    eprintln!("Bearer token ({}s TTL):", DEFAULT_TELEPORT_TTL.as_secs());
    eprintln!("{token}");
    eprintln!();
    eprintln!("Attach with:");
    eprintln!("  vac teleport --attach {token} --url http://{addr}");
    eprintln!();

    // Emit a startup frame so attach clients see the connection
    // immediately (zero-subscriber publishes are silent no-ops).
    broadcast.publish(OutboundEvent::new(
        "session_started",
        serde_json::json!({ "session_id": session_id, "label": label }),
    ));

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

    // Long-lived client (no request timeout for SSE; per-request
    // timeouts set on posts below).
    let client = reqwest::Client::builder().build()?;

    println!("── vac teleport (attach) ─────────────────────");
    println!("sse_url   {sse_url}");
    println!("input_url {input_url}");
    println!("(stdin lines posted as JSON to /input; ctrl-C to exit)");
    println!();

    // Stdin pump — tracked so the SSE loop can abort it on exit.
    let stdin_client = client.clone();
    let stdin_url = input_url.clone();
    let stdin_token = token.clone();
    let stdin_handle = tokio::spawn(async move {
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
                .timeout(Duration::from_secs(30))
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
    // Guard: abort the stdin pump when this function returns (SSE
    // stream closed / error path / ctrl-C). Without this the
    // spawned task lingers past the CLI exit and would silently
    // post to a dead server on the next stdin line.
    struct AbortOnDrop(tokio::task::JoinHandle<()>);
    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _stdin_guard = AbortOnDrop(stdin_handle);

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
            // Collect multi-line `data:` per SSE spec §9.2.6:
            // multiple `data:` lines concatenate with '\n'.
            let mut data_lines: Vec<&str> = Vec::new();
            for raw in frame.lines() {
                let payload = raw
                    .strip_prefix("data: ")
                    .or_else(|| raw.strip_prefix("data:"));
                if let Some(p) = payload {
                    data_lines.push(p);
                }
                // lines starting with ':' are SSE comments / heartbeats — silent.
            }
            if !data_lines.is_empty() {
                println!("{}", data_lines.join("\n"));
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
        let bc = SessionBroadcast::new();
        let state = AppState {
            keys: Arc::new(keys),
            events: bc.clone(),
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

    /// B5 — caller publishes on a shared `SessionBroadcast`, SSE
    /// subscriber receives the frame. This is the integration
    /// contract between `run_via_session_engine_with_broadcast`
    /// and `teleport_serve_with_broadcast`.
    #[tokio::test]
    async fn live_bridge_publishes_outbound_to_sse_attach() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().to_path_buf();
        let (keys, kid) = load_or_create_keyset(&project).unwrap();
        let token = issue_teleport_token(
            &keys,
            &kid,
            "session-live",
            "live",
            DEFAULT_TELEPORT_TTL,
        )
        .unwrap();
        let bc = SessionBroadcast::new();
        let state = AppState {
            keys: Arc::new(keys),
            events: bc.clone(),
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
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let mut body = resp.bytes_stream();

        tokio::time::sleep(Duration::from_millis(50)).await;
        // Caller (e.g. run_via_session_engine) publishes a live
        // SubmitChunk-derived event.
        bc.publish(OutboundEvent::new(
            "text",
            serde_json::json!({ "text": "live session hello" }),
        ));

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
            if buf.contains("live session hello") {
                break;
            }
        }
        assert!(
            buf.contains("live session hello"),
            "expected broadcast frame on SSE, got: {buf}",
        );
    }

    #[tokio::test]
    async fn sse_rejects_missing_bearer() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().to_path_buf();
        let (keys, _kid) = load_or_create_keyset(&project).unwrap();
        let state = AppState {
            keys: Arc::new(keys),
            events: SessionBroadcast::new(),
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
        let state = AppState {
            keys: Arc::new(keys),
            events: SessionBroadcast::new(),
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
