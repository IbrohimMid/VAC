//! VAC Tools — Tool registry, execution router, MCP bridge, and sandbox.

pub mod approvals;
pub mod builtin;
pub mod error;
pub mod journal;
pub mod mcp;
pub mod privacy;
pub mod registry;
pub mod router;
pub mod sandbox;
pub mod skills;

pub use approvals::{ScopePolicy, ShellApprovalPolicy};
pub use error::ToolError;
pub use privacy::PrivacyVault;
pub use registry::{ToolDefinition, ToolRegistry, VilTool};
pub use router::ToolRouter;
