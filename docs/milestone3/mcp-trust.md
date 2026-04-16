# MCP Trust Guide

## Overview

Model Context Protocol (MCP) servers extend VAC with external tools. Trust classification ensures safe integration.

## Trust Classes

### 🟢 Local Trusted
Servers running locally via stdio.
- **Risk:** Low - runs as local process
- **Use when:** Filesystem access, local tools
- **Example:** Local file browser, git operations

### 🟡 Remote Verified
Verified remote servers over HTTPS.
- **Risk:** Medium - network communication
- **Use when:** Trusted third-party services
- **Example:** GitHub API, verified cloud services

### 🔴 Remote Untrusted
Unverified remote servers.
- **Risk:** High - untrusted network endpoint
- **Use when:** Testing, development only
- **Example:** Experimental services, dev endpoints

## Configuration

In `vac.toml`:

```toml
[[mcp_servers]]
name = "filesystem"
trust_class = "local_trusted"
transport = { type = "stdio", command = "mcp-server-fs", args = [] }

[[mcp_servers]]
name = "github"
trust_class = "remote_verified"
transport = { type = "sse", url = "https://api.github.com/mcp" }
[mcp_servers.tls]
ca_file = "/path/to/ca.pem"

[[mcp_servers]]
name = "experimental"
trust_class = "remote_untrusted"
transport = { type = "sse", url = "http://localhost:8080/mcp" }
```

## CLI Commands

### List MCP servers
```bash
vac mcp list
```

Output:
```
MCP Servers:

  filesystem [🟢 local-trusted]
    Transport: stdio
    Command: mcp-server-fs

  github [🟡 remote-verified]
    Transport: sse
    URL: https://api.github.com/mcp
```

### Show MCP status
```bash
vac mcp status
```

Output:
```
MCP Status:

  Total servers: 3

  Trust distribution:
    🟢 Local Trusted: 1
    🟡 Remote Verified: 1
    🔴 Remote Untrusted: 1
```

## TUI Indicators

Activity panel shows MCP events:
- 🔌 `MCP server 'filesystem' connected (12 tools)`
- 🔌 `MCP server 'github' failed: connection timeout`

## Security Best Practices

1. **Prefer local trusted** - Use stdio transport when possible
2. **Verify remote servers** - Only use verified HTTPS endpoints
3. **Limit untrusted** - Avoid remote_untrusted in production
4. **Use TLS** - Always configure TLS for remote servers
5. **Restrict modes** - Use `allowed_in_modes` to limit when servers are active

## Approval Policies

Control tool execution with approval policies:

```toml
[[mcp_servers]]
name = "filesystem"
approval_policy = "auto"  # or "manual" or "deny"
```

- `auto` - Execute without approval
- `manual` - Require operator approval
- `deny` - Block all tool calls

## Troubleshooting

**MCP server failed to connect:**
- Check server is running (stdio)
- Verify URL is accessible (sse)
- Check trust class matches transport

**Tools not registered:**
- Check MCP server logs
- Verify server implements tools/list
- Check allowed_in_modes configuration
