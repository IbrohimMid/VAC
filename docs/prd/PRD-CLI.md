# PRD — CLI Commands

**Feature Area:** CLI Layer (`crates/vac_cli`)  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

The VAC CLI is a single binary (`vac`) that exposes all system capabilities through subcommands. It is the entry point for both headless automation and the interactive TUI. Every subcommand supports `--format text|json` for CI/scripting integration and `-C/--project <PATH>` to target a specific project root.

---

## Global Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-C, --project <PATH>` | cwd | Override project root |
| `-v, --verbose` | off | Increase log verbosity (cumulative) |
| `--format <FORMAT>` | text | Output format: `text`, `json` |
| `--log-format <FORMAT>` | text | Log format: `text`, `json` |
| `--otel-endpoint <URL>` | — | OpenTelemetry collector endpoint |
| `--metrics-addr <ADDR>` | — | Prometheus metrics server address |

---

## Commands

### `vac init`

Initialize VAC in a project directory. Creates `.vac/` with default `config.toml`, directory structure for sessions, checkpoints, traces, and memory.

**Flags:** none beyond globals  
**Side effects:** Writes `.vac/config.toml`, creates subdirectories

---

### `vac run`

Execute a single task via the agent swarm. The agent plans, calls tools, and completes the task. Returns when the task reaches a terminal state (Completed/Failed/Cancelled).

```bash
vac run "Add error handling to the auth module"
vac run "Fix the failing test" --priority high --profile strict
vac run "Refactor services/" --approve   # headless auto-approve
```

**Flags:**

| Flag | Description |
|------|-------------|
| `--approve` | Auto-approve all tool calls (headless) |
| `--priority <LEVEL>` | `low`, `normal` (default), `high`, `critical` |
| `--profile <NAME>` | Use a specific configuration profile |
| `--format json` | Machine-readable task result output |

**Output:** Task result summary including modified files, validation score, and checkpoint ID.

---

### `vac interactive`

Open the full-screen TUI. This is the recommended mode for any task requiring review, approval, or monitoring. See `docs/prd/PRD-TUI.md` for full TUI feature coverage.

```bash
vac interactive
vac interactive --resume <session-id>
vac interactive --replay <jsonl-file>   # replay a recorded session
```

**Flags:**

| Flag | Description |
|------|-------------|
| `--resume <SESSION_ID>` | Start with a session resumed from checkpoint |
| `--replay <FILE>` | Replay a recorded JSONL session (read-only) |

---

### `vac resume`

Resume a task or session that was interrupted. Equivalent to `vac interactive --resume` but can also operate headlessly.

```bash
vac resume <session-id>
vac resume --latest
```

---

### `vac restore`

Restore a file to its pre-agent state using the snapshot journal. Useful when a task modified a file incorrectly and you want to undo only that file without reverting the entire session.

```bash
vac restore src/auth.rs
vac restore src/auth.rs --checkpoint <checkpoint-id>
```

---

### `vac status`

Show current engine status: active session, LLM provider, task queue depth, and subsystem health.

```bash
vac status
vac status --format json
```

---

### `vac doctor`

Check all VAC subsystems for readiness. Verifies config, LLM provider reachability, VIL binary availability, session storage, and tool permissions. Can auto-repair common issues.

```bash
vac doctor
vac doctor --strict         # fail on any warning
vac doctor --format json    # machine-readable health report
```

**Checks performed:**
- `config.toml` parse + required fields
- LLM provider API key presence + connectivity
- `vil` binary discovery + version guard
- `.vac/` directory structure + permissions
- MCP server reachability
- Isolation runtime availability (Docker/OCI if configured)

---

### `vac trajectory`

Inspect recent agent execution artifacts for audit and debugging.

```bash
vac trajectory observe                      # list recent trajectory artifacts
vac trajectory explain <id-or-label>        # explain a trajectory by ID
vac trajectory why <file-path>             # explain why a file changed
```

---

### `vac config`

Manage VAC configuration.

```bash
vac config show                             # dump current effective config
vac config set llm.default_provider openai
vac config add-provider anthropic --api-key-env ANTHROPIC_API_KEY
```

---

### `vac auth`

Authentication management.

```bash
vac auth login
vac auth status
vac auth logout
```

---

### `vac export`

Export a session bundle for sharing, archiving, or handoff.

```bash
vac export <session-id>
vac export <session-id> --format vac-cbor
vac export <session-id> --format bundle-json --sign   # COSE-signed
```

**Formats:** `vac-cbor` (compact binary), `bundle-json` (human-readable)

---

### `vac import`

Import a session bundle.

```bash
vac import <bundle-file>
vac import <bundle-file> --require-signed   # reject unsigned bundles
vac import <bundle-file> --redact-secrets   # strip secrets on import
```

---

### `vac rulebook`

Manage governance rulebooks. Rulebooks are markdown files with YAML frontmatter defining constraints applied to every agent task.

```bash
vac rulebook list
vac rulebook validate <rulebook-file>
vac rulebook apply <rulebook-name>
```

---

### `vac vil`

VIL native binary wrapper. Wraps `vil` binary commands with VAC's approval gate, checkpoint system, and TUI integration.

```bash
vac vil init                                # initialize VIL project
vac vil dev                                 # run vil dev (streams to task tray + activity)
vac vil gen handler --kind vilserver --execution-mode native --name my_handler
vac vil deploy --target production          # requires policy approval
```

**Behaviour:**
- `vac vil dev` streams events to the TUI task tray with Running/Exited/Error states
- `vac vil gen` produces a changeset diff preview before writing files
- `vac vil deploy` routes through `PolicyGateAction::Deploy` → `vac_approvals`

---

### `vac acp`

Start the ACP (Agent Communication Protocol) editor server. Allows IDEs and editors to interact with VAC's agent runtime.

```bash
vac acp                     # default port 4123
vac acp --port 4200
```

---

### `vac runtime`

Inspect and manage the background agent runtime queue.

```bash
vac runtime status                          # queue depth, mode, environment
vac runtime jobs                            # list all jobs
vac runtime inspect <job-id>               # job detail + task graph
vac runtime cancel <job-id>
vac runtime retry <job-id>
```

---

### `vac isolation`

Manage execution isolation boundaries.

```bash
vac isolation status                        # current isolation mode + health
vac isolation logs                          # container/sandbox logs
vac isolation run -- bash -lc "pwd"        # run command in isolation boundary
vac isolation wrap <command>               # wrap a command in isolation
vac isolation clear-logs
vac isolation doctor                        # isolation subsystem health check
```

---

### `vac mcp`

Manage Model Context Protocol external tool servers.

```bash
vac mcp list                                # list configured MCP servers
vac mcp status                              # connectivity + trust classification
```

---

### `vac autopilot`

Manage the 24/7 autonomous background daemon.

```bash
vac autopilot up                            # start daemon
vac autopilot down                          # stop daemon
vac autopilot status                        # daemon state + queue
vac autopilot run <task>                   # submit task to autopilot queue
```

---

### `vac migrate`

Run schema migrations on `.vac/` artifacts when upgrading VAC.

```bash
vac migrate
vac migrate --dry-run    # preview migration without writing
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Task failed or agent error |
| `2` | Configuration / initialization error |
| `3` | Provider / network error |
| `4` | Approval denied |
| `5` | Policy gate rejected action |
