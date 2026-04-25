//! D7A — host-side `VacConfig` → `.vac/model_config.json` probe.
//!
//! This crate is the **first** ADR-sanctioned exception in the
//! shell stack that depends on `vac_core` directly. Every other
//! shell crate (UI widget, bridge, host-state) is forbidden from
//! that dependency by the D-track ADR. The probe lives outside
//! that boundary because its job is precisely to translate the
//! engine's authoritative config into the neutral on-disk
//! snapshot the rest of the shell stack consumes.
//!
//! # Boundary discipline
//!
//! * **Producer**: this crate. Reads `vac_core::VacConfig`,
//!   writes JSON.
//! * **Consumer**: `vac_shell_host_vac_config`. Parses JSON,
//!   exposes `VacConfigModelSource`. Never depends on this crate.
//! * No UI / widget / bridge / app crate may depend on this
//!   crate. The dogfood example may invoke it; the entrypoint
//!   library does not.
//!
//! # Allowlist-shaped projection (no secrets cross the seam)
//!
//! The on-disk schema is **closed**: only `providers[]`,
//! `models[]`, and an optional `active` are written. We never
//! flatten or pass through `ProviderConfig`, so a future field
//! added there cannot leak by accident. Specifically excluded:
//!
//! * `api_key` values (resolved via env, never read into the
//!   snapshot).
//! * `api_key_env` *names* (env-var taxonomy can itself be a
//!   sensitive operator hint on shared hosts).
//! * `base_url` (may carry credentials in a `user:pw@host` URL).
//! * `max_tokens`, `temperature`, routing, fallback chain — out
//!   of scope for the model picker.
//!
//! Only the boolean `credentials_present` per provider crosses
//! the seam, computed from a pluggable [`EnvPresence`] (default:
//! `std::env`).
//!
//! # Atomic write
//!
//! [`write_snapshot`] writes to a unique temp path
//! (`<file>.tmp.<pid>.<nanos>`), `sync_all`s, then renames over
//! the destination so a partial write can never replace a valid
//! snapshot. The temp file is best-effort cleaned on error.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vac_core::VacConfig;
use vac_shell_contracts::VacPaths;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("probe io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("probe serialize error: {0}")]
    Serialize(#[from] serde_json::Error),
}

// =====================================================================
// Snapshot DTO (mirrors vac_shell_host_vac_config::DiskSnapshot)
// =====================================================================
//
// Duplicated by design — the file format is the contract, not a
// shared Rust type. A round-trip test pins the schema parity.

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDoc {
    pub providers: Vec<SnapshotProvider>,
    pub models: Vec<SnapshotModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<SnapshotActive>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotProvider {
    pub id: String,
    pub credentials_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotModel {
    pub provider: String,
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotActive {
    pub provider: String,
    pub id: String,
}

// =====================================================================
// EnvPresence — pluggable creds detector (test-friendly)
// =====================================================================

/// Strategy for deciding whether a provider's API key is
/// available. Keeps the snapshot deterministic in tests and
/// avoids `std::env::var` calls inside the projection logic.
pub trait EnvPresence: Send + Sync {
    /// Return `true` when the env var named `name` is set to a
    /// non-empty (after-trim) value in the underlying
    /// environment.
    fn present_non_empty(&self, name: &str) -> bool;
}

/// Default impl backed by `std::env::var`. Mirrors the
/// canonical `vil_llm::LlmConfig::provider_ready` env check —
/// any unset / unreadable / empty / whitespace value is
/// treated as not present.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessEnvPresence;

impl EnvPresence for ProcessEnvPresence {
    fn present_non_empty(&self, name: &str) -> bool {
        match std::env::var(name) {
            Ok(v) => !v.trim().is_empty(),
            Err(_) => false,
        }
    }
}

/// Backwards-compatible alias for [`ProcessEnvPresence`]. Retained
/// so code written before the D7A hardening rename keeps
/// compiling. New code should prefer `ProcessEnvPresence`.
pub type StdEnvPresence = ProcessEnvPresence;

// =====================================================================
// Projection
// =====================================================================

/// Build a snapshot from `config` using the default
/// [`ProcessEnvPresence`] strategy.
pub fn build_snapshot(config: &VacConfig) -> SnapshotDoc {
    build_snapshot_with_env(config, &ProcessEnvPresence)
}

/// Build a snapshot from `config` and an explicit
/// [`EnvPresence`]. Output is deterministic — providers and
/// models are sorted by `(provider_id, model_id)` so two runs
/// against the same input produce byte-identical JSON.
pub fn build_snapshot_with_env(config: &VacConfig, env: &dyn EnvPresence) -> SnapshotDoc {
    // Stable iteration order — VacConfig.llm.providers is a
    // HashMap, so we sort by provider id before projecting.
    let mut provider_ids: Vec<&String> = config.llm.providers.keys().collect();
    provider_ids.sort();

    let mut providers = Vec::with_capacity(provider_ids.len());
    let mut models = Vec::with_capacity(provider_ids.len());

    for pid in &provider_ids {
        let provider_cfg = match config.llm.providers.get(*pid) {
            Some(p) => p,
            None => continue,
        };

        // Allowlist-shaped projection: only the boolean.
        // Mirrors `vil_llm::LlmConfig::provider_ready` —
        // providers without `api_key_env` are local / no-key
        // and treated as ready when the provider entry exists.
        let credentials_present = match provider_cfg.api_key_env.as_deref() {
            Some(name) if !name.trim().is_empty() => env.present_non_empty(name),
            Some(_) => false, // `api_key_env = ""` is malformed config, not "no key needed".
            None => true,
        };
        providers.push(SnapshotProvider {
            id: (*pid).clone(),
            credentials_present,
        });

        // One model per provider — VacConfig today exposes a
        // single `model: Option<String>` per provider; later
        // slices can extend the registry without touching the
        // file format (additive `models[]`).
        if let Some(model_id) = provider_cfg
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            models.push(SnapshotModel {
                provider: (*pid).clone(),
                id: model_id.to_string(),
                label: model_id.to_string(),
                reasoning: false,
                cost_label: None,
            });
        }
    }

    // Sort models by (provider, id) — deterministic regardless
    // of provider iteration order above.
    models.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then_with(|| a.id.cmp(&b.id))
    });

    let active = build_active(config);

    SnapshotDoc {
        providers,
        models,
        active,
    }
}

fn build_active(config: &VacConfig) -> Option<SnapshotActive> {
    let provider_id = config.llm.default_provider.trim();
    if provider_id.is_empty() {
        return None;
    }
    let provider_cfg = config.llm.providers.get(provider_id)?;
    let model_id = provider_cfg
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    Some(SnapshotActive {
        provider: provider_id.to_string(),
        id: model_id.to_string(),
    })
}

// =====================================================================
// Atomic write
// =====================================================================

/// Write `config`'s projection to `paths.model_config_file()`.
/// Default env strategy; see [`write_snapshot_with_env`] for a
/// test override.
pub fn write_snapshot(config: &VacConfig, paths: &dyn VacPaths) -> Result<PathBuf, ProbeError> {
    write_snapshot_with_env(config, paths, &ProcessEnvPresence)
}

/// Same as [`write_snapshot`] using an explicit
/// [`ProcessEnvPresence`]-equivalent strategy.
pub fn write_snapshot_with_env(
    config: &VacConfig,
    paths: &dyn VacPaths,
    env: &dyn EnvPresence,
) -> Result<PathBuf, ProbeError> {
    let snapshot = build_snapshot_with_env(config, env);
    write_snapshot_doc(&snapshot, paths)
}

/// Write a pre-built snapshot to `paths.model_config_file()`
/// atomically. Writes via a unique temp path → fsync → rename;
/// the existing file is replaced only after the new bytes are on
/// disk.
pub fn write_snapshot_doc(
    snapshot: &SnapshotDoc,
    paths: &dyn VacPaths,
) -> Result<PathBuf, ProbeError> {
    let dest = paths.model_config_file();
    write_to_path(snapshot, &dest)?;
    Ok(dest)
}

fn write_to_path(snapshot: &SnapshotDoc, dest: &Path) -> Result<(), ProbeError> {
    use std::fs;
    use std::io::Write;

    let parent = match dest.parent() {
        Some(p) if !p.as_os_str().is_empty() => Some(p),
        _ => None,
    };
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|e| ProbeError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }

    let bytes = serde_json::to_vec_pretty(snapshot)?;

    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut tmp = dest.to_path_buf();
    let stem = dest.file_name().and_then(|s| s.to_str()).unwrap_or("snap");
    tmp.set_file_name(format!("{stem}.tmp.{pid}.{nanos}"));

    let write_then_rename = || -> std::io::Result<()> {
        {
            let mut f = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        atomic_replace(&tmp, dest)?;
        // On Unix, fsync the parent dir so the rename's directory
        // entry is durable even if the box crashes immediately after
        // this returns. No-op on Windows (FS journals the rename).
        if let Some(parent) = parent {
            sync_dir(parent)?;
        }
        Ok(())
    };

    if let Err(e) = write_then_rename() {
        let _ = fs::remove_file(&tmp);
        return Err(ProbeError::Io {
            path: dest.display().to_string(),
            source: e,
        });
    }
    Ok(())
}

/// Cross-platform atomic-ish replace.
///
/// * Unix: `rename(2)` already replaces the destination atomically.
/// * Windows: `std::fs::rename` fails when `dest` exists; we fall
///   back to `remove_file` + `rename`. There is a narrow window
///   where the destination is absent — operators that need a
///   stronger guarantee on Windows should use a vetted crate
///   (e.g. `tempfile::persist`); the on-disk consumer here treats
///   a missing snapshot as "fall back to fixture", so a partial
///   failure on Windows degrades to the documented missing-file
///   path rather than data loss.
#[cfg(unix)]
fn atomic_replace(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::rename(tmp, dest)
}

#[cfg(windows)]
fn atomic_replace(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    match std::fs::rename(tmp, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = std::fs::remove_file(dest);
            std::fs::rename(tmp, dest)
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn atomic_replace(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::rename(tmp, dest)
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    // Best-effort: opening the directory as O_RDONLY and fsyncing
    // it is the documented POSIX way to durably commit a rename.
    match std::fs::File::open(dir) {
        Ok(f) => f.sync_all(),
        Err(e) => Err(e),
    }
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}
