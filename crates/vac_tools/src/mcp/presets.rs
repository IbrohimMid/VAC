use super::{McpServerConfig, McpTlsConfig, McpTransport, McpTrustClass};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

#[derive(Debug, Clone)]
pub struct McpServerPreset {
    pub preset: &'static str,
    pub name: &'static str,
    pub env_requirements: &'static [&'static str],
    pub transport: McpTransport,
    pub args: &'static [&'static str],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPresetInstanceConfig {
    pub preset: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub trust_class: Option<McpTrustClass>,
    #[serde(default)]
    pub tls: Option<McpTlsConfig>,
    #[serde(default)]
    pub approval_policy: Option<String>,
    #[serde(default)]
    pub allowed_in_modes: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

pub fn builtin_mcp_presets() -> Vec<McpServerPreset> {
    vec![
        McpServerPreset {
            preset: "github",
            name: "github",
            env_requirements: &["GITHUB_TOKEN"],
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-github".to_string(),
                ],
            },
            args: &["-y", "@modelcontextprotocol/server-github"],
        },
        McpServerPreset {
            preset: "jira",
            name: "jira",
            env_requirements: &["JIRA_BASE_URL", "JIRA_EMAIL", "JIRA_API_TOKEN"],
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-jira".to_string(),
                ],
            },
            args: &["-y", "@modelcontextprotocol/server-jira"],
        },
        McpServerPreset {
            preset: "ci",
            name: "ci",
            env_requirements: &["GITHUB_TOKEN"],
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-github-actions".to_string(),
                ],
            },
            args: &["-y", "@modelcontextprotocol/server-github-actions"],
        },
    ]
}

pub fn resolve_mcp_presets(
    presets: &[McpPresetInstanceConfig],
) -> (Vec<McpServerConfig>, Vec<String>) {
    let builtins = builtin_mcp_presets();
    let mut out = Vec::new();
    let mut warnings = Vec::new();

    for instance in presets {
        let Some(preset) = builtins
            .iter()
            .find(|p| p.preset.eq_ignore_ascii_case(instance.preset.as_str()))
        else {
            warnings.push(format!(
                "MCP preset '{}' tidak dikenal; lewati aktivasi",
                instance.preset
            ));
            continue;
        };

        let mut missing = Vec::new();
        let mut env_map = instance.env.clone();
        for var in preset.env_requirements {
            match env::var(var) {
                Ok(v) if !v.trim().is_empty() => {
                    env_map.insert((*var).to_string(), v);
                }
                _ => missing.push((*var).to_string()),
            }
        }

        if !missing.is_empty() {
            warnings.push(format!(
                "MCP preset '{}' tidak aktif (env missing: {})",
                preset.preset,
                missing.join(", ")
            ));
            continue;
        }

        let name = instance
            .name
            .clone()
            .unwrap_or_else(|| preset.name.to_string());

        out.push(McpServerConfig {
            name,
            transport: preset.transport.clone(),
            env: env_map,
            trust_class: instance.trust_class,
            tls: instance.tls.clone(),
            approval_policy: instance.approval_policy.clone(),
            allowed_in_modes: instance.allowed_in_modes.clone(),
        });
    }

    (out, warnings)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn unset(vars: &[&str]) {
        for v in vars {
            unsafe {
                env::remove_var(v);
            }
        }
    }

    #[test]
    fn unknown_preset_emits_warning() {
        let _g = ENV_LOCK.lock().unwrap();
        let (servers, warnings) = resolve_mcp_presets(&[McpPresetInstanceConfig {
            preset: "nope".to_string(),
            name: None,
            trust_class: None,
            tls: None,
            approval_policy: None,
            allowed_in_modes: vec![],
            env: HashMap::new(),
        }]);
        assert!(servers.is_empty());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn missing_env_skips_activation() {
        let _g = ENV_LOCK.lock().unwrap();
        unset(&["GITHUB_TOKEN"]);
        let (servers, warnings) = resolve_mcp_presets(&[McpPresetInstanceConfig {
            preset: "github".to_string(),
            name: None,
            trust_class: None,
            tls: None,
            approval_policy: None,
            allowed_in_modes: vec![],
            env: HashMap::new(),
        }]);
        assert!(servers.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("env missing"));
    }

    #[test]
    fn env_present_builds_server_config() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("GITHUB_TOKEN", "token");
        }
        let (servers, warnings) = resolve_mcp_presets(&[McpPresetInstanceConfig {
            preset: "github".to_string(),
            name: Some("github2".to_string()),
            trust_class: Some(McpTrustClass::LocalTrusted),
            tls: None,
            approval_policy: Some("prompt".to_string()),
            allowed_in_modes: vec!["host".to_string()],
            env: HashMap::new(),
        }]);
        assert!(warnings.is_empty());
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "github2");
        assert_eq!(
            servers[0].env.get("GITHUB_TOKEN").map(String::as_str),
            Some("token")
        );
    }
}
