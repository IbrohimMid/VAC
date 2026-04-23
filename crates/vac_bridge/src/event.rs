//! Events that flow across the bridge. Kept JSON-tagged so stdio / WS
//! transports can ship them verbatim.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client → VAC messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum InboundEvent {
    /// First message after connect. Carries client id + protocol version.
    Hello {
        client: String,
        protocol_version: u16,
    },
    /// Operator-submitted prompt text for the attached session.
    Submit { text: String },
    /// Response to a previously-sent permission request.
    PermissionResponse {
        request_id: Uuid,
        allow: bool,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Client wants to gracefully detach.
    Detach,
}

/// VAC → client messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum OutboundEvent {
    /// Reply to `Hello` — handshake accepted.
    Welcome {
        session_id: Uuid,
        server_version: String,
    },
    /// Streaming chunk from the current submit (assistant text).
    Chunk { text: String },
    /// The engine wants a permission decision before proceeding.
    PermissionRequest {
        request_id: Uuid,
        tool: String,
        summary: String,
    },
    /// Submit finished cleanly.
    SubmitFinished,
    /// Submit aborted — reason for the client to display.
    SubmitAborted { reason: String },
    /// Error surface — bridge encountered a protocol or I/O problem.
    Error { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_hello_roundtrips() {
        let ev = InboundEvent::Hello {
            client: "vscode-vac".into(),
            protocol_version: 1,
        };
        let s = serde_json::to_string(&ev).unwrap();
        let back: InboundEvent = serde_json::from_str(&s).unwrap();
        match back {
            InboundEvent::Hello {
                client,
                protocol_version,
            } => {
                assert_eq!(client, "vscode-vac");
                assert_eq!(protocol_version, 1);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn outbound_permission_request_tag_stable() {
        let ev = OutboundEvent::PermissionRequest {
            request_id: Uuid::nil(),
            tool: "file_write".into(),
            summary: "overwrite /etc/hosts".into(),
        };
        let v: serde_json::Value = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["kind"], "permission_request");
    }
}
