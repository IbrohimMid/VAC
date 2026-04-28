//! D.1 — `WebFetchTool` + D.2 — `WebSearchTool` primitives.
//!
//! Transport-independent fetch + search pipelines that the
//! vac_tools tool-registry wraps as LLM-callable tools. Both
//! funnel through:
//!
//! - a response-size cap (2 MB default) that spills larger bodies
//!   to `.vac/tool-results/<id>.bin` so NotifyRouter's activity
//!   lane doesn't drown in a 50 MB HTML dump;
//! - a header allowlist + User-Agent normalisation so the hook
//!   layer's audit trail reads uniform across callers;
//! - a configurable timeout (30 s default).
//!
//! TrustGate integration happens at the tool-registry layer —
//! this module is transport/policy-agnostic (it neither knows nor
//! cares about `Isolated` vs `Host` modes). The tool wrapper
//! passes `Isolated` by default and upgrades on explicit operator
//! opt-in.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, EngineResult};

/// Response-size cap. Bodies larger spill to disk; the tool
/// result carries a `PreviewStub`-shape pointer.
pub const DEFAULT_RESPONSE_CAP: usize = 2 * 1024 * 1024;

/// Default request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Input to WebFetch. Narrow shape so model-supplied JSON
/// deserialises straightforwardly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchRequest {
    pub url: String,
    /// Optional HTTP method. Default GET. POST bodies ship in
    /// `body` (JSON serialised).
    #[serde(default)]
    pub method: Option<String>,
    /// Optional JSON body (POST only).
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    /// Header overrides (subject to the allowlist below).
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
}

/// Result the tool wrapper hands back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchResult {
    pub status: u16,
    pub final_url: String,
    pub bytes_read: usize,
    pub truncated: bool,
    /// When the body fits under the cap, this is the full body as
    /// UTF-8. Larger bodies → `spill_path` is set and body is a
    /// short summary stub.
    pub body: String,
    pub spill_path: Option<PathBuf>,
    pub duration_ms: u64,
}

/// Request-header keys the WebFetch tool sends through. Anything
/// not on this list is dropped to keep the audit trail clean.
pub const REQUEST_HEADER_ALLOWLIST: &[&str] = &[
    "Accept",
    "Accept-Language",
    "Authorization",
    "Content-Type",
    "User-Agent",
];

