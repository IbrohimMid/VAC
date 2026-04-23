//! M4 — Unified TrustGate.
//!
//! Three consumer sites converge here:
//!
//! 1. **`vac_tools::router`** — tool dispatch, called per tool call
//!    before the tool's `execute` fires.
//! 2. **`vac_tools::mcp::client`** — MCP tool invocation gate.
//! 3. **Isolation spawn** — [`TrustGate::check_environment`] is the
//!    pure decision helper `vac_runtime::IsolationManager` calls
//!    before launching a command.
//!
//! ## Typed vs string API
//!
//! The typed [`EnvironmentMode`] + [`TrustClass`] enums are the
//! canonical surface; string-based construction is kept as a thin
//! adapter for the router call site that still reads
//! `context.environment_mode: String`. Every decision logs at
//! `info` target `vac_tools::trust_gate` so audit replays are
//! possible.

use tracing::info;
use vac_tool_core::ToolSpec;

use crate::mcp::McpTrustClass;

/// Typed environment mode. Matches the isolation surface exposed by
/// `vac_runtime::IsolationManager`. String labels live on
/// [`EnvironmentMode::label`] for audit rendering; round-trip via
/// [`EnvironmentMode::from_label`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvironmentMode {
    Host,
    Isolated,
    TrustedNetworked,
    RestrictedOffline,
}

impl EnvironmentMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Isolated => "isolated",
            Self::TrustedNetworked => "trusted-networked",
            Self::RestrictedOffline => "restricted-offline",
        }
    }

    /// Parse a string label. Unknown values degrade to `Host` so a
    /// typo in config never silently downgrades the gate. Typos are
    /// traced at `warn` so the operator sees the fallback.
    pub fn from_label(s: &str) -> Self {
        match s {
            "host" => Self::Host,
            "isolated" => Self::Isolated,
            "trusted-networked" | "trusted_networked" => Self::TrustedNetworked,
            "restricted-offline" | "restricted_offline" => Self::RestrictedOffline,
            other => {
                tracing::warn!(
                    target: "vac_tools::trust_gate",
                    input = other,
                    "unknown environment_mode label; falling back to Host"
                );
                Self::Host
            }
        }
    }
}

/// Trust class unified across "MCP server trust" + "local tool".
/// Wrapping `McpTrustClass` gives callers a single lens; `Local`
/// means "not from an MCP server".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustClass {
    Local,
    LocalTrusted,
    RemoteVerified,
    RemoteUntrusted,
}

impl From<Option<McpTrustClass>> for TrustClass {
    fn from(v: Option<McpTrustClass>) -> Self {
        match v {
            None => Self::Local,
            Some(McpTrustClass::LocalTrusted) => Self::LocalTrusted,
            Some(McpTrustClass::RemoteVerified) => Self::RemoteVerified,
            Some(McpTrustClass::RemoteUntrusted) => Self::RemoteUntrusted,
        }
    }
}

impl TrustClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::LocalTrusted => "local-trusted",
            Self::RemoteVerified => "remote-verified",
            Self::RemoteUntrusted => "remote-untrusted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GateDecision {
    Allow,
    Deny(String),
    NeedsApproval(String),
}

impl GateDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Deny(_))
    }
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Deny(r) | Self::NeedsApproval(r) => Some(r),
            Self::Allow => None,
        }
    }
}

/// Per-call gate input. Construct via helpers; do not share across
/// calls — the struct is tiny and keeping it per-call makes the
/// audit trail one-entry-per-decision.
#[derive(Debug, Clone)]
pub struct TrustContext<'a> {
    /// Kept as the original string form for backward-compat with
    /// callers that read `policy::ExecutionContext::environment_mode`.
    /// Gate logic uses `mode_typed()` so typos fall back to `Host`
    /// rather than silently granting more access.
    pub environment_mode: &'a str,
    /// MCP trust class string (`LocalTrusted`/`RemoteVerified`/
    /// `RemoteUntrusted`) or `None` for a local tool. Parsed into
    /// [`TrustClass`] at decision time.
    pub mcp_trust: Option<&'a str>,
}

impl<'a> TrustContext<'a> {
    pub fn host() -> Self {
        Self {
            environment_mode: "host",
            mcp_trust: None,
        }
    }

    fn mode_typed(&self) -> EnvironmentMode {
        EnvironmentMode::from_label(self.environment_mode)
    }

    fn trust_typed(&self) -> TrustClass {
        match self.mcp_trust {
            Some("LocalTrusted") | Some("local-trusted") => TrustClass::LocalTrusted,
            Some("RemoteVerified") | Some("remote-verified") => TrustClass::RemoteVerified,
            Some("RemoteUntrusted") | Some("remote-untrusted") => TrustClass::RemoteUntrusted,
            _ => TrustClass::Local,
        }
    }
}

