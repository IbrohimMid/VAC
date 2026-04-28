//! Global configuration for VAC engine.

use crate::policy_gate::{PolicyGateAction, PolicyGateMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
pub use vac_signal::SignalConfig;
pub use vil_llm::{LlmConfig, ProviderConfig as LlmProviderConfig};

/// Top-level VAC configuration, loaded from `~/.config/vac/config.toml`
/// or `<project>/.vac/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacConfig {
    /// LLM provider settings
    pub llm: LlmConfig,
    /// Tool permission settings
    pub tools: ToolConfig,
    /// Memory persistence settings
    pub memory: MemoryConfig,
    /// Context engine settings
    pub context: ContextConfig,
    /// Swarm orchestrator settings
    pub swarm: SwarmConfig,
    /// Trace/audit settings
    pub trace: TraceConfig,
    /// Signal layer (bounded output buffers + distillation)
    #[serde(default)]
    pub signal: SignalConfig,
    /// vil-lsp integration settings
    #[serde(default)]
    pub vil_lsp: VilLspConfig,
    /// Rulebook governance settings
    #[serde(default)]
    pub rulebook: RulebookConfig,
    #[serde(default)]
    pub policy_gate: PolicyGateConfig,
    /// Background runtime settings
    #[serde(default)]
    pub runtime: RuntimeConfig,
    /// MCP server configurations
    #[serde(default)]
    pub mcp_servers: Option<Vec<vac_tools::mcp::McpServerConfig>>,
    /// MCP preset instances (expanded to mcp_servers at runtime)
    #[serde(default)]
    pub mcp_presets: Vec<vac_tools::mcp::McpPresetInstanceConfig>,
    /// Maximum virtual memory (bytes) enforced via setrlimit(RLIMIT_AS).
    #[serde(default)]
    pub memory_cap_bytes: Option<u64>,
    /// Maximum disk usage (bytes) for the .vac/ directory.
    #[serde(default)]
    pub disk_quota_bytes: Option<u64>,
    /// VIL configuration
    #[serde(default)]
    pub vil: VilConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VilConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_version: Option<semver::VersionReq>,
    #[serde(default = "default_vwfd_paths")]
    pub vwfd_paths: Vec<PathBuf>,
    #[serde(default = "default_dev_command")]
    pub dev_command: Option<String>,
    #[serde(default = "default_vil_checkpoint_interval")]
    pub checkpoint_interval_secs: u64,
}

fn default_vwfd_paths() -> Vec<PathBuf> {
    vec![PathBuf::from("./workflows/**/*.vwfd.yaml")]
}

fn default_dev_command() -> Option<String> {
    Some("vac vil dev".to_string())
}

fn default_vil_checkpoint_interval() -> u64 {
    300
}

impl Default for VilConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            min_version: None,
            vwfd_paths: default_vwfd_paths(),
            dev_command: default_dev_command(),
            checkpoint_interval_secs: default_vil_checkpoint_interval(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    /// Default policy: "deny" or "allow"
    #[serde(default = "default_policy")]
    pub default_policy: String,
    /// Explicitly allowed tools
    #[serde(default)]
    pub allow: HashMap<String, bool>,
    /// Explicitly denied tools
    #[serde(default)]
    pub deny: HashMap<String, bool>,
}

fn default_policy() -> String {
    "deny".into()
}

fn default_allowed_tools() -> HashMap<String, bool> {
    [
        "bash",
        "cargo",
        "file_edit",
        "file_read",
        "file_write",
        "git",
        "glob",
        "grep",
        "search",
        "task_done",
        "todo_write",
    ]
    .into_iter()
    .map(|name| (name.to_string(), true))
    .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Path for persistent memory storage
    #[serde(default = "default_memory_path")]
    pub persist_path: PathBuf,
    /// Enable episodic memory
    #[serde(default = "bool_true")]
    pub enable_episodic: bool,
    /// Enable semantic memory
    #[serde(default = "bool_true")]
    pub enable_semantic: bool,
    /// Max episodic entries before consolidation
    #[serde(default = "default_max_episodic")]
    pub max_episodic_entries: usize,
}

