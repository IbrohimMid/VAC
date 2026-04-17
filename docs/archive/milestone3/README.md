# Milestone 3 Operator Documentation

Comprehensive guides for operating VAC with Milestone 3 features: isolation, MCP integration, shell runtime, and background jobs.

## Quick Links

- **[Isolation Guide](isolation.md)** - Execution boundaries and containerization
- **[MCP Trust Guide](mcp-trust.md)** - Model Context Protocol server security
- **[Shell Runtime Guide](shell-runtime.md)** - Interactive shell execution
- **[Runtime Jobs Guide](runtime-jobs.md)** - Background task management
- **[Mode Matrix Guide](mode-matrix.md)** - Mode combinations and security

## Getting Started

1. **Read Mode Matrix** - Understand mode combinations
2. **Configure Isolation** - Set execution environment
3. **Setup MCP Servers** - Add trusted tools
4. **Test Shell** - Verify PTY execution
5. **Monitor Jobs** - Check runtime status

## Key Concepts

### Execution Boundaries
VAC can run on host or in isolated containers. Choose based on trust level and security requirements.

### Trust Classification
MCP servers are classified by trust level. Use appropriate trust class for each server.

### Shell Lifecycle
Interactive shell with full PTY support. Monitor via Activity panel.

### Job Management
Background jobs for automation. Monitor and control via Runtime tab.

## Common Workflows

### Development Setup
```toml
[runtime]
execution_environment = "host"
environment_mode = "trusted-networked"
task_intent_mode = "patch"
```

### CI/CD Setup
```toml
[runtime]
execution_environment = "isolated_batch"
environment_mode = "restricted-offline"
task_intent_mode = "auto-fix-low-risk"
```

### Production Monitoring
```toml
[runtime]
execution_environment = "isolated_batch"
environment_mode = "trusted-networked"
task_intent_mode = "monitor"
```

## Troubleshooting

See individual guides for specific troubleshooting:
- Isolation issues → [Isolation Guide](isolation.md)
- MCP connection problems → [MCP Trust Guide](mcp-trust.md)
- Shell not responding → [Shell Runtime Guide](shell-runtime.md)
- Jobs stuck → [Runtime Jobs Guide](runtime-jobs.md)
- Mode conflicts → [Mode Matrix Guide](mode-matrix.md)

## Additional Resources

- [Activity & Audit System](../activity-audit.md)
- [VAC Configuration Reference](../../README.md)
- [Runtime Operating Guide](../runtime_operating_guide.md)
