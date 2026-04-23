//! Scoped MCP server config. A server named `notion` may be defined
//! at user scope (`~/.vac/mcp.toml`) AND overridden at project scope
//! (`<project>/.vac/mcp.toml`). `resolve_config` picks the most
//! specific scope per server, deterministically.

use serde::{Deserialize, Serialize};

use crate::transport::McpTransportKind;

/// Which config file the entry came from. Higher variants win when
/// the same server name appears at multiple scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum McpConfigScope {
    /// System-wide install default.
    System,
    /// User-level config (`~/.vac/mcp.toml`).
    User,
    /// Project-local override (`<project>/.vac/mcp.toml`).
    Project,
    /// Inline in the current session (e.g. `/mcp add ...`).
    Session,
}

impl McpConfigScope {
    pub fn label(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Project => "project",
            Self::Session => "session",
        }
    }
}

/// Minimal server-config record. Concrete transport details
/// (command/args, URL, TLS) live on consumer types that embed this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransportKind,
    pub scope: McpConfigScope,
    /// True if the operator has disabled this entry without deleting it.
    #[serde(default)]
    pub disabled: bool,
    /// Opaque adapter-specific payload (command+args, URL, headers).
    /// The core doesn't interpret it — consumers deserialize their
    /// own richer type from this field.
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// For a given server name, pick the config entry with the highest
/// scope (`Session > Project > User > System`). Returns `None` if no
/// entry matches.
pub fn resolve_config<'a>(
    name: &str,
    candidates: &'a [McpServerConfig],
) -> Option<&'a McpServerConfig> {
    candidates
        .iter()
        .filter(|c| c.name == name)
        .max_by_key(|c| c.scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(name: &str, scope: McpConfigScope) -> McpServerConfig {
        McpServerConfig {
            name: name.into(),
            transport: McpTransportKind::Stdio,
            scope,
            disabled: false,
            extra: serde_json::Value::Null,
        }
    }

    #[test]
    fn project_wins_over_user() {
        let pool = vec![
            cfg("notion", McpConfigScope::User),
            cfg("notion", McpConfigScope::Project),
        ];
        let picked = resolve_config("notion", &pool).unwrap();
        assert_eq!(picked.scope, McpConfigScope::Project);
    }

    #[test]
    fn session_wins_over_all() {
        let pool = vec![
            cfg("n", McpConfigScope::System),
            cfg("n", McpConfigScope::User),
            cfg("n", McpConfigScope::Project),
            cfg("n", McpConfigScope::Session),
        ];
        assert_eq!(
            resolve_config("n", &pool).unwrap().scope,
            McpConfigScope::Session
        );
    }

    #[test]
    fn unknown_name_is_none() {
        let pool = vec![cfg("a", McpConfigScope::User)];
        assert!(resolve_config("b", &pool).is_none());
    }
}