fn default_memory_path() -> PathBuf {
    PathBuf::from(".vac/memory/vil_memory.db")
}
fn bool_true() -> bool {
    true
}
fn default_max_episodic() -> usize {
    1000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Enable SHM-based context
    #[serde(default = "bool_true")]
    pub enable_shm: bool,
    /// Max SHM pool size in MB
    #[serde(default = "default_shm_size")]
    pub shm_pool_size_mb: usize,
    /// Enable semantic chunking
    #[serde(default = "bool_true")]
    pub enable_semantic_chunking: bool,
    /// Enable attention routing
    #[serde(default = "bool_true")]
    pub enable_attention_routing: bool,
}

fn default_shm_size() -> usize {
    512
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmConfig {
    /// Max concurrent agents
    #[serde(default = "default_max_agents")]
    pub max_concurrent_agents: usize,
    /// Agent checkpoint interval in seconds
    #[serde(default = "default_checkpoint_interval")]
    pub checkpoint_interval_secs: u64,
    /// Enable parallel agent execution
    #[serde(default = "bool_true")]
    pub enable_parallel: bool,
    /// Agent decision strategy: "default" | "conservative".
    /// Resolved at runtime via vil_swarm::strategy::strategy_from_name.
    #[serde(default = "default_strategy_name")]
    pub strategy: String,
}

fn default_strategy_name() -> String {
    "default".to_string()
}

fn default_max_agents() -> usize {
    8
}
fn default_checkpoint_interval() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceConfig {
    /// Enable VAC trace recording
    #[serde(default = "bool_true")]
    pub enable: bool,
    /// Output directory for trace files
    #[serde(default = "default_trace_path")]
    pub output_path: PathBuf,
    /// Enable COSE signing
    #[serde(default)]
    pub enable_signing: bool,
    /// Redaction policy
    #[serde(default = "default_redaction")]
    pub redaction: RedactionPolicy,
}

fn default_trace_path() -> PathBuf {
    PathBuf::from(".vac/traces")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RedactionPolicy {
    /// Strip file paths
    #[serde(default)]
    pub strip_paths: bool,
    /// Strip API keys/tokens
    #[serde(default = "bool_true")]
    pub strip_secrets: bool,
    /// Custom patterns to redact
    #[serde(default)]
    pub custom_patterns: Vec<String>,
}

fn default_redaction() -> RedactionPolicy {
    RedactionPolicy {
        strip_paths: false,
        strip_secrets: true,
        custom_patterns: vec![],
    }
}

fn get_default_config_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".config"))
}

impl VacConfig {
    fn resolve_relative_paths(mut self, project_root: &Path) -> Self {
        if self.memory.persist_path.is_relative() {
            self.memory.persist_path = project_root.join(&self.memory.persist_path);
        }
        if self.trace.output_path.is_relative() {
            self.trace.output_path = project_root.join(&self.trace.output_path);
        }
        self
    }

