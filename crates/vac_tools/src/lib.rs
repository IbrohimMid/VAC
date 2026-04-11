//! VAC Tools — Tool registry, execution router, MCP bridge, and sandbox.

pub mod builtin;
pub mod error;
pub mod mcp;
pub mod registry;
pub mod router;
pub mod sandbox;
pub mod skills;

pub use error::ToolError;
pub use registry::{ToolDefinition, ToolRegistry};
pub use router::ToolRouter;
