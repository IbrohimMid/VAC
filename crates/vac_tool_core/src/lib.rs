//! VAC Tool Core — the formal contract every tool in `vac_tools`,
//! runtime, and future subsystems consumes.
//!
//! This crate intentionally owns **only types and traits**. It has no
//! I/O, no tokio, no tool implementations. Downstream crates (e.g.
//! `vac_tools`, `vac_session_engine`, `vac_bridge`) import these types
//! to agree on shape without circular dependencies.
//!
//! Design rationale — derived from the Claude Code `Tool.ts` review:
//! a tool is a first-class product object, not a naked function. It
//! carries schema, permission class, concurrency class, render hints,
//! and produces a typed result envelope so TUI, transcript, trace, and
//! MCP surfaces can all render it uniformly.

pub mod capability;
pub mod permission;
pub mod render;
pub mod result;
pub mod spec;

pub use capability::ToolCapability;
pub use permission::ToolPermissionClass;
pub use render::ToolRenderHints;
pub use result::{ToolResultEnvelope, ToolResultKind};
pub use spec::ToolSpec;
