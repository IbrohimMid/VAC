//! Startup helpers — warning classification + environment checks surfaced as
//! banners/toasts when the TUI boots.

use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::InputEvent;

/// Classify an engine init warning into a banner style + severity.
pub(super) fn classify_init_warning(
    warning: &str,
) -> (
    crate::services::banner::BannerStyle,
    crate::services::banner::BannerSeverity,
) {
    let lower = warning.to_ascii_lowercase();
    if lower.contains("failed to connect mcp server") || lower.contains("mcp server") {
        let tls_related = lower.contains("tls")
            || lower.contains("certificate")
            || lower.contains("ca file")
            || lower.contains("mtls")
            || lower.contains("server_name")
            || lower.contains("identity");
        if tls_related {
            (
                crate::services::banner::BannerStyle::Error,
                crate::services::banner::BannerSeverity::Blocking,
            )
        } else {
            (
                crate::services::banner::BannerStyle::Warning,
                crate::services::banner::BannerSeverity::Suggested,
            )
        }
    } else {
        (
            crate::services::banner::BannerStyle::Warning,
            crate::services::banner::BannerSeverity::Suggested,
        )
    }
}

fn read_toml_str(path: &Path, keys: &[&str]) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let table = content.parse::<toml::Table>().ok()?;
    let mut current = &toml::Value::Table(table);
    for key in keys {
        current = current.get(key)?;
    }
    current.as_str().map(String::from)
}

/// Run environment startup checks: verify VIL_KNOWLEDGE_ROOT (env or
/// `.vac/config.toml` `[knowledge].root`) exists, and verify at least one MCP
/// server is configured. Surfaces info toasts when something is missing.
pub(super) async fn run_startup_checks(project_root: &Path, input_tx: &mpsc::Sender<InputEvent>) {
    let corpus_root = if let Ok(env_root) = std::env::var("VIL_KNOWLEDGE_ROOT") {
        let p = PathBuf::from(env_root);
        if p.exists() { Some(p) } else { None }
    } else {
        let config_path = project_root.join(".vac/config.toml");
        read_toml_str(&config_path, &["knowledge", "root"])
            .map(PathBuf::from)
            .filter(|p| p.exists())
    };

    if corpus_root.is_none() {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::info(
                "VIL Knowledge missing. Using bootstrap fallback.".to_string(),
            )))
            .await;
    }

    let project_root_clone = project_root.to_path_buf();
    let config = tokio::task::spawn_blocking(move || {
        vac_core::VacConfig::load_with_fallback(&project_root_clone).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    if config.mcp_servers.as_ref().is_none_or(|s| s.is_empty()) {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::info(
                "No MCP servers configured.".to_string(),
            )))
            .await;
    }
}
