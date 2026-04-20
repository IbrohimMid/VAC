//! VWFD schema types.
//!
//! Canonical form: `apiVersion: vil.vastar.io/v1`, `kind: VilServer`.
//! Legacy alias: `kind: VxApp` maps to `VilServer` via `VwfdKind::normalize()`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Top-level document ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VwfdDocument {
    pub api_version: String,
    pub kind: VwfdKind,
    pub metadata: VwfdMetadata,
    pub spec: VwfdSpec,
}

impl VwfdDocument {
    /// Validate required fields and return normalised document.
    pub fn validate(mut self) -> Result<Self, crate::error::VwfdError> {
        use crate::version::SUPPORTED_API_VERSION;
        if self.api_version != SUPPORTED_API_VERSION {
            return Err(crate::error::VwfdError::UnsupportedApiVersion(
                self.api_version.clone(),
            ));
        }
        self.kind = self.kind.normalize();
        Ok(self)
    }
}

// ── Kind ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum VwfdKind {
    VilServer,
    /// Legacy alias — normalized to `VilServer` on load.
    #[serde(rename = "VxApp")]
    VxApp,
    Pipeline,
    Connector,
}

impl VwfdKind {
    pub fn normalize(self) -> Self {
        match self {
            VwfdKind::VxApp => VwfdKind::VilServer,
            other => other,
        }
    }
}

// ── Metadata ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VwfdMetadata {
    pub name: String,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

// ── Spec ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VwfdSpec {
    #[serde(default)]
    pub workflows: Vec<VwfdWorkflow>,
    #[serde(default)]
    pub triggers: Vec<VwfdTrigger>,
    #[serde(default)]
    pub handlers: Vec<VwfdHandler>,
}

// ── Workflow ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VwfdWorkflow {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub steps: Vec<VwfdStep>,
}

// ── Step ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VwfdStep {
    pub id: String,
    pub handler: String,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub inputs: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub outputs: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub on_error: Option<String>,
}

// ── Trigger ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VwfdTrigger {
    pub name: String,
    pub kind: VwfdTriggerKind,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
    /// Workflow to invoke on trigger fire.
    pub workflow: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VwfdTriggerKind {
    Http,
    Cron,
    Event,
    Manual,
}

// ── Handler ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VwfdHandler {
    pub name: String,
    pub execution: VwfdExecutionMode,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VwfdExecutionMode {
    Native,
    Wasm,
    Sidecar,
}
