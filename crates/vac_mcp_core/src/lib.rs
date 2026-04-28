//! VAC MCP Core — transport kinds, scoped config resolution, and the
//! connection state machine for MCP (Model Context Protocol) servers.
//!
//! This crate is deliberately transport-free: it does not depend on
//! reqwest, hyper, or stdio plumbing. Consumers (`vac_tools::mcp`,
//! `vac_bridge`) wire the concrete JSON-RPC / SSE clients. The
//! purpose of the crate is to own the *model* — what kinds of
//! servers exist, how their config is scoped, and what states a
//! connection can be in — so both the interactive runtime and the
//! headless/bridge paths see the same semantics.

pub mod channel;
pub mod config;
pub mod elicitation;
pub mod error;
pub mod state;
pub mod transport;

pub use channel::ChannelAcl;
pub use config::{McpConfigScope, McpServerConfig, resolve_config};
pub use elicitation::{
    ElicitationHandler, ElicitationRequest, ElicitationResult, FailingElicitationHandler,
    McpElicitationRegistry, UnsupportedElicitationHandler,
};
pub use error::{McpCoreError, McpCoreResult};
pub use state::{McpConnection, McpConnectionState, StateTransition};
pub use transport::McpTransportKind;
