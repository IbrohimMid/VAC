//! D3 — read-only VAC model-config seam.
//!
//! The shell stack needs a way to discover what providers and
//! models a project has wired without coupling to the live VAC
//! engine config types in this slice. The seam:
//!
//! ```text
//! VacPaths::model_config_file()
//!   → JSON snapshot on disk
//!   → VacConfigModelSource (parsed)
//!   → ProviderInfo / HostModel
//!   → ModelSource impl
//!   → ShellComposition / build_switcher_view
//! ```
//!
//! # Hard rules (D-track ADR)
//!
//! * `credentials_present: bool` ONLY — the snapshot never
//!   contains an API key, OAuth token, secret path, or
//!   credential filename.
//! * Read-only — no writer here. Provider switching, key writes,
//!   and config persistence remain VAC-engine concerns.
//! * No `vac_core` / `vac_session_engine` dep — the snapshot is a
//!   neutral DTO so the shell does not need to compile against
//!   the engine. A later slice (≥ D5.1 / D7+) can swap a real
//!   engine-backed `VacModelConfigSnapshot` impl in without
//!   changing call sites.
//!
//! # Snapshot format
//!
//! On-disk JSON at `paths.model_config_file()`:
//!
//! ```json
//! {
//!   "providers": [
//!     {"id": "anthropic", "credentials_present": true},
//!     {"id": "openai",    "credentials_present": false}
//!   ],
//!   "models": [
//!     {
//!       "provider": "anthropic",
//!       "id": "claude-sonnet-4.5",
//!       "label": "Claude Sonnet 4.5",
//!       "reasoning": true,
//!       "cost_label": "$3 / $15 per M"
//!     }
//!   ],
//!   "active": {"provider": "anthropic", "id": "claude-sonnet-4.5"}
//! }
//! ```
//!
//! Missing file is `Ok(None)` (callers fall back to fixtures).
//! Parse / IO errors surface a typed error — production hosts
//! either log + fall back or surface to the operator.

use std::path::Path;

use serde::{Deserialize, Serialize};
use vac_shell_bridge::ProviderId;
use vac_shell_contracts::VacPaths;
use vac_shell_host_model::{HostModel, ModelSource, ProviderInfo};

#[derive(Debug, thiserror::Error)]
pub enum VacConfigModelError {
    #[error("model config io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("model config parse error at {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

/// Trait the shell consumes for the read-only model-config view.
/// Implementations either parse the on-disk JSON snapshot
/// (`JsonFileVacConfig` below) or, in a later slice, project
/// directly from `vac_core::VacConfig` without leaking secrets.
pub trait VacModelConfigSnapshot: Send + Sync {
    fn providers(&self) -> Vec<ProviderInfo>;
    fn models(&self) -> Vec<HostModel>;
    fn active(&self) -> Option<(ProviderId, String)>;
}

// =====================================================================
// On-disk JSON shape
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DiskSnapshot {
    #[serde(default)]
    providers: Vec<DiskProvider>,
    #[serde(default)]
    models: Vec<DiskModel>,
    #[serde(default)]
    active: Option<DiskActive>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskProvider {
    id: String,
    #[serde(default)]
    credentials_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskModel {
    provider: String,
    id: String,
    label: String,
    #[serde(default)]
    reasoning: bool,
    #[serde(default)]
    cost_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskActive {
    provider: String,
    id: String,
}

// =====================================================================
// JSON-file impl
// =====================================================================

/// Read the snapshot from `paths.model_config_file()`. Missing
/// file → `Ok(None)`; the caller decides whether to fall back to
/// fixtures or surface an error.
pub fn load_from_paths(
    paths: &dyn VacPaths,
) -> Result<Option<VacConfigModelSource>, VacConfigModelError> {
    let path = paths.model_config_file();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(VacConfigModelError::Io {
                path: path.display().to_string(),
                source: e,
            });
        }
    };
    let snap: DiskSnapshot =
        serde_json::from_slice(&bytes).map_err(|e| VacConfigModelError::Parse {
            path: path.display().to_string(),
            source: e,
        })?;
    Ok(Some(VacConfigModelSource::from_disk(snap)))
}

/// Same as [`load_from_paths`] but reads from an explicit path,
/// useful for tests and host integrations that resolve the path
/// outside `VacPaths`.
pub fn load_from_file(
    path: impl AsRef<Path>,
) -> Result<Option<VacConfigModelSource>, VacConfigModelError> {
    let path = path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(VacConfigModelError::Io {
                path: path.display().to_string(),
                source: e,
            });
        }
    };
    let snap: DiskSnapshot =
        serde_json::from_slice(&bytes).map_err(|e| VacConfigModelError::Parse {
            path: path.display().to_string(),
            source: e,
        })?;
    Ok(Some(VacConfigModelSource::from_disk(snap)))
}

