//! VIL Skills — pre-built workflows for common VIL operations.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::error::ToolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStep {
    pub tool: String,
    pub arguments: serde_json::Value,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParam {
    pub name: String,
    pub description: String,
    pub param_type: String,
    pub default: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub category: String,
    pub steps: Vec<SkillStep>,
    pub parameters: Vec<SkillParam>,
    pub source_pattern: Option<String>,
}

pub struct SkillLoader {
    skills_dir: PathBuf,
    builtin_skills: Vec<Skill>,
}

impl SkillLoader {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills_dir,
            builtin_skills: Self::load_builtin_skills(),
        }
    }

    pub async fn load_skills(&self) -> Result<Vec<Skill>, ToolError> {
        info!("Loading skills from {:?}", self.skills_dir);
        let mut skills = self.builtin_skills.clone();

        if self.skills_dir.exists() {
            let entries = std::fs::read_dir(&self.skills_dir)
                .map_err(|e| ToolError::ExecutionFailed(format!("Cannot read skills dir: {}", e)))?;

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    match Self::load_skill_file(&path) {
                        Ok(skill) => {
                            info!(name = %skill.name, "Loaded skill");
                            skills.push(skill);
                        }
                        Err(e) => {
                            warn!(path = %path.display(), error = %e, "Failed to load skill");
                        }
                    }
                }
            }
        }

        Ok(skills)
    }

    pub async fn find_skill(&self, name: &str) -> Option<Skill> {
        debug!("Looking for skill: {}", name);
        let skills = self.load_skills().await.ok()?;
        skills.into_iter().find(|s| s.name == name)
    }

    pub async fn list_skill_names(&self) -> Vec<String> {
        self.load_skills()
            .await
            .unwrap_or_default()
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    fn load_skill_file(path: &std::path::Path) -> Result<Skill, ToolError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ToolError::ExecutionFailed(format!("Cannot read {}: {}", path.display(), e)))?;
        toml::from_str(&content)
            .map_err(|e| ToolError::ExecutionFailed(format!("Invalid TOML in {}: {}", path.display(), e)))
    }

    fn load_builtin_skills() -> Vec<Skill> {
        vec![
            Skill {
                name: "create-vilapp".to_string(),
                description: "Scaffold a new VilApp server with ServiceProcess, endpoints, and extractors".to_string(),
                category: "server".to_string(),
                source_pattern: Some("vx_app_handler".to_string()),
                parameters: vec![
                    SkillParam {
                        name: "app_name".to_string(),
                        description: "Application name".to_string(),
                        param_type: "string".to_string(),
                        default: Some("my-app".to_string()),
                        required: true,
                    },
                    SkillParam {
                        name: "port".to_string(),
                        description: "Server port".to_string(),
                        param_type: "number".to_string(),
                        default: Some("8080".to_string()),
                        required: false,
                    },
                ],
                steps: vec![
                    SkillStep {
                        tool: "vil_knowledge".to_string(),
                        arguments: serde_json::json!({"query": "vilapp assembly server"}),
                        description: "Look up VilApp pattern from VIL knowledge base".to_string(),
                    },
                    SkillStep {
                        tool: "file_write".to_string(),
                        arguments: serde_json::json!({
                            "path": "src/main.rs",
                            "content": "use vil_server::prelude::*;\n\n#[vil_handler(shm)]\nasync fn hello(ctx: ServiceCtx) -> VilResponse<String> {\n    VilResponse::ok(\"Hello from {{app_name}}\".to_string())\n}\n\n#[tokio::main]\nasync fn main() {\n    let api = ServiceProcess::new(\"api\")\n        .visibility(Visibility::Public)\n        .endpoint(Method::GET, \"/\", get(hello));\n\n    VilApp::new(\"{{app_name}}\")\n        .port({{port}})\n        .observer(true)\n        .service(api)\n        .run()\n        .await;\n}"
                        }),
                        description: "Create main.rs with VilApp scaffold".to_string(),
                    },
                    SkillStep {
                        tool: "file_write".to_string(),
                        arguments: serde_json::json!({
                            "path": "Cargo.toml",
                            "content": "[package]\nname = \"{{app_name}}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nvil_server = \"*\"\ntokio = { version = \"1\", features = [\"full\"] }"
                        }),
                        description: "Create Cargo.toml with VIL dependencies".to_string(),
                    },
                    SkillStep {
                        tool: "file_write".to_string(),
                        arguments: serde_json::json!({
                            "path": "vil-server.yaml",
                            "content": "server:\n  port: {{port}}\n  workers: 0\n\nshm:\n  pool_size: \"8MB\"\n\nprofiles:\n  dev:\n    log_level: debug\n  prod:\n    log_level: warn\n    shm_pool_size: \"256MB\""
                        }),
                        description: "Create vil-server.yaml configuration".to_string(),
                    },
                ],
            },
            Skill {
                name: "create-pipeline".to_string(),
                description: "Scaffold a VIL streaming pipeline with vil_workflow!, HttpSink/Source, and SSE/NDJSON support".to_string(),
                category: "pipeline".to_string(),
                source_pattern: Some("sdk_pipeline".to_string()),
                parameters: vec![
                    SkillParam {
                        name: "pipeline_name".to_string(),
                        description: "Pipeline name".to_string(),
                        param_type: "string".to_string(),
                        default: Some("MyPipeline".to_string()),
                        required: true,
                    },
                    SkillParam {
                        name: "upstream_url".to_string(),
                        description: "Upstream URL to stream from".to_string(),
                        param_type: "string".to_string(),
                        default: None,
                        required: true,
                    },
                    SkillParam {
                        name: "format".to_string(),
                        description: "Stream format: SSE or NDJSON".to_string(),
                        param_type: "string".to_string(),
                        default: Some("SSE".to_string()),
                        required: false,
                    },
                    SkillParam {
                        name: "listen_port".to_string(),
                        description: "Port to listen on".to_string(),
                        param_type: "number".to_string(),
                        default: Some("3080".to_string()),
                        required: false,
                    },
                ],
                steps: vec![
                    SkillStep {
                        tool: "vil_knowledge".to_string(),
                        arguments: serde_json::json!({"query": "sdk pipeline workflow streaming"}),
                        description: "Look up pipeline pattern".to_string(),
                    },
                    SkillStep {
                        tool: "file_write".to_string(),
                        arguments: serde_json::json!({
                            "path": "src/main.rs",
                            "content": "use vil_sdk::prelude::*;\nuse vil_sdk::http::{HttpSinkBuilder, HttpSourceBuilder, HttpFormat, SseSourceDialect};\n\nfn main() -> Result<(), Box<dyn std::error::Error>> {\n    let sink = HttpSinkBuilder::new()\n        .port({{listen_port}})\n        .path(\"/trigger\")\n        .build();\n\n    let source = HttpSourceBuilder::new()\n        .url(\"{{upstream_url}}\")\n        .format(HttpFormat::{{format}})\n        .build();\n\n    let (_ir, handles) = vil_workflow! {\n        name: \"{{pipeline_name}}\",\n        token: ShmToken,\n        instances: [sink, source],\n        routes: [\n            sink.out -> source.in (LoanWrite),\n            source.data -> sink.in (LoanWrite),\n        ]\n    };\n\n    for h in handles { h.join().unwrap(); }\n    Ok(())\n}"
                        }),
                        description: "Create pipeline main.rs".to_string(),
                    },
                ],
            },
            Skill {
                name: "add-plugin".to_string(),
                description: "Create a new VilPlugin implementation with trait, state registration, and endpoint setup".to_string(),
                category: "plugin".to_string(),
                source_pattern: Some("vil_plugin".to_string()),
                parameters: vec![
                    SkillParam {
                        name: "plugin_name".to_string(),
                        description: "Plugin name (PascalCase)".to_string(),
                        param_type: "string".to_string(),
                        default: None,
                        required: true,
                    },
                    SkillParam {
                        name: "plugin_id".to_string(),
                        description: "Plugin ID (dot-separated)".to_string(),
                        param_type: "string".to_string(),
                        default: None,
                        required: true,
                    },
                ],
                steps: vec![
                    SkillStep {
                        tool: "vil_knowledge".to_string(),
                        arguments: serde_json::json!({"query": "VilPlugin trait register"}),
                        description: "Look up plugin pattern".to_string(),
                    },
                    SkillStep {
                        tool: "file_write".to_string(),
                        arguments: serde_json::json!({
                            "path": "src/plugin.rs",
                            "content": "use vil_server::prelude::*;\n\n#[derive(Debug, Clone)]\npub struct {{plugin_name}};\n\nimpl VilPlugin for {{plugin_name}} {\n    fn id(&self) -> &str { \"{{plugin_id}}\" }\n    fn name(&self) -> &str { \"{{plugin_name}}\" }\n\n    fn register(&self, ctx: &mut PluginContext) -> Result<(), PluginError> {\n        ctx.endpoint(Method::GET, \"/status\", status_handler);\n        Ok(())\n    }\n}\n\n#[vil_handler]\nasync fn status_handler(ctx: ServiceCtx) -> VilResponse<String> {\n    VilResponse::ok(\"Plugin active\".to_string())\n}"
                        }),
                        description: "Create plugin implementation file".to_string(),
                    },
                ],
            },
            Skill {
                name: "setup-llm".to_string(),
                description: "Configure multi-provider LLM routing with OpenAI, Anthropic, and Ollama support".to_string(),
                category: "plugin".to_string(),
                source_pattern: Some("llm_plugin".to_string()),
                parameters: vec![
                    SkillParam {
                        name: "primary_provider".to_string(),
                        description: "Primary LLM provider (openai/anthropic/ollama)".to_string(),
                        param_type: "string".to_string(),
                        default: Some("openai".to_string()),
                        required: false,
                    },
                    SkillParam {
                        name: "model".to_string(),
                        description: "Default model".to_string(),
                        param_type: "string".to_string(),
                        default: Some("gpt-4o".to_string()),
                        required: false,
                    },
                ],
                steps: vec![
                    SkillStep {
                        tool: "vil_knowledge".to_string(),
                        arguments: serde_json::json!({"query": "LLM plugin multi-provider routing"}),
                        description: "Look up LLM plugin pattern".to_string(),
                    },
                    SkillStep {
                        tool: "file_write".to_string(),
                        arguments: serde_json::json!({
                            "path": "src/llm_config.rs",
                            "content": "use vil_llm::prelude::*;\n\npub fn setup_llm_router() -> LlmRouter {\n    let router = LlmRouter::new()\n        .route(\"fast\", LlmProvider::openai().model(\"gpt-4o-mini\").build())\n        .route(\"smart\", LlmProvider::{{primary_provider}}().model(\"{{model}}\").build())\n        .route(\"local\", LlmProvider::ollama().model(\"llama3\").build())\n        .default(\"fast\");\n    router\n}"
                        }),
                        description: "Create LLM router configuration".to_string(),
                    },
                ],
            },
            Skill {
                name: "setup-rag".to_string(),
                description: "Configure RAG pipeline with document ingestion, vector search, and LLM generation".to_string(),
                category: "plugin".to_string(),
                source_pattern: Some("rag_pipeline".to_string()),
                parameters: vec![
                    SkillParam {
                        name: "embedder".to_string(),
                        description: "Embedding model".to_string(),
                        param_type: "string".to_string(),
                        default: Some("text-embedding-3-small".to_string()),
                        required: false,
                    },
                ],
                steps: vec![
                    SkillStep {
                        tool: "vil_knowledge".to_string(),
                        arguments: serde_json::json!({"query": "RAG pipeline embedder vector search"}),
                        description: "Look up RAG pattern".to_string(),
                    },
                    SkillStep {
                        tool: "file_write".to_string(),
                        arguments: serde_json::json!({
                            "path": "src/rag_config.rs",
                            "content": "use vil_rag::prelude::*;\n\npub fn setup_rag() -> RagPipeline {\n    RagPipeline::new()\n        .embedder(EmbedderConfig::openai(\"{{embedder}}\"))\n        .vector_store(VectorStoreConfig::in_memory())\n        .llm(LlmProvider::openai().model(\"gpt-4o\").build())\n        .build()\n}"
                        }),
                        description: "Create RAG pipeline configuration".to_string(),
                    },
                ],
            },
        ]
    }
}

pub fn substitute_params(template: &str, params: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in params {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

pub fn substitute_params_json(value: &serde_json::Value, params: &HashMap<String, String>) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(substitute_params(s, params)),
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), substitute_params_json(v, params));
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| substitute_params_json(v, params)).collect())
        }
        other => other.clone(),
    }
}