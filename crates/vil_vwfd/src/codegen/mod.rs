//! VWFD scaffold generation helpers.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::SUPPORTED_API_VERSION;
use crate::error::VwfdError;
use crate::schema::{
    VwfdDocument, VwfdExecutionMode, VwfdHandler, VwfdKind, VwfdMetadata, VwfdSpec, VwfdStep,
    VwfdWorkflow,
};

pub mod native;
pub mod sidecar;
pub mod wasm;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedArtifact {
    pub document: VwfdDocument,
    pub preview: String,
    pub files: Vec<GeneratedFile>,
}

pub fn generate_handler(
    kind: VwfdKind,
    execution_mode: VwfdExecutionMode,
    name: &str,
) -> Result<GeneratedArtifact, VwfdError> {
    let scaffold_name = normalize_name(name)?;
    let workflow_name = format!("{scaffold_name}-flow");
    let handler = handler_spec(&scaffold_name, &execution_mode);

    let document = VwfdDocument {
        api_version: SUPPORTED_API_VERSION.to_string(),
        kind: kind.normalize(),
        metadata: VwfdMetadata {
            name: scaffold_name.clone(),
            namespace: None,
            labels: HashMap::new(),
            annotations: HashMap::new(),
        },
        spec: VwfdSpec {
            workflows: vec![VwfdWorkflow {
                name: workflow_name.clone(),
                description: Some(format!("Generated scaffold for {scaffold_name}")),
                steps: vec![VwfdStep {
                    id: "run".to_string(),
                    handler: scaffold_name.clone(),
                    condition: None,
                    inputs: HashMap::new(),
                    outputs: HashMap::new(),
                    on_error: None,
                }],
            }],
            triggers: vec![],
            handlers: vec![handler],
        },
    };

    let preview = serde_yaml::to_string(&document)?;
    let mut files = vec![GeneratedFile {
        path: PathBuf::from(format!("workflows/{scaffold_name}.vwfd.yaml")),
        contents: preview.clone(),
    }];

    match execution_mode {
        VwfdExecutionMode::Native => {
            files.push(GeneratedFile {
                path: PathBuf::from(format!("handlers/{scaffold_name}/mod.rs")),
                contents: native::render_handler_module(&scaffold_name),
            });
        }
        VwfdExecutionMode::Wasm => {
            files.push(GeneratedFile {
                path: PathBuf::from(format!("handlers/{scaffold_name}/Cargo.toml")),
                contents: wasm::render_cargo_toml(&scaffold_name),
            });
            files.push(GeneratedFile {
                path: PathBuf::from(format!("handlers/{scaffold_name}/src/lib.rs")),
                contents: wasm::render_lib_rs(&scaffold_name),
            });
        }
        VwfdExecutionMode::Sidecar => {
            files.push(GeneratedFile {
                path: PathBuf::from(format!("handlers/{scaffold_name}/handler.py")),
                contents: sidecar::render_python(&scaffold_name),
            });
            files.push(GeneratedFile {
                path: PathBuf::from(format!("handlers/{scaffold_name}/handler.go")),
                contents: sidecar::render_go(&scaffold_name),
            });
        }
    }

    Ok(GeneratedArtifact {
        document,
        preview,
        files,
    })
}

pub fn parse_kind(input: &str) -> Result<VwfdKind, VwfdError> {
    match input.trim().to_ascii_lowercase().as_str() {
        "vilserver" | "vil_server" | "server" | "handler" => Ok(VwfdKind::VilServer),
        "vxapp" | "vx_app" => Ok(VwfdKind::VilServer),
        "pipeline" => Ok(VwfdKind::Pipeline),
        "connector" => Ok(VwfdKind::Connector),
        other => Err(VwfdError::MissingField(format!(
            "unsupported kind: {other}"
        ))),
    }
}

pub fn parse_execution_mode(input: &str) -> Result<VwfdExecutionMode, VwfdError> {
    match input.trim().to_ascii_lowercase().as_str() {
        "native" => Ok(VwfdExecutionMode::Native),
        "wasm" => Ok(VwfdExecutionMode::Wasm),
        "sidecar" => Ok(VwfdExecutionMode::Sidecar),
        other => Err(VwfdError::UnknownExecutionMode(other.to_string())),
    }
}

fn normalize_name(input: &str) -> Result<String, VwfdError> {
    let mut out = String::with_capacity(input.len());
    let mut prev_underscore = false;

    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };

        if mapped == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }

        out.push(mapped);
    }

    let scaffold_name = out.trim_matches('_').to_string();
    if scaffold_name.is_empty() {
        return Err(VwfdError::MissingField(
            "generator name must contain at least one ASCII alphanumeric character".to_string(),
        ));
    }

    Ok(scaffold_name)
}

fn handler_spec(scaffold_name: &str, execution_mode: &VwfdExecutionMode) -> VwfdHandler {
    let (image, entrypoint, config) = match execution_mode {
        VwfdExecutionMode::Native => (
            None,
            Some(format!("handlers::{scaffold_name}::run")),
            HashMap::new(),
        ),
        VwfdExecutionMode::Wasm => (
            Some(format!("ghcr.io/vastar/{scaffold_name}:wasm")),
            Some("run".to_string()),
            HashMap::new(),
        ),
        VwfdExecutionMode::Sidecar => {
            let mut config = HashMap::new();
            config.insert(
                "socket".to_string(),
                serde_json::Value::String(format!("/tmp/vil-sidecar-{scaffold_name}.sock")),
            );
            config.insert(
                "protocol".to_string(),
                serde_json::Value::String("uds-json".to_string()),
            );
            (
                Some(format!("ghcr.io/vastar/{scaffold_name}:sidecar")),
                Some("main".to_string()),
                config,
            )
        }
    };

    VwfdHandler {
        name: scaffold_name.to_string(),
        execution: execution_mode.clone(),
        image,
        entrypoint,
        config,
    }
}
