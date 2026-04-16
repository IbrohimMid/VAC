//! vil_plumbing — agent-callable tool that explains VIL-generated plumbing.
//!
//! Walks `#[vil_*]` attributes on every struct/function/impl in a target file
//! and provides a structured explanation of what each macro generates.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

/// Validate that a file path is within the project root (no traversal).
fn validate_path_within_root(
    working_dir: &std::path::Path,
    file: &str,
) -> Result<std::path::PathBuf, ToolError> {
    let abs_path = working_dir.join(file);
    let canonical = abs_path.canonicalize().map_err(|_| {
        ToolError::ExecutionFailed(format!("File not found: {}", file))
    })?;
    if !canonical.starts_with(working_dir) {
        return Err(ToolError::ExecutionFailed(
            "Path traversal denied: file must be within project root".into(),
        ));
    }
    Ok(canonical)
}

#[derive(Debug, Deserialize)]
struct PlumbingInput {
    /// File path relative to project root.
    file: String,
}

#[derive(Debug, Serialize)]
struct PlumbingOutput {
    file: String,
    entries: Vec<PlumbingEntry>,
    total_vil_attrs: usize,
}

#[derive(Debug, Serialize)]
struct PlumbingEntry {
    entity_name: String,
    entity_type: String,
    attr: String,
    generated_traits: Vec<String>,
    description: String,
}

pub struct VilPlumbingTool;

impl VilPlumbingTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VilPlumbingTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns (generated_traits, description) for a known VIL attribute.
fn describe_vil_attr(attr: &str) -> (Vec<String>, String) {
    let attr_base = attr.split("::").next().unwrap_or(attr);
    let attr_base = attr_base.split('(').next().unwrap_or(attr_base);
    match attr_base.trim() {
        "vil_handler" => (
            vec!["VilHandler".into(), "VilRoute".into()],
            "Generates handler registration, request parsing, ShmSlice body extraction, \
            and tri-lane routing plumbing."
                .into(),
        ),
        "vil_state" => (
            vec!["VilState".into(), "Serialize".into(), "Deserialize".into()],
            "Generates state management plumbing: serialization, persistence hooks, \
            and semantic role tagging for the IR."
                .into(),
        ),
        "vil_event" => (
            vec!["VilEvent".into(), "Serialize".into(), "Deserialize".into()],
            "Generates event bus integration: publish/subscribe wiring, serialization, \
            and semantic role tagging."
                .into(),
        ),
        "vil_fault" => (
            vec!["VilFault".into(), "Display".into(), "Error".into()],
            "Generates fault handling plumbing: error classification, display formatting, \
            and fault propagation chain."
                .into(),
        ),
        "vil_decision" => (
            vec!["VilDecision".into()],
            "Generates decision record plumbing: audit trail integration and \
            semantic classification."
                .into(),
        ),
        "vil_message" => (
            vec![
                "VilMessage".into(),
                "Serialize".into(),
                "Deserialize".into(),
            ],
            "Generates message encoding/decoding, zero-copy compatibility checks, \
            and queue integration plumbing."
                .into(),
        ),
        "vil_config" => (
            vec!["VilConfig".into(), "Deserialize".into()],
            "Generates configuration loading from TOML/env, validation hooks, \
            and hot-reload support."
                .into(),
        ),
        "vil_plugin" => (
            vec!["VilPlugin".into()],
            "Generates plugin lifecycle hooks: init, start, stop, health check wiring.".into(),
        ),
        "vil_pipeline" => (
            vec!["VilPipeline".into()],
            "Generates data pipeline stage registration and throughput instrumentation.".into(),
        ),
        _ => (
            vec![],
            format!(
                "Unknown VIL attribute '{}'. No generated trait information available.",
                attr
            ),
        ),
    }
}

#[async_trait]
impl VilTool for VilPlumbingTool {
    fn name(&self) -> &str {
        "vil_plumbing"
    }

    fn description(&self) -> &str {
        "Explain the VIL-generated plumbing in a file. Lists every #[vil_*] attribute \
        on structs, functions, and impls with a description of what each macro generates \
        (traits, wiring, serialization, routing, etc.)."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "File path relative to project root"
                }
            },
            "required": ["file"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "safe"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: PlumbingInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let abs_path = validate_path_within_root(&context.working_dir, &input.file)?;

        let module = vil_ir::parser::parse_file(&abs_path)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to parse file: {e}")))?;

        let mut entries = Vec::new();

        // Walk functions
        for func in &module.functions {
            for attr in &func.vil_attrs {
                let (traits, desc) = describe_vil_attr(attr);
                entries.push(PlumbingEntry {
                    entity_name: func.name.clone(),
                    entity_type: "Function".into(),
                    attr: attr.clone(),
                    generated_traits: traits,
                    description: desc,
                });
            }
        }

        // Walk structs
        for s in &module.structs {
            for attr in &s.vil_attrs {
                let (traits, desc) = describe_vil_attr(attr);
                entries.push(PlumbingEntry {
                    entity_name: s.name.clone(),
                    entity_type: "Struct".into(),
                    attr: attr.clone(),
                    generated_traits: traits,
                    description: desc,
                });
            }
        }

        // Walk impls for VIL trait implementations
        for imp in &module.impls {
            if let Some(trait_name) = &imp.trait_name {
                if trait_name.starts_with("Vil") {
                    entries.push(PlumbingEntry {
                        entity_name: imp.self_type.clone(),
                        entity_type: "Impl".into(),
                        attr: format!("impl {} (manual)", trait_name),
                        generated_traits: vec![trait_name.clone()],
                        description: format!(
                            "Manual implementation of {} — VIL macros normally generate this. \
                            Check if this is intentional or should be replaced with a macro.",
                            trait_name
                        ),
                    });
                }
            }
        }

        let total = entries.len();
        let output = PlumbingOutput {
            file: input.file,
            entries,
            total_vil_attrs: total,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
