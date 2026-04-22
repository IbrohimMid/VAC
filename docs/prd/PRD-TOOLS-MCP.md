# PRD — Tools & MCP

**Feature Area:** `crates/vac_tools`  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

`vac_tools` provides the tool routing layer: a registry of 25+ built-in tools, a bridge to external MCP servers, a policy-based dispatch engine, and a sandbox sanitizer. Every tool call passes through this layer before execution.

---

## Tool Trait

All tools implement:

```rust
trait VilTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;   // JSON Schema
    fn trust_requirement(&self) -> TrustLevel;
    fn risk_level(&self) -> RiskLevel;
    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolResult;
}
```

### TrustLevel

| Level | Meaning |
|-------|---------|
| `ParentAgent` | Only callable by the top-level agent |
| `SandboxedSubagent` | Callable by subagents in sandboxed scope |
| `Any` | No trust restriction |

### RiskLevel

| Level | Meaning |
|-------|---------|
| `ReadOnly` | No side effects |
| `SafeEdit` | Writes to tracked files only |
| `Destructive` | Deletes or overwrites existing content |
| `Network` | Makes outbound network requests |
| `Execute` | Runs arbitrary commands |

---

## Built-in Tools (25+)

### File Operations
| Tool | Description |
|------|-------------|
| `read_file` | Read file contents with optional line range |
| `write_file` | Write or overwrite a file |
| `edit_file` | Apply a targeted string replacement |
| `delete_file` | Delete a file (destructive) |
| `list_directory` | List files matching a glob pattern |
| `search_files` | Ripgrep-backed content search |
| `move_file` | Rename or move a file |

### Code Intelligence
| Tool | Description |
|------|-------------|
| `grep` | Regex search across files |
| `glob` | Pattern-match file paths |
| `parse_ir` | Extract semantic IR from a Rust file |
| `validate_vil` | Run VIL validation passes on a file |
| `diff_ir` | Compute IR diff between two files |

### Shell
| Tool | Description |
|------|-------------|
| `bash` | Run a shell command (sandboxed) |
| `spawn_process` | Long-running process with streaming output |
| `kill_process` | Terminate a spawned process |

### VIL Operations
| Tool | Description |
|------|-------------|
| `vil_gen` | Generate handler scaffold from VWFD |
| `vil_validate_schema` | Validate a VWFD file |
| `parse_vil_expr` | Parse and validate a vil-expr string |

### Agent & Session
| Tool | Description |
|------|-------------|
| `spawn_subagent` | Delegate a subtask to a new subagent |
| `create_checkpoint` | Write a session checkpoint |
| `read_approval` | Check the state of an approval record |

### Web & Network
| Tool | Description |
|------|-------------|
| `web_fetch` | Fetch a URL (requires `network` policy) |
| `web_search` | Search the web (requires `network` policy) |

### Observability
| Tool | Description |
|------|-------------|
| `emit_span` | Write an OpenTelemetry span |
| `read_trajectory` | Read trajectory artifacts for a session |

---

## Tool Registry

`ToolRegistry` (thread-safe `RwLock`) stores all registered tool implementations:

- Built-in tools registered at startup
- MCP tools registered on server connect
- Skills (multi-step workflows) registered as composed tool sequences

`ToolRouter` dispatches a tool call:
1. Look up tool in registry
2. Check trust level against caller context
3. Check risk level against active approval policy
4. Sanitize arguments (sandbox)
5. Route to approval gate if required
6. Execute + return result

---

## MCP Bridge

VAC supports external [Model Context Protocol](https://modelcontextprotocol.io/) tool servers. Each MCP server is configured with a trust classification:

```toml
[[mcp.servers]]
name = "github"
endpoint = "http://localhost:3001"
trust = "low"       # low | medium | high

[[mcp.servers]]
name = "internal-db"
endpoint = "http://db-tools:4000"
trust = "high"
```

Trust classification affects the approval policy applied to MCP tool calls:

| Trust | Policy |
|-------|--------|
| `high` | Follows active policy gate (same as built-in tools) |
| `medium` | Always requires explicit approval |
| `low` | Blocked unless user explicitly approves per-call |

MCP server connectivity is verified by `vac doctor` and shown in `vac mcp status`.

---

## Sandbox Sanitizer

Before any `bash` or `spawn_process` call, arguments pass through the sandbox sanitizer, which rejects:

- Null bytes (`\0`)
- Backtick command substitution
- Process substitution (`$(...)`, `<(...)`)
- Shell operator chaining (`;`, `&&`, `||`) outside of explicitly allowed patterns
- Path traversal (`../` exceeding the allowed mount boundary)

Rejected arguments return a `ToolResult::Error` without execution.

---

## Skill Loader

Skills are pre-defined multi-step workflows composed from built-in tools. They appear as single tool calls to the LLM but execute as a sequence internally:

| Skill | Steps |
|-------|-------|
| `create-vilapp` | init VIL project → generate VWFD → scaffold handlers → validate |

Custom skills can be added via `.vac/skills/` directory as TOML definitions.

---

## Privacy Vault

Before any tool arguments are sent to the LLM, the `PrivacyVault` redacts secrets:

- Replaces detected secrets with `[REDACTED:<type>]` tokens
- Detects: API keys, passwords, tokens, private keys, connection strings
- Redaction is logged (what was redacted, not the value) for audit

Redacted sessions can be exported safely with `vac export --redact-secrets`.
