//! MCP (Model Context Protocol) management commands

use anyhow::Result;
use std::path::Path;
use vac_core::VacConfig;
use vac_tools::mcp::{McpTransport, McpTrustClass};

pub fn list(project_root: &Path) -> Result<()> {
    let config = VacConfig::load_with_fallback(project_root)?;

    let mut servers = config.mcp_servers.clone().unwrap_or_default();
    let (preset_servers, preset_warnings) =
        vac_tools::mcp::resolve_mcp_presets(&config.mcp_presets);
    let has_preset_warnings = !preset_warnings.is_empty();
    servers.extend(preset_servers);

    if servers.is_empty() {
        println!("No MCP servers configured.");
        for w in &preset_warnings {
            println!("  ⚠️  {}", w);
        }
        return Ok(());
    }

    println!("MCP Servers:");
    println!();

    for w in &preset_warnings {
        println!("  ⚠️  {}", w);
    }
    if has_preset_warnings {
        println!();
    }

    for server in &servers {
        let trust_badge = match server.trust_class {
            Some(McpTrustClass::LocalTrusted) => "🟢 local-trusted",
            Some(McpTrustClass::RemoteVerified) => "🟡 remote-verified",
            Some(McpTrustClass::RemoteUntrusted) => "🔴 remote-untrusted",
            None => "⚪ unspecified",
        };

        println!("  {} [{}]", server.name, trust_badge);

        match &server.transport {
            McpTransport::Stdio { command, args } => {
                println!("    Transport: stdio");
                let args_str = args.join(" ");
                println!("    Command: {} {}", command, args_str);
            }
            McpTransport::Sse { url } => {
                println!("    Transport: sse");
                println!("    URL: {}", url);
            }
        }

        if !server.env.is_empty() {
            println!("    Environment: {} vars", server.env.len());
        }

        if let Some(policy) = &server.approval_policy {
            println!("    Approval policy: {}", policy);
        }

        if !server.allowed_in_modes.is_empty() {
            println!(
                "    Allowed in modes: {}",
                server.allowed_in_modes.join(", ")
            );
        }

        // Diagnostic Warnings
        let mut warnings = Vec::new();
        if server.trust_class.is_none() {
            let default_trust = match server.transport.is_remote() {
                true => "remote-untrusted",
                false => "local-trusted",
            };
            warnings.push(format!(
                "Trust class is unspecified, defaulting to {}",
                default_trust
            ));
        }

        let is_untrusted = server.effective_trust_class() == McpTrustClass::RemoteUntrusted;
        let has_auto_policy = matches!(
            server.approval_policy.as_deref(),
            Some("auto_all") | Some("auto_safe")
        );

        if is_untrusted && has_auto_policy {
            warnings.push("WARNING: Untrusted remote server has an auto-approval policy! This is a security risk.".to_string());
        }

        for warning in warnings {
            println!("    ⚠️  {}", warning);
        }

        println!();
    }

    Ok(())
}

pub async fn status(project_root: &Path) -> Result<()> {
    let config = VacConfig::load_with_fallback(project_root)?;

    let mut servers = config.mcp_servers.clone().unwrap_or_default();
    let (preset_servers, preset_warnings) =
        vac_tools::mcp::resolve_mcp_presets(&config.mcp_presets);
    servers.extend(preset_servers);

    println!("MCP Status:");
    println!();
    println!("  Total servers: {}", servers.len());
    println!();

    if !preset_warnings.is_empty() {
        println!("  Preset warnings:");
        for w in &preset_warnings {
            println!("    ⚠️  {}", w);
        }
        println!();
    }

    let local_count = servers
        .iter()
        .filter(|s| matches!(s.trust_class, Some(McpTrustClass::LocalTrusted)))
        .count();
    let verified_count = servers
        .iter()
        .filter(|s| matches!(s.trust_class, Some(McpTrustClass::RemoteVerified)))
        .count();
    let untrusted_count = servers
        .iter()
        .filter(|s| matches!(s.trust_class, Some(McpTrustClass::RemoteUntrusted)))
        .count();
    let unspecified_count = servers.iter().filter(|s| s.trust_class.is_none()).count();

    println!("  Trust distribution:");
    println!("    🟢 Local Trusted: {}", local_count);
    println!("    🟡 Remote Verified: {}", verified_count);
    println!("    🔴 Remote Untrusted: {}", untrusted_count);
    if unspecified_count > 0 {
        println!("    ⚪ Unspecified: {}", unspecified_count);
    }

    println!();
    println!("  Connection Status:");
    for server in &servers {
        let state = vac_tools::mcp::probe_mcp_server(server).await;
        let status_badge = if state.is_connected() {
            "✅ connected"
        } else {
            "❌ unreachable"
        };

        println!("    {} [{}]", server.name, status_badge);
        if let vac_tools::mcp::McpConnectionStatus::Unreachable(reason) = state.status {
            println!("      Reason: {}", reason);
        }
    }

    Ok(())
}
