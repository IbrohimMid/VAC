//! Persistent auth helpers for VAC CLI.

use crate::error::{VacError, VacResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

const APP_DIR: &str = "vac";
const AUTH_FILE: &str = "auth.toml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredAuth {
    pub kilo_api_key: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct AuthStatus {
    pub config_path: PathBuf,
    pub env_present: bool,
    pub stored_present: bool,
    pub effective_present: bool,
    pub updated_at: Option<DateTime<Utc>>,
}

fn config_dir() -> VacResult<PathBuf> {
    dirs_next::config_dir()
        .map(|dir| dir.join(APP_DIR))
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".config").join(APP_DIR))
        })
        .ok_or_else(|| {
            VacError::Config("Could not determine config directory for VAC auth".to_string())
        })
}

pub fn auth_file_path() -> VacResult<PathBuf> {
    Ok(config_dir()?.join(AUTH_FILE))
}

pub fn load_stored_auth() -> VacResult<Option<StoredAuth>> {
    let path = auth_file_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| VacError::Config(format!("Failed to read auth file: {e}")))?;
    let auth: StoredAuth = toml::from_str(&content)
        .map_err(|e| VacError::Config(format!("Failed to parse auth file: {e}")))?;
    Ok(Some(auth))
}

pub fn save_kilo_api_key(api_key: &str) -> VacResult<PathBuf> {
    let token = api_key.trim();
    if token.is_empty() {
        return Err(VacError::Config("Kilo API key cannot be empty".to_string()));
    }

    let path = auth_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let auth = StoredAuth {
        kilo_api_key: Some(token.to_string()),
        updated_at: Some(Utc::now()),
    };
    let serialized = toml::to_string_pretty(&auth)
        .map_err(|e| VacError::Config(format!("Failed to serialize auth file: {e}")))?;

    let mut file = std::fs::File::create(&path)?;
    file.write_all(serialized.as_bytes())?;
    set_user_only_permissions(&path)?;
    Ok(path)
}

pub fn clear_auth() -> VacResult<PathBuf> {
    let path = auth_file_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(path)
}

pub fn resolve_kilo_api_key() -> VacResult<Option<String>> {
    for env_name in ["KILO_API_KEY", "ANTHROPIC_API_KEY"] {
        if let Ok(value) = std::env::var(env_name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed.to_string()));
            }
        }
    }

    Ok(load_stored_auth()?
        .and_then(|auth| auth.kilo_api_key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

pub fn auth_status() -> VacResult<AuthStatus> {
    let path = auth_file_path()?;
    let env_present = std::env::var("KILO_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        || std::env::var("ANTHROPIC_API_KEY")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
    let stored = load_stored_auth()?;
    let stored_present = stored
        .as_ref()
        .and_then(|auth| auth.kilo_api_key.as_ref())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    Ok(AuthStatus {
        config_path: path,
        env_present,
        stored_present,
        effective_present: env_present || stored_present,
        updated_at: stored.and_then(|auth| auth.updated_at),
    })
}

#[cfg(unix)]
fn set_user_only_permissions(path: &std::path::Path) -> VacResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_user_only_permissions(_path: &std::path::Path) -> VacResult<()> {
    Ok(())
}
