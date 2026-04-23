use crate::rust_analysis::{AnalysisHost, AnalysisRequest, AnalysisResponse};
use std::path::PathBuf;
use std::sync::Arc;
use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use serde_json::Value;

pub struct RustSymbolLookup {
    host: Arc<dyn AnalysisHost>,
}

impl RustSymbolLookup {
    pub fn new(host: Arc<dyn AnalysisHost>) -> Self {
        Self { host }
    }
}

#[async_trait::async_trait]
impl VilTool for RustSymbolLookup {
    fn name(&self) -> &str {
        "rust_symbol_lookup"
    }

    fn description(&self) -> &str {
        "Query rust-analyzer for workspace symbols or file symbols."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "symbol_name": { "type": "string", "description": "Symbol name to lookup in workspace" },
                "file_path": { "type": "string", "description": "File path to list symbols for" }
            }
        })
    }

    fn trust_requirement(&self) -> &str {
        "read_only"
    }

    fn risk_level(&self) -> &str {
        "low"
    }

    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args = args.as_object().ok_or(ToolError::InvalidArguments("args must be object".into()))?;
        
        let response = if let Some(name) = args.get("symbol_name").and_then(|n| n.as_str()) {
            self.host.analyze(AnalysisRequest::ResolveSymbol { name: name.into() }).await
        } else if let Some(path) = args.get("file_path").and_then(|p| p.as_str()) {
            self.host.analyze(AnalysisRequest::FileSymbols { path: PathBuf::from(path) }).await
        } else {
            return Err(ToolError::InvalidArguments("Must provide symbol_name or file_path".into()));
        };

        match response {
            Ok(AnalysisResponse::ResolveSymbol(syms)) | Ok(AnalysisResponse::FileSymbols(syms)) => {
                let json = serde_json::to_value(syms.iter().map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "kind": s.kind,
                        "file": s.file,
                        "line": s.line,
                        "column": s.column
                    })
                }).collect::<Vec<_>>()).unwrap();
                Ok(json)
            }
            Ok(_) => Err(ToolError::ExecutionFailed("Unexpected response type".into())),
            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
        }
    }
}

pub struct RustDiagnostics {
    host: Arc<dyn AnalysisHost>,
}

impl RustDiagnostics {
    pub fn new(host: Arc<dyn AnalysisHost>) -> Self {
        Self { host }
    }
}

#[async_trait::async_trait]
impl VilTool for RustDiagnostics {
    fn name(&self) -> &str {
        "rust_diagnostics"
    }

    fn description(&self) -> &str {
        "Query rust-analyzer for workspace diagnostics."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    fn trust_requirement(&self) -> &str {
        "read_only"
    }

    fn risk_level(&self) -> &str {
        "low"
    }

    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        match self.host.analyze(AnalysisRequest::WorkspaceDiagnostics).await {
            Ok(AnalysisResponse::WorkspaceDiagnostics(diags)) => {
                Ok(serde_json::to_value(diags).unwrap())
            }
            Ok(_) => Err(ToolError::ExecutionFailed("Unexpected response type".into())),
            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
        }
    }
}
