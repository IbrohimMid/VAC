# Mode Matrix Guide

## Overview

VAC operates in different modes that control execution boundaries, network access, and automation level. Understanding mode combinations is critical for safe operation.

## Mode Dimensions

### 1. Execution Environment
Controls where code executes.

| Mode | Description | Risk | Use Case |
|------|-------------|------|----------|
| `host` | Direct host execution | High | Development, trusted ops |
| `isolated_batch` | Container without TTY | Low | CI/CD, automation |
| `isolated_interactive` | Container with TTY | Low | Interactive debugging |

### 2. Environment Mode
Controls network and resource access.

| Mode | Description | Network | Use Case |
|------|-------------|---------|----------|
| `trusted-networked` | Full network access | Yes | API integration, remote tools |
| `restricted-offline` | No network access | No | Local-only operations |

### 3. Task Intent Mode
Controls automation level.

| Mode | Description | Approval | Use Case |
|------|-------------|----------|----------|
| `monitor` | Observe only | N/A | Monitoring, analysis |
| `suggest` | Propose changes | Manual | Code review, suggestions |
| `patch` | Apply changes | Manual | Assisted development |
| `auto-fix-low-risk` | Auto-apply safe fixes | Auto | Linting, formatting |

## Mode Combinations

### Development (High Trust)
```toml
[runtime]
execution_environment = "host"
environment_mode = "trusted-networked"
task_intent_mode = "patch"
```
- **Use:** Local development
- **Risk:** High - full system access
- **Approval:** Manual for changes

### CI/CD (Isolated)
```toml
[runtime]
execution_environment = "isolated_batch"
environment_mode = "restricted-offline"
task_intent_mode = "auto-fix-low-risk"
```
- **Use:** Automated testing, linting
- **Risk:** Low - contained execution
- **Approval:** Auto for safe operations

### Production Monitoring
```toml
[runtime]
execution_environment = "isolated_batch"
environment_mode = "trusted-networked"
task_intent_mode = "monitor"
```
- **Use:** Observability, diagnostics
- **Risk:** Low - read-only
- **Approval:** N/A - no modifications

### Interactive Debugging
```toml
[runtime]
execution_environment = "isolated_interactive"
environment_mode = "trusted-networked"
task_intent_mode = "suggest"
```
- **Use:** Troubleshooting, exploration
- **Risk:** Medium - contained but networked
- **Approval:** Manual for all changes

## TUI Mode Indicators

Header shows current mode combination:
```
exec:host  intent:patch  env:trusted-networked
```

Color coding:
- **Yellow** - Host execution (high risk)
- **Green** - Isolated batch (low risk)
- **Cyan** - Isolated interactive (medium risk)
- **Red** - Trusted-networked (network access)

## Mode Selection Guide

### Choose Execution Environment

**Use `host` when:**
- Local development
- Need native performance
- Trust all operations

**Use `isolated_batch` when:**
- CI/CD pipelines
- Untrusted code
- Automated tasks

**Use `isolated_interactive` when:**
- Need shell access
- Debugging in isolation
- Interactive exploration

### Choose Environment Mode

**Use `trusted-networked` when:**
- Need API access
- Remote tool integration
- Cloud operations

**Use `restricted-offline` when:**
- Local-only operations
- Security-critical tasks
- No external dependencies

### Choose Task Intent Mode

**Use `monitor` when:**
- Observing system state
- Collecting diagnostics
- No modifications needed

**Use `suggest` when:**
- Code review assistance
- Exploring solutions
- Manual approval preferred

**Use `patch` when:**
- Assisted development
- Controlled modifications
- Manual approval required

**Use `auto-fix-low-risk` when:**
- Automated linting
- Safe formatting
- High confidence operations

## Security Matrix

| Execution | Environment | Intent | Risk Level | Recommended For |
|-----------|-------------|--------|------------|-----------------|
| host | trusted-networked | auto-fix | ⚠️ HIGH | Never use |
| host | trusted-networked | patch | ⚠️ HIGH | Dev only |
| host | restricted-offline | patch | 🟡 MEDIUM | Local dev |
| isolated_batch | trusted-networked | auto-fix | 🟡 MEDIUM | CI/CD |
| isolated_batch | restricted-offline | auto-fix | 🟢 LOW | Safe automation |
| isolated_interactive | trusted-networked | suggest | 🟡 MEDIUM | Debug |
| isolated_batch | trusted-networked | monitor | 🟢 LOW | Production |

## Best Practices

1. **Start restrictive** - Begin with isolated + restricted + monitor
2. **Escalate gradually** - Add permissions as needed
3. **Never auto-fix on host** - Always require approval for host execution
4. **Isolate untrusted code** - Use containers for unknown operations
5. **Restrict network** - Disable network unless required
6. **Monitor first** - Observe before modifying

## Mode Switching

Modes can be changed:
- **Config file** - Edit `vac.toml` and restart
- **Runtime** - Some modes switchable via autopilot
- **Per-task** - Override for specific operations

## Troubleshooting

**Operation blocked:**
- Check current mode in header
- Verify mode allows operation
- Switch to appropriate mode

**Unexpected behavior:**
- Review mode combination
- Check mode matrix for conflicts
- Verify configuration

**Security concerns:**
- Audit current mode settings
- Review security matrix
- Switch to more restrictive mode