/// Concrete read-only model source. Impls both
/// `VacModelConfigSnapshot` (this crate's own neutral trait) and
/// `vac_shell_host_model::ModelSource` (so it slots into
/// `ShellCompositionBuilder` directly).
#[derive(Debug, Clone, Default)]
pub struct VacConfigModelSource {
    providers: Vec<ProviderInfo>,
    models: Vec<HostModel>,
    active: Option<(ProviderId, String)>,
}

impl VacConfigModelSource {
    fn from_disk(snap: DiskSnapshot) -> Self {
        let providers = snap
            .providers
            .into_iter()
            .map(|p| ProviderInfo {
                id: ProviderId(p.id),
                credentials_present: p.credentials_present,
            })
            .collect();
        let models = snap
            .models
            .into_iter()
            .map(|m| HostModel {
                provider: ProviderId(m.provider),
                id: m.id,
                label: m.label,
                reasoning: m.reasoning,
                cost_label: m.cost_label,
            })
            .collect();
        let active = snap.active.map(|a| (ProviderId(a.provider), a.id));
        Self {
            providers,
            models,
            active,
        }
    }
}

impl VacModelConfigSnapshot for VacConfigModelSource {
    fn providers(&self) -> Vec<ProviderInfo> {
        self.providers.clone()
    }
    fn models(&self) -> Vec<HostModel> {
        self.models.clone()
    }
    fn active(&self) -> Option<(ProviderId, String)> {
        self.active.clone()
    }
}

/// Hardening helper (D-track post-RC fix). Drop a
/// `(provider, id)` whose provider lacks credentials, or whose
/// model is no longer present in the registry. Without this guard
/// the entrypoint's `fallback_active` path would let a snapshot
/// boot the cockpit with a no-creds active model, contradicting
/// `ModelSelectionState::select_model`'s validation contract.
pub fn sanitize_active_model(
    providers: &[ProviderInfo],
    models: &[HostModel],
    active: Option<(ProviderId, String)>,
) -> Option<(ProviderId, String)> {
    let (provider, id) = active?;
    let provider_has_creds = providers
        .iter()
        .any(|p| p.id == provider && p.credentials_present);
    let model_exists = models
        .iter()
        .any(|m| m.provider == provider && m.id == id);
    if provider_has_creds && model_exists {
        Some((provider, id))
    } else {
        None
    }
}

impl ModelSource for VacConfigModelSource {
    fn providers(&self) -> Vec<ProviderInfo> {
        self.providers.clone()
    }
    fn models(&self) -> Vec<HostModel> {
        self.models.clone()
    }
    fn active_model(&self) -> Option<(ProviderId, String)> {
        self.active.clone()
    }
    fn recent_models(&self, _limit: usize) -> Vec<(ProviderId, String)> {
        // Recents are operator-side state, not config state. The
        // model-config snapshot does not own them; the persisted
        // selection state (slice 9.1+) does.
        Vec::new()
    }
    fn pinned_provider(&self) -> Option<ProviderId> {
        None
    }
}
