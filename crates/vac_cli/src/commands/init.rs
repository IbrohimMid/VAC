//! `vac init` — Initialize project context.

use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

pub async fn execute(project_root: PathBuf, force: bool) -> anyhow::Result<()> {
    let vac_dir = project_root.join(".vac");

    if tokio::fs::try_exists(&vac_dir).await? && !force {
        println!(
            "✓ Project already initialized at {}",
            project_root.display()
        );
        println!("   Use --force to re-initialize.");
        return Ok(());
    }

    println!(
        "🚀 Initializing VAC project at {}...",
        project_root.display()
    );

    // Create .vac directory structure
    tokio::fs::create_dir_all(vac_dir.join("sessions")).await?;
    tokio::fs::create_dir_all(vac_dir.join("memory")).await?;
    tokio::fs::create_dir_all(vac_dir.join("traces")).await?;
    tokio::fs::create_dir_all(vac_dir.join("cache")).await?;
    tokio::fs::create_dir_all(vac_dir.join("skills")).await?;
    tokio::fs::create_dir_all(vac_dir.join("approvals")).await?;
    tokio::fs::create_dir_all(project_root.join("workflows")).await?;

    // Create default config if not exists
    let config_path = vac_dir.join("config.toml");
    if !tokio::fs::try_exists(&config_path).await? {
        let default_config = r#"# VAC Configuration
# VIL-Native Autonomous Development Agent
# See https://vastar.id/docs/vac/config for full reference

# =============================================================================
# vil-lsp Integration (Phase 6)
# =============================================================================
# VAC integrates with vil-lsp for editor-grade semantic diagnostics.
# Requires vil-lsp binary in PATH or set binary_path below.

[vil_lsp]
enable = true
binary_path = "vil-lsp"
arguments = []
startup_timeout_ms = 3000
max_prompt_items = 8
fail_on_unavailable = false
analyze_on_init = true
analyze_after_edit = true

# =============================================================================
# VIL Knowledge Source of Truth
# =============================================================================
# Set this to the path of your VIL llm_knowledge corpus.
# If not set, VAC falls back to built-in bootstrap patterns (not authoritative).
# Can also be set via VIL_KNOWLEDGE_ROOT environment variable.

[knowledge]
# root = "/home/emp/Downloads/vac/VIL/llm_knowledge"
# require_authoritative = false

# =============================================================================
# LLM Configuration
# =============================================================================

[llm]
default_provider = "anthropic"
budget_tokens = 0

[llm.providers.anthropic]
# Kilo Gateway: https://kilo.ai
api_key_env = "KILO_API_KEY"
model = "kilo/free"
max_tokens = 4000

# =============================================================================
# Tool Trust Policy
# =============================================================================
# Risk levels: safe, medium, needs_approval, dangerous
# - safe: auto-allowed in all modes
# - medium: needs approval in interactive mode
# - needs_approval: always requires approval
# - dangerous: denied unless explicitly allowed

[tools]
default_policy = "medium"

# Builtin tools - VIL-native development tools
[tools.allow]
# File operations
file_read = true
file_write = true
file_edit = true

# Search tools
glob = true
grep = true
search = true

# Development tools
cargo = true
git = true
bash = true

# Agent tools
vil_status = true
vil_knowledge = true
task_done = true
todo_write = true

# Advanced tools (require approval)
# spawn_subtask = "needs_approval"
# run_skill = "needs_approval"

# =============================================================================
# Memory & Context
# =============================================================================

[memory]
persist_path = ".vac/memory/vil_memory.db"
enable_episodic = true
enable_semantic = true

# =============================================================================
# Context Engine - Zero-Copy SHM
# =============================================================================

[context]
enable_shm = true
shm_pool_size_mb = 512
chunk_size = 512
chunk_overlap = 50
max_context_tokens = 8192

# =============================================================================
# Swarm Configuration
# =============================================================================

[swarm]
max_concurrent_agents = 4
enable_parallel = true

# =============================================================================
# Trace & Audit
# =============================================================================

[trace]
enable = true
output_path = ".vac/traces"
enable_signing = false

# =============================================================================
# VIL Workflow Definition (VWFD)
# =============================================================================

[vil]
binary_path = "vil"
min_version = ">=0.1.0"
vwfd_paths = ["./workflows/**/*.vwfd.yaml"]
dev_command = "vac vil dev"
checkpoint_interval_secs = 300

# =============================================================================
# Runtime & Isolation
# =============================================================================

[runtime]
enable = false
task_intent_mode = "monitor-only"
environment_mode = "host"
execution_environment = "host"
network_policy = "inherit"
max_concurrent_jobs = 2

# Optional isolation settings for production/operator use:
# container_runtime = "docker"
# container_image = "ghcr.io/your-org/vac-runtime:latest"
# allowed_mounts = ["."]
# allowed_env = ["KILO_API_KEY"]

# =============================================================================
# MCP Servers (Optional)
# =============================================================================
# Example MCP server configurations:
# [[mcp_servers]]
# name = "filesystem"
# transport.type = "stdio"
# transport.command = "npx"
# transport.args = ["-y", "@modelcontextprotocol/server-filesystem", "/"]
#
# Example MCP preset wiring (expanded into MCP servers at runtime):
# [[mcp_presets]]
# preset = "github"
# allowed_in_modes = ["trusted-networked"]
#
# [[mcp_presets]]
# preset = "jira"
# allowed_in_modes = ["trusted-networked"]
#
# [[mcp_presets]]
# preset = "ci"
# allowed_in_modes = ["trusted-networked"]
#
# [[mcp_servers]]
# name = "remote-knowledge"
# transport.type = "sse"
# transport.url = "https://mcp.example.com"
# trust_class = "remote_verified"
# allowed_in_modes = ["trusted-networked"]
#
# [mcp_servers.tls]
# ca_file = "/etc/ssl/custom-ca.pem"
# server_name = "mcp.example.com"


# =============================================================================
# Skills Directory
# =============================================================================
# Custom skills are loaded from .vac/skills/*.toml
"#;
        tokio::fs::write(&config_path, default_config).await?;
        println!("   📝 Created default config: .vac/config.toml");
    }

    let example_vwfd_path = project_root.join("workflows/example.vwfd.yaml");
    if !tokio::fs::try_exists(&example_vwfd_path).await? {
        let example_vwfd = r#"apiVersion: vil.vastar.io/v1
kind: VilServer
metadata:
  name: example-service
spec:
  workflows:
    - name: main
      description: Example workflow scaffold created by vac init
      steps:
        - id: validate
          handler: validate_input
  handlers:
    - name: validate_input
      execution: native
"#;
        tokio::fs::write(&example_vwfd_path, example_vwfd).await?;
        println!("   📝 Created example VWFD: workflows/example.vwfd.yaml");
    }

    // Create empty rulebook template if not exists
    let rules_path = vac_dir.join("rules.toml");
    if !tokio::fs::try_exists(&rules_path).await? {
        let rules_template = r#"# VAC Rulebook — team/repo constraints (overlay only)
# VIL semantic contracts from llm_knowledge/ always take precedence over these rules.

name = "Project Rules"

# Example conventions:
# [[conventions]]
# id = "no-unwrap"
# description = "Do not use .unwrap() in production code, use ? or proper error handling"
# severity = "warn"

# Example acceptance gates:
# [[acceptance_gates]]
# id = "tests-required"
# description = "New public functions must have at least one test"
# severity = "block"

# Example policies:
# [[policies]]
# id = "no-secrets"
# description = "Never commit API keys or secrets to source control"
# severity = "block"
"#;
        tokio::fs::write(&rules_path, rules_template).await?;
        println!("   📋 Created rulebook template: .vac/rules.toml");
    }

    // Add .vac to .gitignore if not already there
    let gitignore_path = project_root.join(".gitignore");
    if tokio::fs::try_exists(&gitignore_path).await? {
        let content = tokio::fs::read_to_string(&gitignore_path).await?;
        if !content.contains(".vac/") {
            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&gitignore_path)
                .await?;
            file.write_all(b"\n# VAC agent data\n.vac/\n").await?;
            println!("   📂 Added .vac/ to .gitignore");
        }
    }

    // Initialize engine and scan codebase
    println!("   🔍 Scanning codebase...");
    let mut engine = vac_core::VacEngine::new(project_root).await?;
    let warnings = engine.init().await?;
    for warning in warnings {
        eprintln!("Warning: {}", warning);
    }

    let status = engine.status().await?;
    println!("\n✓ VAC initialized successfully!");
    println!("   Session: {}", status.session_id);
    println!("   Project: {}", status.project_root.display());
    println!("\n   Run `vac auth login` if you have not saved your Kilo token yet.");
    println!("   Then run `vac interactive` to start coding.");

    Ok(())
}
