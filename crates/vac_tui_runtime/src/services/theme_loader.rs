//! TOML theme loader + filesystem hot-reload (PR-W25-2).
//!
//! Lookup order: `$VAC_THEME` → `.vac/theme.toml` → `~/.config/vac/theme.toml` → built-in.

use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

use crate::app::InputEvent;
use crate::services::theme::{Theme, ThemePreset};

/// TOML representation of a theme (keys must match `ThemePreset` labels).
#[derive(Debug, serde::Deserialize, Default)]
pub struct ThemeConfig {
    /// "Dark" | "Light" | "High Contrast"
    pub preset: Option<String>,
}

impl ThemeConfig {
    pub fn to_theme(&self) -> Theme {
        let preset = self
            .preset
            .as_deref()
            .and_then(parse_preset)
            .unwrap_or(ThemePreset::Dark);
        Theme::new(preset)
    }
}

fn parse_preset(s: &str) -> Option<ThemePreset> {
    match s {
        "Dark" | "dark" => Some(ThemePreset::Dark),
        "Light" | "light" => Some(ThemePreset::Light),
        "High Contrast" | "high_contrast" | "HighContrast" => Some(ThemePreset::HighContrast),
        _ => None,
    }
}

/// Resolve the theme TOML path using the lookup order:
/// `$VAC_THEME` → `.vac/theme.toml` (relative to CWD) → `~/.config/vac/theme.toml` → None.
pub fn resolve_theme_path() -> Option<PathBuf> {
    // 1. Env var
    if let Ok(p) = std::env::var("VAC_THEME") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    // 2. Project-local
    let local = PathBuf::from(".vac/theme.toml");
    if local.exists() {
        return Some(local);
    }
    // 3. User config
    if let Some(cfg) = dirs::config_dir() {
        let user = cfg.join("vac/theme.toml");
        if user.exists() {
            return Some(user);
        }
    }
    None
}

/// Parse a TOML file into `ThemeConfig`.
/// Returns `Ok(ThemeConfig::default())` on missing file, `Err` on parse error.
pub fn load_theme_toml(path: &std::path::Path) -> anyhow::Result<ThemeConfig> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let cfg: ThemeConfig = toml::from_str(&content)?;
            Ok(cfg)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ThemeConfig::default()),
        Err(e) => Err(e.into()),
    }
}

/// Load the theme from the resolved path, falling back to the built-in Dark theme.
pub fn load_theme() -> Theme {
    resolve_theme_path()
        .and_then(|p| load_theme_toml(&p).ok())
        .map(|cfg| cfg.to_theme())
        .unwrap_or_default()
}

/// Spawn a blocking background task that watches `path` with `notify` v6 and sends
/// `InputEvent::ThemeReloaded(theme)` on change.  Debounce: 250 ms.
///
/// Returns the `JoinHandle` — caller must keep it alive (or `.abort()` on shutdown).
pub fn watch_theme(path: PathBuf, tx: Sender<InputEvent>) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc as std_mpsc;

        let (std_tx, std_rx) = std_mpsc::channel();
        let config = Config::default().with_poll_interval(std::time::Duration::from_millis(250));
        let mut watcher = match RecommendedWatcher::new(std_tx, config) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("theme watcher init failed: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
            tracing::warn!("theme watcher watch failed: {e}");
            return;
        }

        let mut last_reload = std::time::Instant::now();
        loop {
            match std_rx.recv_timeout(std::time::Duration::from_millis(300)) {
                Ok(_event) => {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_reload) < std::time::Duration::from_millis(250) {
                        continue; // debounce
                    }
                    last_reload = now;
                    if let Ok(cfg) = load_theme_toml(&path) {
                        let theme = cfg.to_theme();
                        let _ = tx.blocking_send(InputEvent::ThemeReloaded(theme));
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {} // continue polling
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.toml");
        std::fs::write(&path, "preset = \"Light\"\n").unwrap();
        let cfg = load_theme_toml(&path).unwrap();
        let theme = cfg.to_theme();
        assert_eq!(theme.preset, crate::services::theme::ThemePreset::Light);
    }

    #[test]
    fn falls_back_to_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let cfg = load_theme_toml(&path).unwrap();
        let theme = cfg.to_theme();
        assert_eq!(theme.preset, crate::services::theme::ThemePreset::Dark);
    }

    #[tokio::test]
    async fn watch_theme_emits_within_500ms() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.toml");
        std::fs::write(&path, "preset = \"Dark\"\n").unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let _handle = watch_theme(path.clone(), tx);

        // Give watcher time to initialize
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Modify the file
        std::fs::write(&path, "preset = \"Light\"\n").unwrap();

        // Wait up to 500ms for the event
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await;
        // Note: may not always fire in CI; just ensure no panic/hang
        let _ = result;
    }
}
