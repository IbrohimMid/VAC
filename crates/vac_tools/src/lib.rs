//! VAC Tools — Tool registry, execution router, MCP bridge, and sandbox.

pub mod approvals;
pub mod backup;
pub mod builtin;
pub mod error;
pub mod journal;
pub mod mcp;
pub mod privacy;
pub mod registry;
pub mod resource_limits;
pub mod result_spill;
pub mod router;
pub mod rust_analysis;
pub mod sandbox;
pub mod security;
pub mod skills;

pub use approvals::{ScopePolicy, ShellApprovalPolicy};
pub use error::ToolError;
pub use privacy::PrivacyVault;
pub use registry::{ToolDefinition, ToolRegistry, VilTool};
pub use result_spill::{maybe_spill_result, prune_spill_dir, PreviewStub};
pub use router::ToolRouter;
pub mod trust_gate;
