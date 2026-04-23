//! `vac skills` subcommands — list and show bundled skills.
//!
//! Runs entirely off the in-process `SkillRegistry` (bundled set) so
//! the command is useful in headless contexts with no network, no
//! LLM, no MCP server running.

use std::sync::Arc;

use vac_skill::SkillRegistry;

pub async fn execute_list() -> anyhow::Result<()> {
    let reg = Arc::new(SkillRegistry::new());
    vac_skill::bundled::register_bundled(&reg).await?;
    let entries = reg.describe_all().await;
    println!("{:<10}  {}", "NAME", "DESCRIPTION");
    println!("{:-<10}  {:-<60}", "", "");
    for e in entries {
        let mut desc = e.description;
        if desc.len() > 70 {
            desc.truncate(67);
            desc.push_str("...");
        }
        println!("{:<10}  {}", e.name, desc);
    }
    Ok(())
}

pub async fn execute_show(name: String) -> anyhow::Result<()> {
    let reg = Arc::new(SkillRegistry::new());
    vac_skill::bundled::register_bundled(&reg).await?;
    let all = reg.describe_all().await;
    let entry = all
        .into_iter()
        .find(|e| e.name == name)
        .ok_or_else(|| anyhow::anyhow!("skill not found: {name}"))?;
    println!("name:        {}", entry.name);
    println!("description: {}", entry.description);
    println!("schema:");
    println!("{}", serde_json::to_string_pretty(&entry.schema)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_builds_without_error() {
        execute_list().await.unwrap();
    }

    #[tokio::test]
    async fn show_known_skill_succeeds() {
        execute_show("verify".into()).await.unwrap();
    }

    #[tokio::test]
    async fn show_unknown_skill_errors() {
        let err = execute_show("does-not-exist".into())
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("skill not found"));
    }
}
