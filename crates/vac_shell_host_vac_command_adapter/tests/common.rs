use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec};
use vac_shell_host_vac_command_adapter::{AdapterCommandSpec, AdapterConfig};

pub fn cmd(id: &str, slash: &str) -> ShellCommandSpec {
    ShellCommandSpec {
        id: id.to_string(),
        slash: slash.to_string(),
        title: format!("{slash} title"),
        description: format!("{slash} description"),
        kind: ShellCommandKind::PromptTemplate,
        palette_visible: true,
        shortcut: None,
        category: None,
        aliases: vec![],
        keywords: vec![],
        disabled_reason: None,
    }
}

#[allow(dead_code)]
pub fn mapped(root: std::path::PathBuf) -> AdapterConfig {
    AdapterConfig::new(root).with_command(AdapterCommandSpec::new(
        "memorize",
        "/memorize",
        "Memorize the current operator context.",
    ))
}
