//! `vac config` — Manage configuration.

use crate::ConfigAction;
use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, action: ConfigAction) -> anyhow::Result<()> {
    let config_path = project_root.join(".vac/config.toml");

    match action {
        ConfigAction::Show => {
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                println!("{}", content);
            } else {
                println!("No config found. Run `vac init` first.");
            }
        }
        ConfigAction::Set { key, value } => {
            println!("Setting {} = {}", key, value);
            println!("(Config set not yet implemented — edit .vac/config.toml directly)");
        }
        ConfigAction::AddProvider {
            name,
            model,
            api_key_env,
            base_url,
        } => {
            println!("Adding LLM provider: {}", name);
            println!("  Model: {}", model);
            println!("  API key env: {}", api_key_env);
            if let Some(url) = &base_url {
                println!("  Base URL: {}", url);
            }
            println!("(Provider add not yet implemented — edit .vac/config.toml directly)");
        }
    }

    Ok(())
}