pub struct TrustGate;

impl TrustGate {
    /// Evaluate a tool call. Decision rules (in order):
    /// 1. `RestrictedOffline` denies every remote MCP tool outright.
    /// 2. `Isolated` denies `LocalTrusted` MCP.
    /// 3. `TrustedNetworked`/`Isolated` deny `RemoteUntrusted` MCP.
    /// 4. `RemoteUntrusted` (when not already denied) → approval.
    /// 5. `RemoteVerified` → approval when the tool prompts operator.
    /// 6. Otherwise allow.
    pub fn check_tool(ctx: &TrustContext<'_>, spec: &ToolSpec) -> GateDecision {
        let mode = ctx.mode_typed();
        let trust = ctx.trust_typed();
        let decision = Self::check_tool_typed(mode, trust, spec);
        info!(
            target: "vac_tools::trust_gate",
            env = mode.label(),
            trust = trust.label(),
            tool = %spec.name,
            decision = ?decision,
            "trust gate tool decision",
        );
        decision
    }

    fn check_tool_typed(
        mode: EnvironmentMode,
        trust: TrustClass,
        spec: &ToolSpec,
    ) -> GateDecision {
        // 1. Restricted-offline denies any remote MCP.
        if mode == EnvironmentMode::RestrictedOffline
            && matches!(
                trust,
                TrustClass::RemoteVerified | TrustClass::RemoteUntrusted
            )
        {
            return GateDecision::Deny("restricted-offline denies remote MCP".into());
        }
        // 2. Isolated denies LocalTrusted MCP (the whole point of
        //    isolation is to revoke local-trusted access).
        if mode == EnvironmentMode::Isolated && trust == TrustClass::LocalTrusted {
            return GateDecision::Deny(
                "LocalTrusted MCP denied in isolated mode".into(),
            );
        }
        // 3. TrustedNetworked + Isolated explicitly deny
        //    RemoteUntrusted. (Host + RestrictedOffline handled
        //    elsewhere.)
        if trust == TrustClass::RemoteUntrusted
            && matches!(
                mode,
                EnvironmentMode::TrustedNetworked | EnvironmentMode::Isolated
            )
        {
            return GateDecision::Deny(
                "RemoteUntrusted MCP blocked in this environment".into(),
            );
        }
        // 4. RemoteUntrusted (on host) asks for approval.
        if trust == TrustClass::RemoteUntrusted {
            return GateDecision::NeedsApproval(format!(
                "remote-untrusted MCP tool '{}' requires per-call approval",
                spec.name,
            ));
        }
        // 5. RemoteVerified asks for approval only on non-safe tools.
        if trust == TrustClass::RemoteVerified && spec.permission.prompts_operator() {
            return GateDecision::NeedsApproval(format!(
                "remote-verified MCP tool '{}' is not operator-safe",
                spec.name,
            ));
        }
        GateDecision::Allow
    }

    /// Isolation-side gate. Called by `IsolationManager` before
    /// spawning a command. `RestrictedOffline` requires explicit
    /// approval for any subprocess; every other mode allows.
    pub fn check_environment(mode: EnvironmentMode) -> GateDecision {
        let decision = match mode {
            EnvironmentMode::RestrictedOffline => GateDecision::NeedsApproval(
                "restricted-offline spawn requires explicit approval".into(),
            ),
            _ => GateDecision::Allow,
        };
        info!(
            target: "vac_tools::trust_gate",
            env = mode.label(),
            decision = ?decision,
            "trust gate environment decision",
        );
        decision
    }

    /// String-adapter for callers that only have the raw label
    /// (e.g. `vac_runtime` config). Delegates to
    /// [`TrustGate::check_environment`] via
    /// [`EnvironmentMode::from_label`].
    pub fn check_environment_label(label: &str) -> GateDecision {
        Self::check_environment(EnvironmentMode::from_label(label))
    }

