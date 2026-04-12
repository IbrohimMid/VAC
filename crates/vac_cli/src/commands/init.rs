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

    // Create default config if not exists
    let config_path = vac_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = r#"# VAC Configuration
# See https://vastar.id/docs/vac/config for full reference

[llm]
default_provider = "anthropic"

[llm.providers.anthropic]
api_key_env = "KILO_API_KEY"
model = "kilo-auto/free"

[tools]
default_policy = "deny"

[tools.allow]
bash = true
file_write = true
file_edit = true
glob = true
grep = true
file_read = true
cargo = true
git = true
search = true
task_done = true
todo_write = true

[memory]
persist_path = ".vac/memory"
enable_episodic = true
enable_semantic = true

[context]
enable_shm = true
shm_pool_size_mb = 512

[trace]
enable = true
output_path = ".vac/traces"
"#;
        std::fs::write(&config_path, default_config)?;
        println!("   📝 Created default config: .vac/config.toml");
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
