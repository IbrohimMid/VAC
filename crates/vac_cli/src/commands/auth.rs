//! `vac auth` — interactive provider onboarding.
//!
//! Ships two flows:
//! - `login` (legacy): operator pastes an API key, saved via
//!   `vac_core::auth::save_kilo_api_key`.
//! - `oauth` (G2): PKCE browser redirect with a loopback callback;
//!   the result is stored via `vac_bridge::auth::TokenCache` for
//!   later consumers.

use crate::AuthAction;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{Duration, timeout};

use vac_bridge::auth::oauth::{PkceChallenge, ProviderToken, TokenCache};

pub async fn execute(action: AuthAction) -> anyhow::Result<()> {
    match action {
        AuthAction::Login { token } => login(token).await,
        AuthAction::Oauth {
            provider,
            client_id,
            auth_url,
            token_url,
        } => login_oauth(provider, client_id, auth_url, token_url).await,
        AuthAction::Status => status().await,
        AuthAction::Logout => logout().await,
    }
}

async fn login(token: Option<String>) -> anyhow::Result<()> {
    println!("VAC Auth Setup");
    println!("==============");

    let provider = select_provider().await?;
    println!();

    let token = match token {
        Some(t) => t,
        None => prompt_token(&provider).await?,
    };

    let path = vac_core::auth::save_kilo_api_key(&token)?;

    println!();
    println!("✓ Auth saved");
    println!("  Provider : {}", provider.display_name());
    println!("  File     : {}", path.display());
    println!("  Mode     : 0600 (user-only)");
    Ok(())
}

async fn status() -> anyhow::Result<()> {
    let s = vac_core::auth::auth_status()?;
    println!("VAC Auth Status");
    println!("  storage  : {}", s.config_path.display());
    println!(
        "  env key  : {}",
        if s.env_present { "✓ present" } else { "✗ missing" }
    );
    println!(
        "  saved key: {}",
        if s.stored_present {
            "✓ present"
        } else {
            "✗ missing"
        }
    );
    println!(
        "  effective: {}",
        if s.effective_present {
            "✓ ready"
        } else {
            "✗ not configured"
        }
    );
    if let Some(t) = s.updated_at {
        println!("  saved at : {t}");
    }
    if !s.effective_present {
        println!();
        println!("Run `vac auth login` to configure.");
    }
    Ok(())
}

async fn logout() -> anyhow::Result<()> {
    let path = vac_core::auth::clear_auth()?;
    println!("✓ Auth removed: {}", path.display());
    Ok(())
}

// ── Provider selection (legacy login) ─────────────────────────────

#[derive(Debug, Clone, Copy)]
enum Provider {
    KiloGateway,
    Anthropic,
    OpenAI,
}

impl Provider {
    fn display_name(&self) -> &'static str {
        match self {
            Self::KiloGateway => "Kilo Gateway (recommended)",
            Self::Anthropic => "Anthropic (direct)",
            Self::OpenAI => "OpenAI (direct)",
        }
    }

    fn key_hint(&self) -> &'static str {
        match self {
            Self::KiloGateway => "Kilo API key (from kilo.ai dashboard)",
            Self::Anthropic => "Anthropic API key (sk-ant-...)",
            Self::OpenAI => "OpenAI API key (sk-...)",
        }
    }
}

async fn select_provider() -> anyhow::Result<Provider> {
    let providers = [
        (1, Provider::KiloGateway),
        (2, Provider::Anthropic),
        (3, Provider::OpenAI),
    ];
    println!("Select provider:");
    for (n, p) in &providers {
        println!("  [{n}] {}", p.display_name());
    }
    print!("Choice [1]: ");
    io::stdout().flush()?;

    let input = crate::io::read_line_async().await?;
    let choice = input.trim();
    Ok(match choice {
        "2" => Provider::Anthropic,
        "3" => Provider::OpenAI,
        _ => Provider::KiloGateway,
    })
}

async fn prompt_token(provider: &Provider) -> anyhow::Result<String> {
    print!("Paste {} → ", provider.key_hint());
    io::stdout().flush()?;
    let token = crate::io::read_secret_async().await?;
    if token.is_empty() {
        anyhow::bail!("No API key provided");
    }
    Ok(token)
}

// ── G2 — PKCE OAuth login ─────────────────────────────────────────

/// Registry of known OAuth providers. Leave endpoint URLs `None`
/// when the provider does NOT have first-class OAuth for VAC's use
/// case — the command then errors out cleanly instead of making up
/// URLs.  Operators with a private provider can always pass
/// `--auth-url` / `--token-url` / `--client-id`.
#[derive(Debug, Clone, Copy)]
struct ProviderSpec {
    key: &'static str,
    client_id: Option<&'static str>,
    auth_url: Option<&'static str>,
    token_url: Option<&'static str>,
    scope: Option<&'static str>,
}

const PROVIDER_REGISTRY: &[ProviderSpec] = &[
    // Anthropic and OpenAI currently issue API keys (not OAuth
    // tokens) for programmatic use. Ship the provider slots as
    // placeholders so `vac auth oauth anthropic` surfaces a clear
    // error pointing operators at `--auth-url` / `--token-url` /
    // `--client-id` overrides rather than an invented endpoint.
    ProviderSpec {
        key: "anthropic",
        client_id: None,
        auth_url: None,
        token_url: None,
        scope: None,
    },
    ProviderSpec {
        key: "openai",
        client_id: None,
        auth_url: None,
        token_url: None,
        scope: None,
    },
];