    /// MCP-client gate shortcut. Accepts the concrete
    /// `McpTrustClass` from the MCP config rather than forcing
    /// callers to stringify it.
    pub fn check_mcp_tool(
        mode: EnvironmentMode,
        mcp: McpTrustClass,
        spec: &ToolSpec,
    ) -> GateDecision {
        let trust = TrustClass::from(Some(mcp));
        let decision = Self::check_tool_typed(mode, trust, spec);
        info!(
            target: "vac_tools::trust_gate",
            env = mode.label(),
            trust = trust.label(),
            tool = %spec.name,
            decision = ?decision,
            "trust gate mcp decision",
        );
        decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_tool_core::{ToolPermissionClass, ToolSpec};

    fn safe_spec(name: &str) -> ToolSpec {
        ToolSpec::read_only(name, "t", serde_json::json!({"type": "object"}))
    }

    fn prompt_spec(name: &str) -> ToolSpec {
        let mut spec = ToolSpec::read_only(name, "t", serde_json::json!({"type": "object"}));
        spec.permission = ToolPermissionClass::AskEveryCall;
        spec
    }

    #[test]
    fn host_local_tool_allows() {
        let d = TrustGate::check_tool(&TrustContext::host(), &safe_spec("r"));
        assert_eq!(d, GateDecision::Allow);
    }

    #[test]
    fn remote_untrusted_on_host_prompts() {
        let ctx = TrustContext {
            environment_mode: "host",
            mcp_trust: Some("RemoteUntrusted"),
        };
        let d = TrustGate::check_tool(&ctx, &safe_spec("x"));
        assert!(matches!(d, GateDecision::NeedsApproval(_)));
    }

    #[test]
    fn remote_untrusted_on_isolated_denies() {
        let ctx = TrustContext {
            environment_mode: "isolated",
            mcp_trust: Some("RemoteUntrusted"),
        };
        let d = TrustGate::check_tool(&ctx, &safe_spec("x"));
        assert!(d.is_denied());
    }

    #[test]
    fn remote_verified_safe_tool_allows() {
        let ctx = TrustContext {
            environment_mode: "host",
            mcp_trust: Some("RemoteVerified"),
        };
        let d = TrustGate::check_tool(&ctx, &safe_spec("x"));
        assert_eq!(d, GateDecision::Allow);
    }

    #[test]
    fn remote_verified_risky_tool_prompts() {
        let ctx = TrustContext {
            environment_mode: "host",
            mcp_trust: Some("RemoteVerified"),
        };
        let d = TrustGate::check_tool(&ctx, &prompt_spec("w"));
        assert!(matches!(d, GateDecision::NeedsApproval(_)));
    }

    #[test]
    fn restricted_offline_denies_any_remote() {
        let ctx = TrustContext {
            environment_mode: "restricted-offline",
            mcp_trust: Some("RemoteVerified"),
        };
        assert!(TrustGate::check_tool(&ctx, &safe_spec("x")).is_denied());

        let ctx = TrustContext {
            environment_mode: "restricted-offline",
            mcp_trust: Some("RemoteUntrusted"),
        };
        assert!(TrustGate::check_tool(&ctx, &safe_spec("x")).is_denied());
    }

    #[test]
    fn isolated_denies_local_trusted_mcp() {
        let ctx = TrustContext {
            environment_mode: "isolated",
            mcp_trust: Some("LocalTrusted"),
        };
        assert!(TrustGate::check_tool(&ctx, &safe_spec("x")).is_denied());
    }

    #[test]
    fn environment_check_restricted_offline_needs_approval() {
        let d = TrustGate::check_environment(EnvironmentMode::RestrictedOffline);
        assert!(matches!(d, GateDecision::NeedsApproval(_)));
    }

    #[test]
    fn environment_check_other_modes_allow() {
        for m in [
            EnvironmentMode::Host,
            EnvironmentMode::Isolated,
            EnvironmentMode::TrustedNetworked,
        ] {
            assert_eq!(TrustGate::check_environment(m), GateDecision::Allow);
        }
    }

    #[test]
    fn environment_label_roundtrip_via_from_label() {
        for m in [
            EnvironmentMode::Host,
            EnvironmentMode::Isolated,
            EnvironmentMode::TrustedNetworked,
            EnvironmentMode::RestrictedOffline,
        ] {
            assert_eq!(EnvironmentMode::from_label(m.label()), m);
        }
    }

    #[test]
    fn from_label_typo_falls_back_to_host() {
        // Underscore variant should parse (forgiving).
        assert_eq!(
            EnvironmentMode::from_label("trusted_networked"),
            EnvironmentMode::TrustedNetworked,
        );
        // Unknown label must NOT silently become a stricter mode;
        // falls back to Host. A stricter fallback would lock out
        // valid configs on a typo.
        assert_eq!(EnvironmentMode::from_label("nonsense"), EnvironmentMode::Host);
    }

    #[test]
    fn mcp_shortcut_maps_trust_correctly() {
        assert!(matches!(
            TrustGate::check_mcp_tool(
                EnvironmentMode::Host,
                McpTrustClass::RemoteUntrusted,
                &safe_spec("x"),
            ),
            GateDecision::NeedsApproval(_),
        ));
        assert_eq!(
            TrustGate::check_mcp_tool(
                EnvironmentMode::Host,
                McpTrustClass::LocalTrusted,
                &safe_spec("x"),
            ),
            GateDecision::Allow,
        );
    }

    #[test]
    fn check_environment_label_adapter_works() {
        assert_eq!(
            TrustGate::check_environment_label("host"),
            GateDecision::Allow
        );
        assert!(matches!(
            TrustGate::check_environment_label("restricted-offline"),
            GateDecision::NeedsApproval(_),
        ));
    }
}
