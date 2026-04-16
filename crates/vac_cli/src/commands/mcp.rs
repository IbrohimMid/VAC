//! MCP (Model Context Protocol) management commands

use anyhow::Result;
use std::path::Path;
use vac_core::VacConfig;
use vac_tools::mcp::{McpTrustClass, McpTransport};

pub fn list(project_root: &Path) -> Result<()> {
    let config = VacConfig::load_with_fallback(project_root)?;
    
    let servers = match &config.mcp_servers {
        Some(servers) if !servers.is_empty() => servers,
        _ => {
            println!("No MCP servers configured.");
            return Ok(());
        }
    };
    
    println!("MCP Servers:");
    println!();
    
    for server in servers {
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
            println!("    Allowed in modes: {}", server.allowed_in_modes.join(", "));
        }
        
        println!();
    }
    
    Ok(())
}

pub fn status(project_root: &Path) -> Result<()> {
    let config = VacConfig::load_with_fallback(project_root)?;
    
    let servers = match &config.mcp_servers {
        Some(servers) => servers,
        None => {
            println!("MCP Status:");
            println!();
            println!("  Total servers: 0");
            return Ok(());
        }
    };
    
    println!("MCP Status:");
    println!();
    println!("  Total servers: {}", servers.len());
    println!();
    
    let local_count = servers.iter()
        .filter(|s| matches!(s.trust_class, Some(McpTrustClass::LocalTrusted)))
        .count();
    let verified_count = servers.iter()
        .filter(|s| matches!(s.trust_class, Some(McpTrustClass::RemoteVerified)))
        .count();
    let untrusted_count = servers.iter()
        .filter(|s| matches!(s.trust_class, Some(McpTrustClass::RemoteUntrusted)))
        .count();
    let unspecified_count = servers.iter()
        .filter(|s| s.trust_class.is_none())
        .count();
    
    println!("  Trust distribution:");
    println!("    🟢 Local Trusted: {}", local_count);
    println!("    🟡 Remote Verified: {}", verified_count);
    println!("    🔴 Remote Untrusted: {}", untrusted_count);
    if unspecified_count > 0 {
        println!("    ⚪ Unspecified: {}", unspecified_count);
    }
    
    Ok(())
}