    /// Load config from file path.
    pub fn load(path: &Path) -> crate::error::VacResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::VacError::Config(format!("Failed to read config: {e}")))?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| crate::VacError::Config(format!("Failed to parse config: {e}")))?;
        config.validate()?;
        Ok(config)
    }

    /// Load config with fallback: project → global → defaults.
    pub fn load_with_fallback(project_root: &Path) -> crate::error::VacResult<Self> {
        let project_config = project_root.join(".vac/config.toml");
        let mut config = if project_config.exists() {
            Self::load(&project_config)?
        } else if let Some(config_dir) = get_default_config_dir()
            && config_dir.join("vac/config.toml").exists()
        {
            Self::load(&config_dir.join("vac/config.toml"))?
        } else {
            let c = Self::default();
            c.validate()?;
            c
        };
        config.apply_kilo_auth_if_present();
        Ok(config.resolve_relative_paths(project_root))
    }

    pub fn save(project_root: &Path, config: &Self) -> crate::error::VacResult<()> {
        let project_config = project_root.join(".vac/config.toml");
        if let Some(parent) = project_config.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(config)
            .map_err(|e| crate::error::VacError::Config(e.to_string()))?;
        std::fs::write(&project_config, content)?;
        Ok(())
    }

    /// If `vac auth login` saved a Kilo API key (or `KILO_API_KEY` env is set),
    /// auto-register the kilo provider so the Model Switcher exposes it and
    /// the LLM router can resolve credentials without a hand-edited
    /// `.vac/config.toml`.
    fn apply_kilo_auth_if_present(&mut self) {
        let Ok(Some(key)) = crate::auth::resolve_kilo_api_key() else {
            return;
        };
        if std::env::var("KILO_API_KEY")
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            // SAFETY: setting an env var is racy across threads; we do this
            // during single-threaded config load before the runtime spawns.
            unsafe {
                std::env::set_var("KILO_API_KEY", &key);
            }
        }
        self.llm
            .providers
            .entry("kilo".to_string())
            .or_insert_with(|| LlmProviderConfig {
                api_key_env: Some("KILO_API_KEY".to_string()),
                model: Some("kilo-auto/free".to_string()),
                base_url: None,
                ..Default::default()
            });
        if !self.llm.fallback_chain.iter().any(|p| p == "kilo") {
            self.llm.fallback_chain.insert(0, "kilo".to_string());
        }
        if self.llm.default_provider == "anthropic" && !self.llm.providers.contains_key("anthropic")
            || self.llm.default_provider == "anthropic"
                && std::env::var("ANTHROPIC_API_KEY")
                    .map(|v| v.trim().is_empty())
                    .unwrap_or(true)
        {
            self.llm.default_provider = "kilo".to_string();
        }
    }

    pub fn validate(&self) -> crate::error::VacResult<()> {
        self.runtime.validate()
    }
}

impl Default for VacConfig {
    fn default() -> Self {
        let mut default_providers = HashMap::new();
        default_providers.insert(
            "anthropic".to_string(),
            LlmProviderConfig {
                api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
                model: Some("claude-3-7-sonnet-20250219".to_string()),
                base_url: None,
                ..Default::default()
            },
        );
        default_providers.insert(
            "openai".to_string(),
            LlmProviderConfig {
                api_key_env: Some("OPENAI_API_KEY".to_string()),
                model: Some("gpt-4o".to_string()),
                base_url: None,
                ..Default::default()
            },
        );

        Self {
            llm: LlmConfig {
                default_provider: "anthropic".into(),
                providers: default_providers,
                fallback_chain: vec!["anthropic".to_string(), "openai".to_string()],
                budget_tokens: 0,
                requests_per_minute: 0,
                routing: HashMap::new(),
            },
            tools: ToolConfig {
                default_policy: default_policy(),
                allow: default_allowed_tools(),
                deny: HashMap::new(),
            },
            memory: MemoryConfig {
                persist_path: default_memory_path(),
                enable_episodic: true,
                enable_semantic: true,
                max_episodic_entries: default_max_episodic(),
            },
            context: ContextConfig {
                enable_shm: true,
                shm_pool_size_mb: default_shm_size(),
                enable_semantic_chunking: true,
                enable_attention_routing: true,
            },
            swarm: SwarmConfig {
                max_concurrent_agents: default_max_agents(),
                checkpoint_interval_secs: default_checkpoint_interval(),
                enable_parallel: true,
                strategy: default_strategy_name(),
            },
            trace: TraceConfig {
                enable: true,
                output_path: default_trace_path(),
                enable_signing: false,
                redaction: default_redaction(),
            },
            signal: SignalConfig::default(),
            vil_lsp: VilLspConfig::default(),
            rulebook: RulebookConfig::default(),
            policy_gate: PolicyGateConfig::default(),
            runtime: RuntimeConfig::default(),
            mcp_servers: None,
            mcp_presets: vec![],
            memory_cap_bytes: None,
            disk_quota_bytes: None,
            vil: VilConfig::default(),
        }
    }
}