/// Synchronous URL fetch. Honors the response cap, spills oversize
/// bodies to `spill_dir`.
pub async fn fetch(
    req: &WebFetchRequest,
    spill_dir: &std::path::Path,
    cap_bytes: usize,
) -> EngineResult<WebFetchResult> {
    let started = std::time::Instant::now();
    let method = req
        .method
        .clone()
        .unwrap_or_else(|| "GET".into())
        .to_uppercase();
    let method_parsed: reqwest::Method = method
        .parse()
        .map_err(|_| EngineError::Other(format!("invalid http method '{method}'")))?;

    let client = reqwest::Client::builder()
        .timeout(DEFAULT_TIMEOUT)
        .user_agent(concat!("vac/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| EngineError::Other(format!("http client build: {e}")))?;

    let mut builder = client.request(method_parsed, &req.url);
    for (k, v) in &req.headers {
        if REQUEST_HEADER_ALLOWLIST
            .iter()
            .any(|a| a.eq_ignore_ascii_case(k))
        {
            builder = builder.header(k, v);
        } else {
            tracing::info!(
                target: "vac_tui_runtime::web",
                header = %k,
                "web fetch: dropping header not on allowlist",
            );
        }
    }
    // Audit fix: never include header values in info/debug
    // traces. The allowlist pass above logs only the key name,
    // never the value — but any future trace that wants to
    // sanity-log the full header map should route through this
    // helper.
    #[allow(dead_code)]
    fn _redact_sensitive(key: &str, _value: &str) -> &'static str {
        match key.to_ascii_lowercase().as_str() {
            "authorization" | "cookie" | "proxy-authorization" => "<redacted>",
            _ => "<value>",
        }
    }
    if let Some(body) = &req.body {
        builder = builder.json(body);
    }

    let resp = builder
        .send()
        .await
        .map_err(|e| EngineError::Other(format!("web fetch send: {e}")))?;

    let status = resp.status().as_u16();
    let final_url = resp.url().to_string();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| EngineError::Other(format!("web fetch read: {e}")))?;
    let len = bytes.len();

    let (body, truncated, spill_path) = if len > cap_bytes {
        tokio::fs::create_dir_all(spill_dir).await?;
        let id = uuid::Uuid::new_v4();
        let path = spill_dir.join(format!("web-fetch-{id}.bin"));
        tokio::fs::write(&path, &bytes).await?;
        (
            format!(
                "[body {} bytes > cap {} — spilled to {}]",
                len,
                cap_bytes,
                path.display(),
            ),
            true,
            Some(path),
        )
    } else {
        (String::from_utf8_lossy(&bytes).into_owned(), false, None)
    };

    Ok(WebFetchResult {
        status,
        final_url,
        bytes_read: len,
        truncated,
        body,
        spill_path,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

// ── D.2 WebSearch ───────────────────────────────────────────────

/// Input to WebSearch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchRequest {
    pub query: String,
    /// Backend identifier (e.g. `"brave"`, `"google"`).
    pub backend: String,
    /// Max result count.
    #[serde(default = "default_search_count")]
    pub count: u8,
}

fn default_search_count() -> u8 {
    10
}

/// Single result row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Pluggable backend contract. Implementors live in the driver
/// crate (so vac_session_engine stays provider-free); the
/// built-in `BraveBackend` wraps the Brave Search API.
#[async_trait::async_trait]
pub trait SearchBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn search(&self, req: &WebSearchRequest) -> EngineResult<Vec<WebSearchHit>>;
}

/// Brave Search HTTP API wrapper. API key supplied at
/// construction from env `VAC_BRAVE_API_KEY` or
/// `~/.vac/auth/brave.json`.
pub struct BraveBackend {
    api_key: String,
}

impl BraveBackend {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }
}

#[async_trait::async_trait]
impl SearchBackend for BraveBackend {
    fn name(&self) -> &str {
        "brave"
    }

    async fn search(&self, req: &WebSearchRequest) -> EngineResult<Vec<WebSearchHit>> {
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent(concat!("vac/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| EngineError::Other(format!("brave client build: {e}")))?;
        let resp = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("q", req.query.as_str()), ("count", &req.count.to_string())])
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("brave send: {e}")))?;
        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "brave status {}",
                resp.status()
            )));
        }
        #[derive(Deserialize)]
        struct BraveResponse {
            web: Option<BraveWeb>,
        }
        #[derive(Deserialize)]
        struct BraveWeb {
            results: Vec<BraveHit>,
        }
        #[derive(Deserialize)]
        struct BraveHit {
            title: String,
            url: String,
            description: Option<String>,
        }
        let parsed: BraveResponse = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("brave parse: {e}")))?;
        Ok(parsed
            .web
            .map(|w| w.results)
            .unwrap_or_default()
            .into_iter()
            .map(|h| WebSearchHit {
                title: h.title,
                url: h.url,
                snippet: h.description.unwrap_or_default(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_is_non_empty_and_uppercase_stable() {
        assert!(!REQUEST_HEADER_ALLOWLIST.is_empty());
        // Quick spot-check: headers operators actually need are
        // present.
        for h in ["Authorization", "Content-Type", "Accept"] {
            assert!(REQUEST_HEADER_ALLOWLIST.contains(&h));
        }
    }

    #[test]
    fn default_response_cap_is_2mb() {
        assert_eq!(DEFAULT_RESPONSE_CAP, 2 * 1024 * 1024);
    }

    #[test]
    fn brave_backend_name_matches_enum_label() {
        let b = BraveBackend::new("dummy");
        assert_eq!(b.name(), "brave");
    }

    #[tokio::test]
    async fn fetch_rejects_invalid_method() {
        let tmp = tempfile::tempdir().unwrap();
        let req = WebFetchRequest {
            url: "http://localhost:1/irrelevant".into(),
            method: Some("👻".into()),
            body: None,
            headers: Default::default(),
        };
        let err = fetch(&req, tmp.path(), DEFAULT_RESPONSE_CAP)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("invalid http method"), "{err}");
    }
}