fn lookup_provider(key: &str) -> Option<&'static ProviderSpec> {
    PROVIDER_REGISTRY.iter().find(|s| s.key == key)
}

async fn login_oauth(
    provider: String,
    client_id_override: Option<String>,
    auth_url_override: Option<String>,
    token_url_override: Option<String>,
) -> anyhow::Result<()> {
    let spec = lookup_provider(&provider).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown provider '{provider}' — registered: {}",
            PROVIDER_REGISTRY
                .iter()
                .map(|s| s.key)
                .collect::<Vec<_>>()
                .join(", "),
        )
    })?;

    let client_id = client_id_override
        .or_else(|| spec.client_id.map(String::from))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "provider '{}' has no registered OAuth client_id; pass --client-id",
                spec.key,
            )
        })?;
    let auth_url_base = auth_url_override
        .or_else(|| spec.auth_url.map(String::from))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "provider '{}' has no registered authorize URL; pass --auth-url",
                spec.key,
            )
        })?;
    let token_url = token_url_override
        .or_else(|| spec.token_url.map(String::from))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "provider '{}' has no registered token URL; pass --token-url",
                spec.key,
            )
        })?;

    let seed: [u8; 32] = rand::random();
    let pkce = PkceChallenge::generate(&seed);

    let listener =
        TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let mut auth_url = url::Url::parse(&auth_url_base)?;
    {
        let mut q = auth_url.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair("client_id", &client_id);
        q.append_pair("redirect_uri", &redirect_uri);
        q.append_pair("code_challenge", &pkce.challenge);
        q.append_pair("code_challenge_method", "S256");
        if let Some(scope) = spec.scope {
            q.append_pair("scope", scope);
        }
    }

    let auth_url_str = auth_url.to_string();
    println!("Opening browser for {} authorization …", spec.key);
    if let Err(e) = open::that(&auth_url_str) {
        tracing::warn!(error = %e, "open::that failed");
    }
    println!("If the browser did not open, visit:\n  {auth_url_str}");
    println!("Waiting for callback on {redirect_uri} …");

    let code = wait_for_callback(listener).await?;

    // Token exchange.
    let client = reqwest::Client::new();
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("client_id", client_id.as_str()),
        ("code_verifier", pkce.verifier.as_str()),
    ];
    let resp = client.post(&token_url).form(&form).send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("token endpoint returned {status}: {body}");
    }
    let parsed: TokenResponse = serde_json::from_str(&body)
        .map_err(|e| anyhow::anyhow!("token response parse: {e}; body={body}"))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let token = ProviderToken {
        provider: spec.key.to_string(),
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        expires_at_unix: now + parsed.expires_in.unwrap_or(3600),
        scope: parsed.scope,
    };

    let cache = TokenCache::new(TokenCache::default_root());
    cache.save(&token).await?;

    println!(
        "✓ OAuth token stored for '{}' (expires at unix={})",
        spec.key, token.expires_at_unix,
    );
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

/// Accept one TCP connection, parse the HTTP/1 request line, extract
/// the `code=` query value, reply with a human-readable page.
async fn wait_for_callback(listener: TcpListener) -> anyhow::Result<String> {
    let (mut socket, _peer) = timeout(Duration::from_secs(300), listener.accept())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for OAuth callback"))??;
    let mut buf = [0u8; 4096];
    let n = socket.read(&mut buf).await?;
    let raw = String::from_utf8_lossy(&buf[..n]);
    let code = parse_code_from_request(&raw).ok_or_else(|| {
        anyhow::anyhow!("callback request had no ?code= parameter")
    })?;

    let body = "You may close this tab and return to the terminal.\n";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    );
    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    Ok(code)
}

/// Extract `code=` from the first line of an HTTP request. Pulled
/// out for unit-testability — no network required.
pub(crate) fn parse_code_from_request(raw: &str) -> Option<String> {
    let first_line = raw.lines().next()?;
    // Expect: "GET /callback?code=abc&state=... HTTP/1.1"
    let path_and_query = first_line.split_whitespace().nth(1)?;
    let query = path_and_query.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("code=") {
            let decoded = url::form_urlencoded::parse(pair.as_bytes())
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_else(|| value.to_string());
            return Some(decoded);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_code_from_well_formed_request() {
        let raw = "GET /callback?code=abc123&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        assert_eq!(parse_code_from_request(raw).as_deref(), Some("abc123"));
    }

    #[test]
    fn parse_code_handles_url_encoded_value() {
        let raw = "GET /callback?code=a%2Bb%2Fc HTTP/1.1\r\n";
        assert_eq!(parse_code_from_request(raw).as_deref(), Some("a+b/c"));
    }

    #[test]
    fn parse_code_missing_returns_none() {
        let raw = "GET /callback?state=xyz HTTP/1.1\r\n";
        assert!(parse_code_from_request(raw).is_none());
    }

    #[test]
    fn parse_code_malformed_returns_none() {
        assert!(parse_code_from_request("").is_none());
        assert!(parse_code_from_request("garbage").is_none());
    }
}
