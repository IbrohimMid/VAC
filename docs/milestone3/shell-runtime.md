# Shell Runtime Guide

## Overview

VAC provides PTY-based shell execution for interactive commands and long-running processes.

## Shell Modes

### Interactive Shell
Full PTY with input/output streaming.
- **Use when:** Interactive commands, debugging
- **Features:** Real-time output, input support, signal handling

### Command Execution
Single command with output capture.
- **Use when:** Build commands, scripts
- **Features:** Exit code capture, output buffering

## TUI Shell Popup

### Opening Shell
- Press `Ctrl+S` to open shell popup
- Enter command or leave empty for interactive shell
- Press `Enter` to execute

### Shell Controls
- `Ctrl+C` - Send interrupt signal
- `Ctrl+D` - Send EOF
- `Esc` - Background shell (keeps running)
- `Ctrl+K` - Kill shell process

### Shell States
- **Running** - Command executing
- **Waiting for input** - Ready for user input
- **Completed** - Finished with exit code
- **Backgrounded** - Running in background

## Activity Events

Shell lifecycle tracked in Activity panel:
- ⚡ `Shell started: cargo build`
- ⚡ `Shell error: command not found`
- ⚡ `Shell success (exit code 0)`
- ⚡ `Shell failed (exit code 1)`

## Configuration

Shell behavior controlled by execution environment:

```toml
[runtime]
execution_environment = "host"  # Direct shell access
# or
execution_environment = "isolated_interactive"  # Shell in container
```

## Shell Lifecycle

1. **Start** - PTY allocated, process spawned
2. **Output** - Real-time streaming to popup
3. **Input** - User input sent to PTY
4. **Completion** - Exit code captured, PTY closed
5. **Cleanup** - Process terminated, resources freed

## Buffer Management

- Output buffer: Last 10,000 lines
- Automatic scrolling to latest output
- Manual scroll with arrow keys

## Exit Codes

- `0` - Success
- `1-255` - Error codes
- `-1` - Process killed or crashed

## Best Practices

1. **Use for interactive tasks** - Debugging, exploration
2. **Background long tasks** - Use `Esc` to background
3. **Check exit codes** - Verify command success
4. **Kill hung processes** - Use `Ctrl+K` if unresponsive
5. **Use isolation** - Run untrusted commands in isolated environment

## Examples

### Build command
```
cargo build --release
```

### Interactive shell
```
(leave empty, press Enter)
```

### Script execution
```
./scripts/deploy.sh
```

### Container command (isolated)
```
docker ps -a
```

## Troubleshooting

**Shell not responding:**
- Press `Ctrl+K` to force kill
- Check process in system monitor

**No output visible:**
- Scroll down to see latest output
- Check if command produces output

**Command not found:**
- Verify command is in PATH
- Check execution environment (host vs isolated)

**Permission denied:**
- Check file permissions
- Verify execution environment allows operation
