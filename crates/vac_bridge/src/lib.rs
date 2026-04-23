//! VAC Bridge — the layer that lets a remote driver (desktop app,
//! IDE extension, bridge daemon) operate a VAC session as if it were
//! a local TUI.
//!
//! Responsibilities split into three modules:
//!
//! - [`session`] — `RemoteSession` handle (id, handshake state,
//!   inbound/outbound event channels).
//! - [`permission`] — `PermissionMediator` trait: bridge forwards a
//!   request, operator responds, bridge relays.
//! - [`acp`] — Agent Client Protocol server skeleton (transport-free;
//!   drivers wire stdio or WebSocket).
//!
//! The crate is deliberately transport-free: it owns the *semantics*
//! of remote operation, not the bytes on the wire. `vac_cli` binds
//! it to stdio for `vac acp serve`; future driver crates can bind it
//! to WebSocket / gRPC / loopback.

pub mod acp;
pub mod auth;
pub mod capacity_wake;
pub mod error;
pub mod event;
pub mod permission;
pub mod session;

pub use acp::{AcpHandshake, AcpServer};
pub use auth::{jwt, oauth};
pub use capacity_wake::{CapacityError, CapacityWake, DEFAULT_MAX_DEPTH};
pub use error::{BridgeError, BridgeResult};
pub use event::{InboundEvent, OutboundEvent};
pub use permission::{
    PermissionDecision, PermissionMediator, PermissionRequest, StaticAllowMediator, StdioPermissionMediator,
};
pub use session::{RemoteSession, RemoteSessionHandle, SessionAttachState};
