//! VIL Knowledge — authoritative VIL pattern library loaded from corpus.
//!
//! Source of truth priority:
//!   1. `VIL_KNOWLEDGE_ROOT` env var
//!   2. `.vac/config.toml` → `[knowledge] root`
//!   3. Hardcoded bootstrap (fallback only, not authoritative)

pub mod canonical;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub patterns: HashMap<String, Pattern>,
    pub blueprints: Vec<Blueprint>,
    pub best_practices: Vec<BestPractice>,
    /// Whether this KB was loaded from authoritative corpus (true) or fallback bootstrap (false)
    #[serde(default)]
    pub is_authoritative: bool,
    /// Path to corpus root, if loaded from external source
    #[serde(default)]
    pub corpus_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub category: String,
    pub description: String,
    pub code_template: String,
    pub when_to_use: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    pub name: String,
    pub description: String,
    pub modules: Vec<String>,
    pub data_flow: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestPractice {
    pub rule: String,
    pub rationale: String,
    pub examples: Vec<String>,
}

/// A single corpus document loaded from `llm_knowledge/`
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CorpusDoc {
    /// Relative path within corpus root (e.g. "patterns/vx-app.md")
    pub path: String,
    pub content: String,
}

impl KnowledgeBase {
    /// Load from authoritative corpus directory.
    ///
    /// Corpus layout expected:
    ///   `<root>/patterns/*.md`   → Pattern entries
    ///   `<root>/best_practices/*.md` → BestPractice entries
    ///   `<root>/blueprints/*.md` → Blueprint entries
    ///
    /// Falls back to `bootstrap()` if corpus is missing or unreadable.
    pub fn load_from_corpus(root: &Path) -> Self {
        if !root.exists() {
            tracing::warn!(
                path = %root.display(),
                "VIL knowledge corpus not found, falling back to bootstrap"
            );
            return Self::bootstrap();
        }

        tracing::info!(path = %root.display(), "Loading VIL knowledge from authoritative corpus");

        let mut patterns = HashMap::new();
        let mut blueprints = Vec::new();
        let mut best_practices = Vec::new();

        // Load patterns
        let patterns_dir = root.join("patterns");
        if patterns_dir.is_dir() {
            for entry in walkdir_md(&patterns_dir) {
                if let Some(pattern) = parse_pattern_doc(&entry) {
                    patterns.insert(pattern.name.clone(), pattern);
                }
            }
        }

        // Load best practices
        let bp_dir = root.join("best_practices");
        if bp_dir.is_dir() {
            for entry in walkdir_md(&bp_dir) {
                best_practices.extend(parse_best_practices_doc(&entry));
            }
        }

        // Load blueprints
        let bp_dir2 = root.join("blueprints");
        if bp_dir2.is_dir() {
            for entry in walkdir_md(&bp_dir2) {
                if let Some(bp) = parse_blueprint_doc(&entry) {
                    blueprints.push(bp);
                }
            }
        }

        // If corpus exists but is empty/unparseable, still fall back
        if patterns.is_empty() && blueprints.is_empty() && best_practices.is_empty() {
            tracing::warn!(
                path = %root.display(),
                "Corpus found but yielded no entries, falling back to bootstrap"
            );
            return Self::bootstrap();
        }

        tracing::info!(
            patterns = patterns.len(),
            blueprints = blueprints.len(),
            best_practices = best_practices.len(),
            "Corpus loaded successfully"
        );

        Self {
            patterns,
            blueprints,
            best_practices,
            is_authoritative: true,
            corpus_root: Some(root.to_path_buf()),
        }
    }

