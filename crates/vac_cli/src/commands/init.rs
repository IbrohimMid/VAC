//! `vac init` — Initialize project context.

use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, force: bool) -> anyhow::Result<()> {
    let vac_dir = project_root.join(".vac");

    if vac_dir.exists() && !force {
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
    std::fs::create_dir_all(vac_dir.join("sessions"))?;
    std::fs::create_dir_all(vac_dir.join("memory"))?;
    std::fs::create_dir_all(vac_dir.join("traces"))?;
    std::fs::create_dir_all(vac_dir.join("cache"))?;
    std::fs::create_dir_all(vac_dir.join("skills"))?;

    // Create default config if not exists
    let config_path = vac_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = r#"# VAC Configuration
# VIL-Native Autonomous Development Agent
# See https://vastar.id/docs/vac/config for full reference

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
persist_path = ".vac/memory"
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
# MCP Servers (Optional)
# =============================================================================
# Example MCP server configurations:
# [[mcp_servers]]
# name = "filesystem"
# transport.type = "stdio"
# transport.command = "npx"
# transport.args = ["-y", "@modelcontextprotocol/server-filesystem", "/"]

# =============================================================================
# Skills Directory
# =============================================================================
# Custom skills are loaded from .vac/skills/*.toml
"#;
        std::fs::write(&config_path, default_config)?;
        println!("   📝 Created default config: .vac/config.toml");
    }

    // Create empty rulebook template if not exists
    let rules_path = vac_dir.join("rules.toml");
    if !rules_path.exists() {
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
        std::fs::write(&rules_path, rules_template)?;
        println!("   📋 Created rulebook template: .vac/rules.toml");
    }

    // Add .vac to .gitignore if not already there
    let gitignore_path = project_root.join(".gitignore");
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if !content.contains(".vac/") {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&gitignore_path)?;
            use std::io::Write;
            writeln!(file, "\n# VAC agent data\n.vac/")?;
            println!("   📂 Added .vac/ to .gitignore");
        }
    }

    // Initialize engine and scan codebase
    println!("   🔍 Scanning codebase...");
    let mut engine = vac_core::VacEngine::new(project_root).await?;
    engine.init().await?;

    let status = engine.status().await?;
    println!("\n✓ VAC initialized successfully!");
    println!("   Session: {}", status.session_id);
    println!("   Project: {}", status.project_root.display());
    println!("\n   Run `vac auth login` if you have not saved your Kilo token yet.");
    println!("   Then run `vac interactive` to start coding.");

    Ok(())
}