impl VacConfig {
    /// Minimal config for research / eval / replay runs where the operator
    /// wants to strip auxiliary subsystems to observe agent loops in
    /// isolation.
    ///
    /// Disables: trace recording, LLM routing-by-task-kind, MCP presets,
    /// memory persistence (in-memory only), policy-gate enforcement.
    /// Keeps: LLM default provider, tool policy, VIL.
    pub fn minimal() -> Self {
        let mut cfg = Self::default();
        cfg.trace.enable = false;
        cfg.memory.enable_episodic = false;
        cfg.memory.enable_semantic = false;
        cfg.mcp_presets.clear();
        cfg.mcp_servers = None;
        cfg.policy_gate.enable = false;
        cfg.signal.enable = false;
        cfg
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateConfig {
    #[serde(default)]
    pub enable: bool,
    #[serde(default = "default_policy_gate_threshold")]
    pub threshold: f64,
    #[serde(default = "default_policy_gate_mode")]
    pub mode: PolicyGateMode,
    #[serde(default)]
    pub actions: Vec<PolicyGateAction>,
}

fn default_policy_gate_threshold() -> f64 {
    0.85
}

fn default_policy_gate_mode() -> PolicyGateMode {
    PolicyGateMode::Soft
}

impl Default for PolicyGateConfig {
    fn default() -> Self {
        Self {
            enable: false,
            threshold: default_policy_gate_threshold(),
            mode: default_policy_gate_mode(),
            actions: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VilLspConfig {
    /// Enable vil-lsp integration
    #[serde(default = "bool_true")]
    pub enable: bool,
    /// Path to vil-lsp binary (default: "vil-lsp" from PATH)
    #[serde(default = "default_vil_lsp_binary")]
    pub binary_path: std::path::PathBuf,
    /// Extra arguments to pass to vil-lsp
    #[serde(default)]
    pub arguments: Vec<String>,
    /// Startup timeout in milliseconds
    #[serde(default = "default_lsp_timeout_ms")]
    pub startup_timeout_ms: u64,
    /// Max diagnostic items to inject into prompt
    #[serde(default = "default_max_prompt_items")]
    pub max_prompt_items: usize,
    /// Fail engine init if vil-lsp is unavailable (default: false = soft fail)
    #[serde(default)]
    pub fail_on_unavailable: bool,
    /// Analyze workspace on init
    #[serde(default = "bool_true")]
    pub analyze_on_init: bool,
    /// Re-analyze modified files after task execution
    #[serde(default = "bool_true")]
    pub analyze_after_edit: bool,
}

fn default_vil_lsp_binary() -> std::path::PathBuf {
    std::path::PathBuf::from("vil-lsp")
}
fn default_lsp_timeout_ms() -> u64 {
    3000
}
fn default_max_prompt_items() -> usize {
    8
}

impl Default for VilLspConfig {
    fn default() -> Self {
        Self {
            enable: true,
            binary_path: default_vil_lsp_binary(),
            arguments: vec![],
            startup_timeout_ms: default_lsp_timeout_ms(),
            max_prompt_items: default_max_prompt_items(),
            fail_on_unavailable: false,
            analyze_on_init: true,
            analyze_after_edit: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulebookConfig {
    #[serde(default = "bool_true")]
    pub enable: bool,
    /// Additional rulebook search paths (beyond .vac/rules.toml and .vac/rulebooks/)
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    /// Fail engine init if any rulebook has validation errors
    #[serde(default)]
    pub fail_on_invalid: bool,
}

impl Default for RulebookConfig {
    fn default() -> Self {
        // Include global config dir by default
        let global_path = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".config/vac/rulebooks"))
            .unwrap_or_default();
        Self {
            enable: true,
            paths: if global_path.as_os_str().is_empty() {
                vec![]
            } else {
                vec![global_path]
            },
            fail_on_invalid: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountPreset {
    Rust,
    Node,
    Python,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Enable background task runtime
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub sandbox_mode: UserSandboxMode,
    /// "monitor-only" | "suggest-only" | "patch-proposal" | "auto-fix-low-risk"
    #[serde(default = "default_task_intent_mode", alias = "operating_mode")]
    pub task_intent_mode: String,
    /// "host" | "isolated" | "trusted-networked" | "restricted-offline"
    #[serde(default = "default_environment_mode")]
    pub environment_mode: String,
    #[serde(default)]
    pub execution_environment: ExecutionEnvironment,
    #[serde(default)]
    pub container_runtime: Option<String>,
    #[serde(default)]
    pub container_image: Option<String>,
    #[serde(default)]
    pub allowed_mounts: Vec<String>,
    #[serde(default)]
    pub mount_presets: Vec<MountPreset>,
    #[serde(default)]
    pub allowed_env: Vec<String>,
    #[serde(default)]
    pub network_policy: NetworkPolicy,
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UserSandboxMode {
    #[default]
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl UserSandboxMode {
    pub fn parse_user(raw: &str) -> Result<Self, String> {
        let s = raw.trim().to_ascii_lowercase();
        match s.as_str() {
            "read-only" | "read_only" | "readonly" => Ok(Self::ReadOnly),
            "workspace-write" | "workspace_write" | "write" => Ok(Self::WorkspaceWrite),
            "danger-full-access" | "danger_full_access" | "danger" | "host" => {
                Ok(Self::DangerFullAccess)
            }
            _ => Err("expected one of: read-only, workspace-write, danger-full-access".to_string()),
        }
    }

    pub fn as_cli_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::DangerFullAccess => "danger-full-access",
        }
    }

    pub fn apply_to_runtime(&self, runtime: &mut RuntimeConfig) {
        match self {
            Self::ReadOnly => {
                runtime.environment_mode = "restricted-offline".to_string();
                runtime.execution_environment = ExecutionEnvironment::IsolatedBatch;
            }
            Self::WorkspaceWrite => {
                runtime.environment_mode = "isolated".to_string();
                runtime.execution_environment = ExecutionEnvironment::IsolatedInteractive;
            }
            Self::DangerFullAccess => {
                runtime.environment_mode = "host".to_string();
                runtime.execution_environment = ExecutionEnvironment::Host;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionEnvironment {
    #[default]
    Host,
    IsolatedInteractive,
    IsolatedBatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    #[default]
    Inherit,
    RestrictedOffline,
    TrustedNetworked,
}

fn default_task_intent_mode() -> String {
    "monitor-only".to_string()
}

fn default_environment_mode() -> String {
    "host".to_string()
}

fn default_max_concurrent_jobs() -> usize {
    2
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enable: false,
            sandbox_mode: UserSandboxMode::DangerFullAccess,
            task_intent_mode: default_task_intent_mode(),
            environment_mode: default_environment_mode(),
            execution_environment: ExecutionEnvironment::Host,
            container_runtime: None,
            container_image: None,
            allowed_mounts: vec![],
            mount_presets: vec![],
            allowed_env: vec![],
            network_policy: NetworkPolicy::Inherit,
            max_concurrent_jobs: default_max_concurrent_jobs(),
        }
    }
}

impl RuntimeConfig {
    pub fn operating_mode(&self) -> &str {
        &self.task_intent_mode
    }

    pub fn validate(&self) -> crate::error::VacResult<()> {
        if !matches!(
            self.task_intent_mode.as_str(),
            "monitor-only" | "suggest-only" | "patch-proposal" | "auto-fix-low-risk"
        ) {
            return Err(crate::VacError::Config(format!(
                "Invalid runtime.task_intent_mode '{}'. Expected one of: monitor-only, suggest-only, patch-proposal, auto-fix-low-risk",
                self.task_intent_mode
            )));
        }

        if !matches!(
            self.environment_mode.as_str(),
            "host" | "isolated" | "trusted-networked" | "restricted-offline"
        ) {
            return Err(crate::VacError::Config(format!(
                "Invalid runtime.environment_mode '{}'. Expected one of: host, isolated, trusted-networked, restricted-offline",
                self.environment_mode
            )));
        }

        if self.environment_mode == "isolated"
            && self.execution_environment == ExecutionEnvironment::Host
        {
            return Err(crate::VacError::Config(
                "runtime.environment_mode='isolated' requires execution_environment to be isolated_interactive or isolated_batch".to_string(),
            ));
        }

        if self.execution_environment != ExecutionEnvironment::Host
            && self
                .container_image
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            return Err(crate::VacError::Config(
                "runtime.container_image must be set when execution_environment is isolated"
                    .to_string(),
            ));
        }

        if self.environment_mode == "trusted-networked"
            && self.network_policy == NetworkPolicy::RestrictedOffline
        {
            return Err(crate::VacError::Config(
                "runtime.environment_mode='trusted-networked' cannot be combined with network_policy='restricted_offline'".to_string(),
            ));
        }

        if self.environment_mode == "restricted-offline"
            && self.network_policy == NetworkPolicy::TrustedNetworked
        {
            return Err(crate::VacError::Config(
                "runtime.environment_mode='restricted-offline' cannot be combined with network_policy='trusted_networked'".to_string(),
            ));
        }

        Ok(())
    }

    pub fn allowed_mcp_classes(&self) -> Vec<&'static str> {
        match self.environment_mode.as_str() {
            "restricted-offline" => vec!["local-trusted"],
            "isolated" | "trusted-networked" => vec!["local-trusted", "remote-verified"],
            _ => vec!["local-trusted", "remote-verified", "remote-untrusted"],
        }
    }

    pub fn shell_execution_mode(&self) -> &'static str {
        match self.execution_environment {
            ExecutionEnvironment::Host => "host-shell",
            ExecutionEnvironment::IsolatedInteractive => "isolated-shell",
            ExecutionEnvironment::IsolatedBatch => "batch-only",
        }
    }

    pub fn write_capability(&self) -> &'static str {
        match self.task_intent_mode.as_str() {
            "auto-fix-low-risk" => "auto-write-low-risk",
            "patch-proposal" => "patch-proposal-only",
            _ => "no-auto-write",
        }
    }
}

/// One scheduled autopilot task — cron expression + task description
/// + optional profile override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleEntry {
    /// Unique id — used by `vac autopilot schedule remove <id>`.
    pub id: String,
    /// Cron expression in standard 5-field form ("m h dom mon dow").
    pub cron: String,
    /// Task description the autopilot should run when the cron fires.
    pub task: String,
    /// Optional profile override applied to the scheduled run.
    #[serde(default)]
    pub profile: Option<String>,
    /// True to temporarily disable without removing.
    #[serde(default)]
    pub disabled: bool,
}

impl ScheduleEntry {
    /// Validate shape before handing the entry to the runtime
    /// scheduler. Rejects empty id/cron/task, id outside
    /// `[A-Za-z0-9_-]{1,64}`, and cron expressions that aren't
    /// either a `@`-preset or a 5- or 6-field whitespace-separated
    /// form. A deeper parse happens in `vac_runtime::cron_scheduler`
    /// at fire time; this just catches malformed entries on config
    /// load so the operator sees the failure immediately instead of
    /// hours later when the cron silently never fires.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("schedule id is empty".into());
        }
        if self.id.len() > 64 {
            return Err(format!("schedule id '{}' exceeds 64 chars", self.id));
        }
        for ch in self.id.chars() {
            if !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')) {
                return Err(format!(
                    "schedule id '{}' contains illegal char {ch:?}",
                    self.id
                ));
            }
        }
        if self.task.trim().is_empty() {
            return Err(format!("schedule '{}' has empty task", self.id));
        }
        let cron = self.cron.trim();
        if cron.is_empty() {
            return Err(format!("schedule '{}' has empty cron", self.id));
        }
        let is_preset = cron.starts_with('@');
        let field_count = cron.split_whitespace().count();
        if !is_preset && !(5..=6).contains(&field_count) {
            return Err(format!(
                "schedule '{}' cron '{cron}' must be a @preset or 5-6 fields",
                self.id
            ));
        }
        Ok(())
    }
}

/// Autopilot daemon configuration (`autopilot.toml`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfig {
    /// Polling interval in seconds
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// Max tasks to run concurrently
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// Operating mode: "monitor" | "auto"
    #[serde(default = "default_autopilot_mode")]
    pub mode: String,
    /// Recurring task profiles driven by cron expressions. Empty by
    /// default; populated via `vac autopilot schedule add`.
    #[serde(default)]
    pub schedules: Vec<ScheduleEntry>,
}

fn default_poll_interval() -> u64 {
    30
}
fn default_max_concurrent() -> usize {
    1
}
fn default_autopilot_mode() -> String {
    "monitor".to_string()
}

impl Default for AutopilotConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: default_poll_interval(),
            max_concurrent: default_max_concurrent(),
            mode: default_autopilot_mode(),
            schedules: Vec::new(),
        }
    }
}