    /// Resolve corpus root from env → config file → None.
    pub fn resolve_corpus_root(project_root: &Path) -> Option<PathBuf> {
        // 1. Env override
        if let Ok(env_root) = std::env::var("VIL_KNOWLEDGE_ROOT") {
            let p = PathBuf::from(env_root);
            if p.exists() {
                return Some(p);
            }
        }

        // 2. .vac/config.toml [knowledge] root
        let config_path = project_root.join(".vac/config.toml");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(table) = content.parse::<toml::Table>() {
                    if let Some(root_str) = table
                        .get("knowledge")
                        .and_then(|k| k.get("root"))
                        .and_then(|v| v.as_str())
                    {
                        let p = PathBuf::from(root_str);
                        if p.exists() {
                            return Some(p);
                        }
                    }
                }
            }
        }

        None
    }

    /// Load from corpus if available, else bootstrap. Use this as the primary constructor.
    pub fn load(project_root: &Path) -> Self {
        match Self::resolve_corpus_root(project_root) {
            Some(root) => {
                // Check digest cache before full scan
                let cache_path = project_root.join(".vac/cache/knowledge_cache.json");
                if let Some(cached) = Self::load_from_cache(&root, &cache_path) {
                    return cached;
                }
                let kb = Self::load_from_corpus(&root);
                // Persist cache if authoritative
                if kb.is_authoritative {
                    let _ = Self::save_cache(&kb, &root, &cache_path);
                }
                kb
            }
            None => {
                tracing::debug!("No corpus root configured, using bootstrap knowledge");
                Self::bootstrap()
            }
        }
    }

    /// Load from digest cache if corpus hasn't changed.
    fn load_from_cache(corpus_root: &Path, cache_path: &Path) -> Option<Self> {
        if !cache_path.exists() { return None; }
        let cache_content = std::fs::read_to_string(cache_path).ok()?;
        let cached: serde_json::Value = serde_json::from_str(&cache_content).ok()?;

        // Verify digest matches current corpus
        let cached_digest = cached["corpus_digest"].as_str()?;
        let current_digest = Self::corpus_digest(corpus_root)?;
        if cached_digest != current_digest {
            tracing::debug!("Corpus digest changed, invalidating knowledge cache");
            return None;
        }

        // Deserialize cached KB
        let kb: Self = serde_json::from_value(cached["knowledge"].clone()).ok()?;
        tracing::info!(digest = %current_digest, "Knowledge loaded from cache");
        Some(kb)
    }

    fn save_cache(kb: &Self, corpus_root: &Path, cache_path: &Path) -> std::io::Result<()> {
        let digest = Self::corpus_digest(corpus_root).unwrap_or_default();
        let cache = serde_json::json!({
            "corpus_digest": digest,
            "knowledge": kb
        });
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(cache_path, serde_json::to_string(&cache).unwrap_or_default())
    }

    /// Compute a simple digest of all .md files in the corpus root.
    fn corpus_digest(root: &Path) -> Option<String> {
        use std::collections::BTreeMap;
        let mut files: BTreeMap<String, u64> = BTreeMap::new();

        let walker = walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "md"));

        for entry in walker {
            let path = entry.path();
            let rel = path.strip_prefix(root).ok()?.display().to_string();
            let meta = std::fs::metadata(path).ok()?;
            // Use file size + mtime as cheap digest
            let mtime = meta.modified().ok()?
                .duration_since(std::time::UNIX_EPOCH).ok()?
                .as_secs();
            files.insert(rel, meta.len() ^ (mtime << 32));
        }

        if files.is_empty() { return None; }

        // Simple hash: XOR all values, concatenate sorted keys
        let hash: u64 = files.values().fold(0u64, |acc, &v| acc.wrapping_add(v));
        Some(format!("{hash:016x}"))
    }

    pub fn bootstrap() -> Self {
        tracing::info!("Bootstrapping VIL knowledge base (fallback, not authoritative)");

        let mut patterns = HashMap::new();

        // === PATTERN 1: VX_APP Handler (from patterns/vx-app.md) ===
        patterns.insert("vx_app_handler".to_string(), Pattern {
            name: "vx_app_handler".to_string(),
            category: "server".to_string(),
            description: "VIL handler with ShmSlice zero-copy body extraction, ServiceCtx state access, and VilResponse output".to_string(),
            code_template: r#"use vil_server::prelude::*;

#[vil_handler(shm)]
async fn create_order(ctx: ServiceCtx, slice: ShmSlice) -> VilResponse<Order> {
    let input: CreateOrder = vil_json::from_slice(slice.as_ref())?;
    let db = ctx.state::<DbPool>();
    let order = db.insert(&input).await?;
    VilResponse::created(order)
}"#.to_string(),
            when_to_use: "Creating HTTP endpoints in VilApp with zero-copy body extraction. Use ShmSlice instead of Json<T>, ServiceCtx instead of Extension<T>, VilResponse instead of Json(data).".to_string(),
        });

        // === PATTERN 2: VilApp Assembly (from patterns/vx-app.md) ===
        patterns.insert("vilapp_assembly".to_string(), Pattern {
            name: "vilapp_assembly".to_string(),
            category: "server".to_string(),
            description: "Assemble a VilApp with multiple ServiceProcess instances and Tri-Lane mesh".to_string(),
            code_template: r#"#[tokio::main]
async fn main() {
    let api = ServiceProcess::new("api")
        .visibility(Visibility::Public)
        .endpoint(Method::GET, "/orders", get(list_orders))
        .endpoint(Method::POST, "/orders", post(create_order))
        .extension(db_pool);

    let worker = ServiceProcess::new("worker")
        .visibility(Visibility::Internal);

    let mesh = VxMeshConfig::new()
        .route("api", "worker", VxLane::Data);

    VilApp::new("my-app")
        .port(8080)
        .observer(true)
        .service(api)
        .service(worker)
        .mesh(mesh)
        .run()
        .await;
}"#.to_string(),
            when_to_use: "Building a multi-service VIL application with inter-service communication via Tri-Lane mesh. Use .observer(true) for built-in dashboard at /_vil/dashboard/.".to_string(),
        });

        // === PATTERN 3: SDK Pipeline (from patterns/sdk-pipeline.md) ===
        patterns.insert("sdk_pipeline".to_string(), Pattern {
            name: "sdk_pipeline".to_string(),
            category: "pipeline".to_string(),
            description: "HTTP streaming pipeline using vil_workflow! macro with HttpSink/Source".to_string(),
            code_template: r#"use vil_sdk::prelude::*;
use vil_sdk::http::{HttpSinkBuilder, HttpSourceBuilder, HttpFormat, SseSourceDialect};

let sink = HttpSinkBuilder::new()
    .port(3080)
    .path("/trigger")
    .build();

let source = HttpSourceBuilder::new()
    .url("http://upstream/stream")
    .format(HttpFormat::SSE)
    .dialect(SseSourceDialect::OpenAi)
    .json_tap("choices[0].delta.content")
    .build();

let (_ir, handles) = vil_workflow! {
    name: "Gateway",
    token: ShmToken,
    instances: [sink, source],
    routes: [
        sink.out -> source.in (LoanWrite),
        source.data -> sink.in (LoanWrite),
    ]
};

for h in handles { h.join().unwrap(); }"#.to_string(),
            when_to_use: "Building streaming pipelines for SSE or NDJSON data. Use ShmToken for high throughput (8.5M msg/s) or GenericToken for simplicity (1.2M msg/s). Three API layers: http_gateway() (~5 lines), Pipeline::new() (~20 lines), vil_workflow! (full control).".to_string(),
        });

        // === PATTERN 4: NDJSON Transform Pipeline (from pipeline/ndjson.md) ===
        patterns.insert("ndjson_transform".to_string(), Pattern {
            name: "ndjson_transform".to_string(),
            category: "pipeline".to_string(),
            description: "NDJSON streaming pipeline with inline .transform() for filtering/enriching records".to_string(),
            code_template: r#"let source = HttpSourceBuilder::new()
    .url("http://localhost:18081/api/v1/data/ndjson")
    .format(HttpFormat::NDJSON)
    .transform(|line: &[u8]| -> Option<Vec<u8>> {
        let record: serde_json::Value = serde_json::from_slice(line).ok()?;
        if record["status"].as_str()? == "active" {
            Some(line.to_vec())  // Keep record
        } else {
            None  // Drop record
        }
    })
    .build();"#.to_string(),
            when_to_use: "Processing NDJSON streams with per-record filtering, mapping, or enrichment. Return None from .transform() to drop records. Use for ETL, data pipelines, and batch processing.".to_string(),
        });

        // === PATTERN 5: SSE with LLM Dialects (from pipeline/sse.md) ===
        patterns.insert("sse_llm_dialect".to_string(), Pattern {
            name: "sse_llm_dialect".to_string(),
            category: "pipeline".to_string(),
            description: "SSE streaming with AI provider dialect support (OpenAI, Anthropic, Ollama, Cohere, Gemini)".to_string(),
            code_template: r#"// OpenAI dialect
let source = HttpSourceBuilder::new()
    .url("https://api.openai.com/v1/chat/completions")
    .format(HttpFormat::SSE)
    .dialect(SseSourceDialect::OpenAi)
    .json_tap("choices[0].delta.content")
    .build();

// Anthropic dialect
let source = HttpSourceBuilder::new()
    .url("https://api.anthropic.com/v1/messages")
    .format(HttpFormat::SSE)
    .dialect(SseSourceDialect::Anthropic)
    .json_tap("delta.text")
    .build();

// Ollama dialect
let source = HttpSourceBuilder::new()
    .url("http://localhost:11434/api/chat")
    .format(HttpFormat::SSE)
    .dialect(SseSourceDialect::Ollama)
    .json_tap("message.content")
    .build();"#.to_string(),
            when_to_use: "Streaming AI inference from different providers. Each dialect knows the provider's done signal and JSON structure. OpenAI: data: [DONE], Anthropic: event: message_stop, Ollama: \"done\": true.".to_string(),
        });

        // === PATTERN 6: Multi-Pipeline (from patterns/multi-pipeline.md) ===
        patterns.insert("multi_pipeline".to_string(), Pattern {
            name: "multi_pipeline".to_string(),
            category: "pipeline".to_string(),
            description: "Multiple vil_workflow! pipelines sharing a common ExchangeHeap for zero-copy data exchange".to_string(),
            code_template: r#"// Pipeline 1: Ingest
let (_ir1, h1) = vil_workflow! {
    name: "Ingest",
    token: ShmToken,
    instances: [ingest_sink, ingest_source],
    routes: [ingest_sink.out -> ingest_source.in (LoanWrite)]
};

// Pipeline 2: Process (shares ExchangeHeap with Pipeline 1)
let (_ir2, h2) = vil_workflow! {
    name: "Process",
    token: ShmToken,
    instances: [process_sink, process_source],
    routes: [process_sink.out -> process_source.in (LoanWrite)]
};"#.to_string(),
            when_to_use: "Running independent pipelines that share SHM for zero-copy data exchange. Topologies: fan-out (1→N), fan-in (N→1), diamond (fan-out + process + fan-in).".to_string(),
        });

        // === PATTERN 7: VilPlugin Implementation (from plugins/plugin-trait.md) ===
        patterns.insert("vil_plugin".to_string(), Pattern {
            name: "vil_plugin".to_string(),
            category: "plugin".to_string(),
            description: "Implement VilPlugin trait to register capabilities, routes, and state into VilApp".to_string(),
            code_template: r#"use vil_server::plugin::{VilPlugin, PluginContext, PluginError};

struct MetricsPlugin {
    retention_hours: u64,
}

impl VilPlugin for MetricsPlugin {
    fn id(&self) -> &str { "vil.metrics" }
    fn name(&self) -> &str { "Metrics Collector" }

    fn register(&self, ctx: &mut PluginContext) -> Result<(), PluginError> {
        ctx.state(MetricsStore::new(self.retention_hours));
        ctx.endpoint(Method::GET, "/metrics", get(metrics_handler));
        ctx.middleware(MetricsMiddleware::new());
        Ok(())
    }
}

// Register in VilApp:
VilApp::new("my-app")
    .plugin(MetricsPlugin { retention_hours: 24 })
    .service(api_service)
    .run()
    .await;"#.to_string(),
            when_to_use: "Creating reusable plugins for VilApp. Use PluginContext to register state (ctx.state()), endpoints (ctx.endpoint()), and middleware (ctx.middleware()). Plugins share ExchangeHeap and Tri-Lane mesh.".to_string(),
        });

        // === PATTERN 8: LLM Plugin (from plugins/llm.md) ===
        patterns.insert("llm_plugin".to_string(), Pattern {
            name: "llm_plugin".to_string(),
            category: "plugin".to_string(),
            description: "Multi-provider LLM chat with streaming and model routing".to_string(),
            code_template: r#"use vil_llm::prelude::*;

// Basic chat
let provider = LlmProvider::openai()
    .api_key(std::env::var("OPENAI_API_KEY")?)
    .model("gpt-4o")
    .build();

let response = provider.chat(vec![
    Message::system("You are a helpful assistant."),
    Message::user("Explain zero-copy in one sentence."),
]).await?;

// Multi-model routing
let router = LlmRouter::new()
    .route("fast", LlmProvider::openai().model("gpt-4o-mini").build())
    .route("smart", LlmProvider::openai().model("gpt-4o").build())
    .route("local", LlmProvider::ollama().model("llama3").build())
    .default("fast");

let response = router.chat("smart", messages).await?;"#.to_string(),
            when_to_use: "Integrating LLM providers (OpenAI, Anthropic, Ollama) with chat, streaming, and multi-model routing. Use LlmRouter for automatic routing by token count.".to_string(),
        });

        // === PATTERN 9: RAG Pipeline (from plugins/rag.md) ===
        patterns.insert("rag_pipeline".to_string(), Pattern {
            name: "rag_pipeline".to_string(),
            category: "plugin".to_string(),
            description: "Document ingestion, vector search, and retrieval-augmented generation".to_string(),
            code_template: r#"use vil_rag::prelude::*;

let rag = RagPipeline::new()
    .embedder(EmbedderConfig::openai("text-embedding-3-small"))
    .vector_store(VectorStoreConfig::in_memory())
    .llm(LlmProvider::openai().model("gpt-4o").build())
    .build();

// Ingest documents
rag.ingest(vec![
    Document::text("VIL uses ShmSlice for zero-copy body extraction."),
    Document::file("path/to/document.pdf")?,
]).await?;

// Query with retrieval
let answer = rag.query("How does VIL handle request bodies?").await?;

// Direct similarity search
let results = rag.search("zero-copy transport", 5).await?;"#.to_string(),
            when_to_use: "Building RAG systems with VIL. Chunk strategies: Sliding, Sentence, Paragraph, Code, Table. Register as VilPlugin for server integration.".to_string(),
        });

        // === PATTERN 10: Agent with ReAct Loop (from plugins/agent.md) ===
        patterns.insert("agent_react".to_string(), Pattern {
            name: "agent_react".to_string(),
            category: "plugin".to_string(),
            description: "Tool-augmented AI agent with ReAct (Thought-Action-Observation) loop".to_string(),
            code_template: r#"use vil_agent::prelude::*;

let agent = Agent::new()
    .llm(LlmProvider::openai().model("gpt-4o").build())
    .tool(CalculatorTool::new())
    .tool(HttpFetchTool::new())
    .memory(ConversationMemory::new(max_turns: 20))
    .max_turns(10)
    .build();

let result = agent.run("What is the population of Tokyo divided by 3?").await?;
println!("{}", result.final_answer);

// Custom tool implementation:
impl Tool for WeatherTool {
    fn name(&self) -> &str { "weather" }
    fn description(&self) -> &str { "Get current weather for a city" }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "city": { "type": "string", "description": "City name" }
            },
            "required": ["city"]
        })
    }
    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let city = params["city"].as_str().unwrap_or("unknown");
        ToolResult::ok(serde_json::to_value(fetch_weather(city).await?)?)
    }
}"#.to_string(),
            when_to_use: "Building AI agents that use tools. Built-in tools: CalculatorTool, HttpFetchTool, RetrievalTool. Custom tools implement the Tool trait. Agent follows Thought-Action-Observation ReAct loop.".to_string(),
        });

        // === PATTERN 11: ShmSlice Extractors (from server/extractors.md) ===
        patterns.insert("shm_extractors".to_string(), Pattern {
            name: "shm_extractors".to_string(),
            category: "server".to_string(),
            description: "VIL request extractors: ShmSlice (zero-copy body), ServiceCtx (typed state), RequestId".to_string(),
            code_template: r#"// ShmSlice — zero-copy body extraction
async fn handler(slice: ShmSlice) -> VilResponse<Output> {
    let data: MyType = slice.json()?;    // Deserialize JSON
    let text = slice.text()?;             // As UTF-8 string
    let raw = slice.as_bytes();           // Raw bytes
    VilResponse::ok(process(data))
}

// ServiceCtx — typed state access
async fn handler(ctx: ServiceCtx) -> VilResponse<Output> {
    let db = ctx.state::<DbPool>();       // Typed state
    let name = ctx.service_name();        // Service identity
    let sid = ctx.session_id();           // Tri-Lane session ID
}

// RequestId — auto-extracted from X-Request-Id
async fn handler(id: RequestId) -> VilResponse<Output> {
    tracing::info!(%id, "Processing request");
}"#.to_string(),
            when_to_use: "Handling HTTP requests in VIL handlers. Always use ShmSlice instead of Json<T>, ServiceCtx instead of Extension<T>. Standard axum extractors (Path, Query, HeaderMap) work alongside VIL extractors.".to_string(),
        });

        // === PATTERN 12: VilResponse & Error Handling (from server/response.md) ===
        patterns.insert("vil_response".to_string(), Pattern {
            name: "vil_response".to_string(),
            category: "server".to_string(),
            description: "VilResponse SIMD-accelerated JSON envelope and VilError RFC 7807 structured errors".to_string(),
            code_template: r#"// Success responses
VilResponse::ok(data)           // 200 OK
VilResponse::created(data)      // 201 Created
VilResponse::with_shm(data)     // Write-through to SHM

// Error handling
async fn get_order(Path(id): Path<u64>) -> HandlerResult<VilResponse<Order>> {
    let order = find_order(id)
        .ok_or_else(|| VilError::not_found(format!("Order {} not found", id)))?;
    Ok(VilResponse::ok(order))
}

// VilError factory methods:
VilError::bad_request("detail")
VilError::unauthorized("detail")
VilError::forbidden("detail")
VilError::not_found("detail")
VilError::internal("detail")
VilError::rate_limited("detail")
VilError::service_unavailable("detail")"#.to_string(),
            when_to_use: "Returning HTTP responses and handling errors in VIL. VilResponse uses SIMD-accelerated JSON (sonic-rs, 2-5x faster). VilError produces RFC 7807 Problem Details. HandlerResult<T> = Result<T, VilError>.".to_string(),
        });

        // === PATTERN 13: Configuration Profiles (from server/config.md) ===
        patterns.insert("config_profiles".to_string(), Pattern {
            name: "config_profiles".to_string(),
            category: "config".to_string(),
            description: "VIL configuration with 3 profiles (dev/staging/prod) and ENV overrides".to_string(),
            code_template: r#"# vil-server.yaml
server:
  port: 8080
  workers: 0  # 0 = auto-detect CPU cores

shm:
  pool_size: "64MB"
  page_size: "4KB"

profiles:
  dev:
    shm_pool_size: "8MB"
    log_level: "debug"
    admin_enabled: true
  staging:
    shm_pool_size: "64MB"
    log_level: "info"
    db_pool_size: 20
  prod:
    shm_pool_size: "256MB"
    log_level: "warn"
    db_pool_size: 50
    admin_enabled: false

# ENV overrides (highest precedence):
# VIL_PROFILE=prod
# VIL_SERVER_PORT=9090
# VIL_SHM_POOL_SIZE=512MB
# VIL_DATABASE_URL=postgres://..."#.to_string(),
            when_to_use: "Configuring VIL applications. Precedence: Code Default → YAML → Profile → ENV (VIL_*). Use VIL_PROFILE env var to select profile. SHM pool sizes: dev=8MB, staging=64MB, prod=256MB.".to_string(),
        });

        // === PATTERN 14: Sidecar SDK (from tools/sidecar.md) ===
        patterns.insert("sidecar_sdk".to_string(), Pattern {
            name: "sidecar_sdk".to_string(),
            category: "tools".to_string(),
            description: "Python/Go sidecar processes connected to VIL via Unix Domain Socket with SHM bridge".to_string(),
            code_template: r#"// Rust server-side registration:
let sidecar = SidecarPool::new()
    .socket("/tmp/vil-sidecar.sock")
    .pool_size(4)
    .reconnect(ReconnectPolicy::exponential(100, 5000))
    .build()
    .await?;

let service = ServiceProcess::new("ml")
    .extension(sidecar.clone())
    .endpoint(Method::POST, "/predict", post(predict_handler));

// Python sidecar:
# from vil_sdk import VilSidecar
# sidecar = VilSidecar(name="ml-inference", socket="/tmp/vil-sidecar.sock")
# @sidecar.handler("predict")
# async def predict(request):
#     model = load_model()
#     return {"prediction": model.predict(request.data).tolist()}
# sidecar.run()"#.to_string(),
            when_to_use: "Integrating Python/Go code into VIL pipelines. Sidecar latency ~12μs per call. Use SHM Bridge for large payloads (request.shm_read/shm_write). Failover: restart with max retries.".to_string(),
        });

        // === PATTERN 15: AI Gateway Recipe (from recipes/ai-gateway.md) ===
        patterns.insert("ai_gateway".to_string(), Pattern {
            name: "ai_gateway".to_string(),
            category: "recipe".to_string(),
            description: "SSE streaming AI gateway with content filtering and multi-provider support".to_string(),
            code_template: r#"use vil_sdk::prelude::*;
use vil_sdk::http::{HttpSinkBuilder, HttpSourceBuilder, HttpFormat, SseSourceDialect};

let sink = HttpSinkBuilder::new()
    .port(3080)
    .path("/chat")
    .build();

let source = HttpSourceBuilder::new()
    .url("https://api.openai.com/v1/chat/completions")
    .format(HttpFormat::SSE)
    .dialect(SseSourceDialect::OpenAi)
    .json_tap("choices[0].delta.content")
    .transform(|chunk: &[u8]| -> Option<Vec<u8>> {
        let text = std::str::from_utf8(chunk).ok()?;
        // Content filtering: block sensitive terms
        if text.contains("CONFIDENTIAL") { return None; }
        Some(chunk.to_vec())
    })
    .build();

let (_ir, handles) = vil_workflow! {
    name: "AiGateway",
    token: ShmToken,
    instances: [sink, source],
    routes: [
        sink.out -> source.in (LoanWrite),
        source.data -> sink.in (LoanWrite),
    ]
};"#.to_string(),
            when_to_use: "Building AI inference gateways. Supports content filtering via .transform(), multi-provider routing (OpenAI/Anthropic/Ollama with dialect-specific json_tap), and zero-copy SHM transport.".to_string(),
        });

        // === BEST PRACTICES (from VIL design principles) ===
        let best_practices = vec![
            BestPractice {
                rule: "Use ShmSlice instead of Json<T> for request bodies".to_string(),
                rationale: "Zero-copy extraction from ExchangeHeap. SIMD JSON parsing via sonic-rs (2-5x faster than serde_json).".to_string(),
                examples: vec![
                    "async fn handler(slice: ShmSlice) -> VilResponse<T> { let data: T = slice.json()?; }".to_string(),
                ],
            },
            BestPractice {
                rule: "Use ServiceCtx instead of Extension<T> for state access".to_string(),
                rationale: "Typed state access with Tri-Lane metadata (session_id, service_name, lane context).".to_string(),
                examples: vec![
                    "async fn handler(ctx: ServiceCtx) -> VilResponse<T> { let db = ctx.state::<DbPool>(); }".to_string(),
                ],
            },
            BestPractice {
                rule: "Use VilResponse instead of Json(data) for responses".to_string(),
                rationale: "SIMD-accelerated serialization, SHM write-through, consistent envelope format.".to_string(),
                examples: vec![
                    "VilResponse::ok(data)".to_string(),
                    "VilResponse::created(data)".to_string(),
                ],
            },
            BestPractice {
                rule: "Use ShmToken for high-throughput pipelines, GenericToken for simple ones".to_string(),
                rationale: "ShmToken: 32 bytes, zero-alloc, 8.5M msg/s. GenericToken: in-memory Bytes, 1.2M msg/s.".to_string(),
                examples: vec![
                    "vil_workflow! { name: \"Fast\", token: ShmToken, ... }".to_string(),
                ],
            },
            BestPractice {
                rule: "Use #[vil_state], #[vil_event], #[vil_fault], #[vil_decision] semantic macros".to_string(),
                rationale: "Semantic types auto-select correct Tri-Lane, memory class, and validation constraints.".to_string(),
                examples: vec![
                    "#[vil_state] for mutable session state → Data Lane".to_string(),
                    "#[vil_event] for immutable event logs → Data/Control Lane".to_string(),
                    "#[vil_fault] for structured errors → Control Lane".to_string(),
                    "#[vil_decision] for routing decisions → Trigger Lane".to_string(),
                ],
            },
        ];

        // === BLUEPRINTS ===
        let blueprints = vec![
            Blueprint {
                name: "microservice".to_string(),
                description: "Multi-service VilApp with API gateway, workers, and Tri-Lane mesh"
                    .to_string(),
                modules: vec![
                    "vil_server_core (VilApp, ServiceProcess)".to_string(),
                    "vil_server_web (extractors, response)".to_string(),
                    "vil_server_mesh (Tri-Lane routing)".to_string(),
                    "vil_server_config (profiles, YAML)".to_string(),
                ],
                data_flow:
                    "HTTP → ShmSlice → ServiceCtx → Business Logic → VilResponse → SIMD JSON"
                        .to_string(),
            },
            Blueprint {
                name: "ai_streaming".to_string(),
                description:
                    "AI inference streaming pipeline with SSE dialects and content filtering"
                        .to_string(),
                modules: vec![
                    "vil_sdk (Pipeline, vil_workflow!)".to_string(),
                    "vil_llm (LlmProvider, LlmRouter)".to_string(),
                    "vil_shm (ExchangeHeap, ShmToken)".to_string(),
                ],
                data_flow:
                    "HTTP Trigger → vil_workflow! → SSE Source (dialect) → .transform() → Response"
                        .to_string(),
            },
            Blueprint {
                name: "rag_service".to_string(),
                description:
                    "RAG service with document ingestion, vector search, and LLM generation"
                        .to_string(),
                modules: vec![
                    "vil_rag (RagPipeline, chunking, embeddings)".to_string(),
                    "vil_llm (LlmProvider for generation)".to_string(),
                    "vil_server_core (VilApp, RagPlugin)".to_string(),
                ],
                data_flow:
                    "Document → Chunk → Embed → VectorStore → Query → Retrieve → LLM → Answer"
                        .to_string(),
            },
        ];

        Self {
            patterns,
            blueprints,
            best_practices,
            is_authoritative: false,
            corpus_root: None,
        }
    }

    /// Search patterns by keyword matching on name, category, description, and when_to_use
    pub fn search_patterns(&self, query: &str) -> Vec<&Pattern> {
        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<(&Pattern, usize)> = self
            .patterns
            .values()
            .filter_map(|pattern| {
                let searchable = format!(
                    "{} {} {} {}",
                    pattern.name, pattern.category, pattern.description, pattern.when_to_use
                )
                .to_lowercase();

                let score = keywords
                    .iter()
                    .filter(|kw| searchable.contains(**kw))
                    .count();

                if score > 0 {
                    Some((pattern, score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by relevance score (descending)
        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.into_iter().map(|(p, _)| p).collect()
    }

    /// Search best practices by keyword
    pub fn search_best_practices(&self, query: &str) -> Vec<&BestPractice> {
        let query_lower = query.to_lowercase();
        self.best_practices
            .iter()
            .filter(|bp| {
                bp.rule.to_lowercase().contains(&query_lower)
                    || bp.rationale.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Get patterns by category
    pub fn patterns_by_category(&self, category: &str) -> Vec<&Pattern> {
        self.patterns
            .values()
            .filter(|p| p.category == category)
            .collect()
    }
}

// ── Corpus parsing helpers ────────────────────────────────────────────────────

/// Walk a directory and return (path, content) for all `.md` files.
fn walkdir_md(dir: &Path) -> Vec<CorpusDoc> {
    let mut docs = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return docs;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                docs.push(CorpusDoc {
                    path: path.to_string_lossy().to_string(),
                    content,
                });
            }
        }
    }
    docs
}

/// Parse a markdown corpus doc into a Pattern.
///
/// Looks for HTML comment metadata at the top of the file:
/// `<!-- name: vx_app_handler -->`, `<!-- category: server -->`, etc.
/// First paragraph after metadata = description.
/// First fenced code block = code_template.
fn parse_pattern_doc(doc: &CorpusDoc) -> Option<Pattern> {
    let name = extract_meta(&doc.content, "name")
        .or_else(|| {
            // Derive name from filename
            std::path::Path::new(&doc.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.replace('-', "_"))
        })?;

    let category = extract_meta(&doc.content, "category").unwrap_or_else(|| "general".to_string());
    let when_to_use = extract_meta(&doc.content, "when_to_use").unwrap_or_default();
    let description = extract_first_paragraph(&doc.content);
    let code_template = extract_first_code_block(&doc.content).unwrap_or_default();

    Some(Pattern {
        name,
        category,
        description,
        code_template,
        when_to_use,
    })
}

/// Parse best practices from a markdown doc.
/// Each `## Rule` heading starts a new best practice.
fn parse_best_practices_doc(doc: &CorpusDoc) -> Vec<BestPractice> {
    let mut practices = Vec::new();
    let mut current_rule: Option<String> = None;
    let mut current_rationale = String::new();
    let mut current_examples: Vec<String> = Vec::new();
    let mut in_example = false;
    let mut example_buf = String::new();

    for line in doc.content.lines() {
        if line.starts_with("## ") {
            // Save previous
            if let Some(rule) = current_rule.take() {
                practices.push(BestPractice {
                    rule,
                    rationale: current_rationale.trim().to_string(),
                    examples: current_examples.clone(),
                });
            }
            current_rule = Some(line.trim_start_matches("## ").trim().to_string());
            current_rationale.clear();
            current_examples.clear();
            in_example = false;
        } else if line.starts_with("```") {
            if in_example {
                current_examples.push(example_buf.trim().to_string());
                example_buf.clear();
                in_example = false;
            } else {
                in_example = true;
            }
        } else if in_example {
            example_buf.push_str(line);
            example_buf.push('\n');
        } else if current_rule.is_some() && !line.trim().is_empty() {
            current_rationale.push_str(line);
            current_rationale.push(' ');
        }
    }
    if let Some(rule) = current_rule {
        practices.push(BestPractice {
            rule,
            rationale: current_rationale.trim().to_string(),
            examples: current_examples,
        });
    }
    practices
}

/// Parse a blueprint from a markdown doc.
fn parse_blueprint_doc(doc: &CorpusDoc) -> Option<Blueprint> {
    let name = extract_meta(&doc.content, "name").or_else(|| {
        std::path::Path::new(&doc.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.replace('-', "_"))
    })?;

    let description = extract_first_paragraph(&doc.content);
    let data_flow = extract_meta(&doc.content, "data_flow").unwrap_or_default();

    // Modules: lines starting with "- " under a "## Modules" heading
    let modules = extract_list_under_heading(&doc.content, "Modules");

    Some(Blueprint {
        name,
        description,
        modules,
        data_flow,
    })
}

fn extract_meta(content: &str, key: &str) -> Option<String> {
    let prefix = format!("<!-- {key}:");
    content.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with(&prefix) && line.ends_with("-->") {
            let inner = line
                .trim_start_matches(&prefix)
                .trim_end_matches("-->")
                .trim();
            Some(inner.to_string())
        } else {
            None
        }
    })
}

fn extract_first_paragraph(content: &str) -> String {
    let mut buf = String::new();
    let mut started = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<!--") || trimmed.starts_with('#') {
            if started {
                break;
            }
            continue;
        }
        if trimmed.is_empty() {
            if started {
                break;
            }
        } else {
            started = true;
            buf.push_str(trimmed);
            buf.push(' ');
        }
    }
    buf.trim().to_string()
}

fn extract_first_code_block(content: &str) -> Option<String> {
    let mut in_block = false;
    let mut buf = String::new();
    for line in content.lines() {
        if line.starts_with("```") {
            if in_block {
                return Some(buf.trim_end().to_string());
            } else {
                in_block = true;
            }
        } else if in_block {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    None
}

fn extract_list_under_heading(content: &str, heading: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut under = false;
    for line in content.lines() {
        if line.starts_with("## ") {
            under = line.trim_start_matches("## ").trim() == heading;
        } else if under && line.trim_start().starts_with("- ") {
            items.push(line.trim_start().trim_start_matches("- ").trim().to_string());
        }
    }
    items
}
