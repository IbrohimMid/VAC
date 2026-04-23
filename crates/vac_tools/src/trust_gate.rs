use vac_tool_core::{ToolSpec, ToolPermissionClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    Allow,
    Deny(String),
    NeedsApproval(String),
}

pub struct TrustContext<'a> {
    pub environment_mode: &'a str,
    pub mcp_trust: Option<&'a str>,
}

pub struct TrustGate;

impl TrustGate {
    pub fn check_tool(ctx: &TrustContext, spec: &ToolSpec) -> GateDecision {
        if ctx.environment_mode == "restricted-offline" {
            if let Some(trust) = ctx.mcp_trust {
                if trust == "RemoteVerified" || trust == "RemoteUntrusted" {
                    return GateDecision::Deny("restricted-offline denies remote MCP".into());
                }
            }
        }
        
        if let Some(trust) = ctx.mcp_trust {
            if trust == "LocalTrusted" && ctx.environment_mode == "isolated" {
                return GateDecision::Deny("LocalTrusted MCP denied in isolated mode".into());
            }
            if trust == "RemoteUntrusted" && (ctx.environment_mode == "trusted-networked" || ctx.environment_mode == "isolated") {
                return GateDecision::Deny("RemoteUntrusted MCP blocked in this environment".into());
            }
            
            if trust == "RemoteUntrusted" {
                return GateDecision::NeedsApproval("RemoteUntrusted MCP requires per-call approval".into());
            }
            if trust == "RemoteVerified" && spec.permission.prompts_operator() {
                return GateDecision::NeedsApproval("RemoteVerified MCP requires approval for non-safe tools".into());
            }
        }

        GateDecision::Allow
    }
}