impl AutopilotConfig {
    pub fn load(project_root: &std::path::Path) -> anyhow::Result<Self> {
        let path = project_root.join("autopilot.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }
}

#[cfg(test)]
mod minimal_config_tests {
    use super::*;

    #[test]
    fn minimal_disables_auxiliary_subsystems() {
        let cfg = VacConfig::minimal();
        assert!(!cfg.trace.enable);
        assert!(!cfg.memory.enable_episodic);
        assert!(!cfg.memory.enable_semantic);
        assert!(!cfg.policy_gate.enable);
        assert!(!cfg.signal.enable);
        assert!(cfg.mcp_servers.is_none());
        assert!(cfg.mcp_presets.is_empty());
        // Core subsystems preserved:
        assert_eq!(cfg.llm.default_provider, "anthropic");
    }

    #[test]
    fn schedule_entry_validate_accepts_well_formed() {
        let e = super::ScheduleEntry {
            id: "nightly-audit".into(),
            cron: "0 2 * * *".into(),
            task: "vac run audit".into(),
            profile: None,
            disabled: false,
        };
        assert!(e.validate().is_ok());
    }

    #[test]
    fn schedule_entry_validate_rejects_malformed() {
        let bad_id = super::ScheduleEntry {
            id: "".into(),
            cron: "0 2 * * *".into(),
            task: "x".into(),
            profile: None,
            disabled: false,
        };
        assert!(bad_id.validate().is_err());

        let bad_cron = super::ScheduleEntry {
            id: "ok".into(),
            cron: "not-a-cron".into(),
            task: "x".into(),
            profile: None,
            disabled: false,
        };
        assert!(bad_cron.validate().is_err());

        let empty_task = super::ScheduleEntry {
            id: "ok".into(),
            cron: "@hourly".into(),
            task: "   ".into(),
            profile: None,
            disabled: false,
        };
        assert!(empty_task.validate().is_err());
    }

    #[test]
    fn schedule_entry_validate_accepts_preset() {
        let e = super::ScheduleEntry {
            id: "daily".into(),
            cron: "@daily".into(),
            task: "vac run x".into(),
            profile: None,
            disabled: false,
        };
        assert!(e.validate().is_ok());
    }
}
