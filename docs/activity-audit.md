# Activity & Audit System

VAC TUI includes a comprehensive activity logging system that tracks all significant operations across different subsystems.

## Activity Kinds

### 🔧 Tool
Tool execution events (approval, execution, completion)

### ⚑ Approval
Approval flow events (approved, rejected, pending)

### Δ Review
File review and changeset events (revert, open editor)

### ⎇ Session
Session management events (switch, restore, new)

### ! Error
Error and failure events

### 🔌 MCP
Model Context Protocol server events:
- MCP server connection success
- MCP server connection failure
- Tool registration count

### 🛡 Isolation
Execution boundary crossing events:
- Entering/exiting isolated environment
- Boundary policy enforcement
- Environment mode switches

### ⚡ Shell
Shell execution lifecycle events:
- Shell command started
- Shell error output
- Shell completion with exit code

### • Status
General status updates and state changes

## Activity Panel

The Activity panel in the TUI shows a real-time stream of all events with:
- Timestamp (HH:MM:SS)
- Icon indicating event type
- Event message

Navigate with:
- `j`/`k` or arrow keys to scroll
- Auto-scrolls to show latest events

## Audit Trail

All activity events are:
1. Timestamped with UTC time
2. Categorized by kind
3. Stored in memory (last 500 events)
4. Visible in the Activity panel

This provides a complete audit trail of:
- What operations were performed
- When they occurred
- What the outcome was
- Any errors or failures

## Event Examples

```
15:30:42 🔌 MCP server 'filesystem' connected (12 tools)
15:30:45 🛡 Isolation: command execution in isolated-batch
15:30:50 ⚡ Shell started: cargo build
15:31:02 ⚡ Shell success (exit code 0)
15:31:05 ⚑ Approved: write_file
15:31:06 🔧 Tool executed: write_file
```

## Integration Points

Activity events are emitted from:
- **TUI event loop** - User interactions, UI state changes
- **Engine** - MCP connections, tool execution
- **Runtime** - Job scheduling, autopilot state
- **Shell** - PTY command execution
- **Isolation** - Boundary enforcement

This provides comprehensive visibility into VAC operations for debugging, auditing, and monitoring.
