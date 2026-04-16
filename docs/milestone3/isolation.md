# Isolation Guide

## Overview

VAC supports execution isolation to contain agent operations within controlled boundaries. This prevents unintended system-wide modifications and provides security guarantees.

## Execution Environments

### Host
Direct execution on the host system.
- **Use when:** Development, trusted operations
- **Risk:** Full system access
- **Performance:** Native speed

### Isolated Batch
Containerized execution without TTY.
- **Use when:** Automated tasks, CI/CD
- **Risk:** Contained within container
- **Performance:** Near-native

### Isolated Interactive
Containerized execution with TTY support.
- **Use when:** Interactive shell sessions
- **Risk:** Contained within container
- **Performance:** Near-native with PTY overhead

## Configuration

In `vac.toml`:

```toml
[runtime]
execution_environment = "isolated_batch"  # or "host" or "isolated_interactive"
```

## CLI Commands

### Check isolation status
```bash
vac isolation status
```

### View isolation logs
```bash
vac isolation logs
```

### Run command in isolated environment
```bash
vac isolation run -- cargo build
vac isolation run --tty -- bash
```

## TUI Indicators

Header shows current execution environment:
- `exec:host` (yellow) - Running on host
- `exec:isolated-batch` (green) - Batch isolation
- `exec:isolated-interactive` (cyan) - Interactive isolation

Activity panel shows boundary crossings:
- 🛡 `Isolation: command execution in isolated-batch`

## Best Practices

1. **Use isolation for untrusted code** - Always run unknown code in isolated environment
2. **Host for development** - Use host mode for faster iteration during development
3. **Batch for automation** - Use isolated-batch for CI/CD and scheduled tasks
4. **Interactive for debugging** - Use isolated-interactive when you need shell access

## Troubleshooting

**Container not found:**
- Ensure Docker/Podman is installed and running
- Check isolation logs: `vac isolation logs`

**Permission denied:**
- Verify container runtime permissions
- Check user is in docker group (Linux)

**Slow performance:**
- Consider using host mode for development
- Check container resource limits
