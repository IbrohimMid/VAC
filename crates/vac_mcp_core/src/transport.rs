//! Transport taxonomy. The concrete I/O lives in consumers; this
//! crate just names the kinds so every caller agrees on a vocabulary.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum McpTransportKind {
    /// Spawn a local subprocess and speak JSON-RPC over stdio.
    /// Highest trust; the subprocess runs as the operator.
    Stdio,
    /// Connect to a remote SSE endpoint. Trust depends on the URL
    /// scheme, TLS presence, and whether the host is loopback.
    Sse,
    /// Connect to a WebSocket endpoint (optional, off by default).
    WebSocket,
}

impl McpTransportKind {
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Stdio)
    }

    /// Wire-label suitable for logs + status bars.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Sse => "sse",
            Self::WebSocket => "websocket",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_is_local_others_are_not() {
        assert!(McpTransportKind::Stdio.is_local());
        assert!(!McpTransportKind::Sse.is_local());
        assert!(!McpTransportKind::WebSocket.is_local());
    }

    #[test]
    fn transport_kind_roundtrips_through_json() {
        for k in [
            McpTransportKind::Stdio,
            McpTransportKind::Sse,
            McpTransportKind::WebSocket,
        ] {
            let s = serde_json::to_string(&k).unwrap();
            let back: McpTransportKind = serde_json::from_str(&s).unwrap();
            assert_eq!(back, k);
        }
    }
}
