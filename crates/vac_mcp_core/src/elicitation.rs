//! W4.1 — MCP elicitation handler.
//!
//! MCP servers can ask the client to prompt the operator for a value
//! (URL-form, text input, confirmation). The spec carries this as an
//! `elicitation/request` sent from server → client mid-session. VAC
//! previously had no handler — the message was logged and dropped,
//! stalling any server that depends on elicitation for auth flows
//! or multi-step provisioning.
//!
//! This module defines the handler **contract** plus an always-safe
//! default (`UnsupportedElicitationHandler`) that replies with the
//! spec's `action: "cancel"`. Concrete interactive handlers (TUI
//! prompts, browser redirects) plug in at the driver layer so
//! `vac_mcp_core` stays transport-free.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{McpCoreError, McpCoreResult};

/// Kinds of elicitation the MCP spec defines. `#[non_exhaustive]`
/// because upstream can land new variants at any point; consumers
/// must cover a default arm.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ElicitationRequest {
    /// Open a URL in the operator's browser and wait for the server
    /// to declare completion. Used by OAuth / device-code flows.
    OpenUrl {
        url: String,
        #[serde(default)]
        prompt: Option<String>,
    },
    /// Ask the operator for a short text value.
    Text {
        prompt: String,
        #[serde(default)]
        default: Option<String>,
    },
    /// Ask for yes/no confirmation.
    Confirm { prompt: String },
}

/// Result the client ships back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ElicitationResult {
    /// Operator completed the prompt. `values` is a free-form map so
    /// URL flows can return tokens, text flows a single entry, etc.
    Accepted {
        #[serde(default)]
        values: HashMap<String, String>,
    },
    /// Operator explicitly declined.
    Declined {
        #[serde(default)]
        reason: Option<String>,
    },
    /// Handler is unavailable or the operator ignored the prompt.
    Cancelled,
}

impl ElicitationResult {
    pub fn accepted_with(values: HashMap<String, String>) -> Self {
        Self::Accepted { values }
    }

    pub fn declined(reason: impl Into<String>) -> Self {
        Self::Declined {
            reason: Some(reason.into()),
        }
    }

    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }
}

/// The handler contract. Drivers plug in via
/// [`crate::state::McpConnection::attach_elicitation_handler`]; the
/// default for every connection is [`UnsupportedElicitationHandler`].
#[async_trait]
pub trait ElicitationHandler: Send + Sync + std::fmt::Debug {
    async fn handle(&self, request: ElicitationRequest) -> McpCoreResult<ElicitationResult>;
}

/// Safe default. Replies `Cancelled` so a server that issues an
/// elicitation against a non-interactive client gracefully times
/// out instead of hanging the session.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedElicitationHandler;

#[async_trait]
impl ElicitationHandler for UnsupportedElicitationHandler {
    async fn handle(&self, _request: ElicitationRequest) -> McpCoreResult<ElicitationResult> {
        Ok(ElicitationResult::Cancelled)
    }
}

/// Error-returning handler for tests that want to assert the
/// failure path.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailingElicitationHandler;

#[async_trait]
impl ElicitationHandler for FailingElicitationHandler {
    async fn handle(&self, _request: ElicitationRequest) -> McpCoreResult<ElicitationResult> {
        Err(McpCoreError::Protocol(
            "elicitation handler deliberately failing (test fixture)".into(),
        ))
    }
}

/// G1c — runtime-side registry that lets a driver attach an
/// [`ElicitationHandler`] for the lifetime of an MCP connection
/// without polluting the serializable [`crate::state::McpConnection`]
/// record. Kept separate so tests and transport layers can clone
/// the `Arc<dyn ElicitationHandler>` freely while the state record
/// stays pure data.
///
/// A connection-scoped registry is the chosen shape (not a global)
/// so multi-tenant hosts with per-session handlers keep their own
/// isolation. Drivers construct one registry per live MCP session,
/// attach their handler after the `Connected` transition, and call
/// [`McpElicitationRegistry::dispatch`] when an
/// `elicitation/request` JSON-RPC arrives on the session transport.
/// When no handler is attached the registry falls back to
/// [`UnsupportedElicitationHandler`] — the same
/// answer-Cancelled-so-the-server-doesn't-hang contract the spec
/// has always had.
#[derive(Clone, Default, Debug)]
pub struct McpElicitationRegistry {
    handler: Option<std::sync::Arc<dyn ElicitationHandler>>,
}

impl McpElicitationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a handler. Replaces any previously-attached handler
    /// for this registry (caller's responsibility — a live session
    /// should never swap handlers mid-flight).
    pub fn attach_elicitation_handler(&mut self, handler: std::sync::Arc<dyn ElicitationHandler>) {
        self.handler = Some(handler);
    }

    pub fn has_handler(&self) -> bool {
        self.handler.is_some()
    }

    /// Dispatch a single `elicitation/request`. Falls back to
    /// [`UnsupportedElicitationHandler`] when nothing is attached
    /// so the server always sees a well-formed response.
    pub async fn dispatch(&self, request: ElicitationRequest) -> McpCoreResult<ElicitationResult> {
        match &self.handler {
            Some(h) => h.handle(request).await,
            None => UnsupportedElicitationHandler.handle(request).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_handler_cancels() {
        let h = UnsupportedElicitationHandler;
        let res = h
            .handle(ElicitationRequest::Confirm {
                prompt: "ok?".into(),
            })
            .await
            .unwrap();
        assert_eq!(res, ElicitationResult::Cancelled);
    }

    #[tokio::test]
    async fn failing_handler_surfaces_protocol_error() {
        let h = FailingElicitationHandler;
        let err = h
            .handle(ElicitationRequest::Text {
                prompt: "val".into(),
                default: None,
            })
            .await
            .unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("elicitation"));
    }

    #[test]
    fn accepted_helpers() {
        let mut values = HashMap::new();
        values.insert("token".into(), "abc".into());
        let r = ElicitationResult::accepted_with(values.clone());
        assert!(r.is_accepted());
        match r {
            ElicitationResult::Accepted { values: v } => {
                assert_eq!(v.get("token"), Some(&"abc".to_string()));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn declined_helper_wraps_reason() {
        let r = ElicitationResult::declined("operator said no");
        assert!(!r.is_accepted());
        match r {
            ElicitationResult::Declined { reason } => {
                assert_eq!(reason.as_deref(), Some("operator said no"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn request_serde_roundtrips() {
        let req = ElicitationRequest::OpenUrl {
            url: "https://example.test/auth".into(),
            prompt: Some("authorise".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ElicitationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn result_serde_cancelled_roundtrip() {
        let r = ElicitationResult::Cancelled;
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("cancelled"));
        let back: ElicitationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }
}
