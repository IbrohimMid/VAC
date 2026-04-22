# PRD — Security & Governance

**Feature Area:** `vac_core`, `vac_approvals`, `vac_tools`, `vac_trace`  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

VAC enforces a multi-layer security model: approval gates for every tool call, secret detection before every LLM API call, COSE-signed session exports, network isolation for agent processes, and a rulebook system for governance constraints.

---

## Approval Gate

The primary security boundary. Every tool call must pass through the approval state machine before execution. See `docs/prd/PRD-APPROVAL-SYSTEM.md` for the full specification.

**Summary:**
- All tool calls start as `Pending`
- User approves/rejects via TUI or headless policy
- Auto-rejection on timeout
- Policy gate modes: `enforce` / `permit` / `audit`

---

## Secret Detection & Redaction

`SecretDetector` scans every outbound LLM API request payload (messages, tool results, context chunks) for secret patterns:

| Pattern Type | Examples |
|-------------|---------|
| API keys | `sk-...`, `AKIA...`, generic `*_API_KEY=` |
| Passwords | `password=`, `passwd=`, `pwd=` |
| Tokens | `bearer `, `token=`, `access_token` |
| Private keys | `-----BEGIN ... PRIVATE KEY-----` |
| Connection strings | `postgres://`, `mongodb://`, `redis://` |

Detected values are replaced inline with `[REDACTED:<type>]` before the payload is sent. The substitution is recorded in the session audit log (what type was redacted, not the value).

This runs on **every** API call — not just at session export time.

---

## Rulebook System

Rulebooks are markdown files with YAML frontmatter defining governance constraints:

```markdown
---
name: no-destructive-ops
version: 1
applies_to: ["bash", "delete_file"]
policy: enforce
---

# No Destructive Operations

Tools matching `applies_to` require explicit user approval regardless
of the active policy gate mode.
```

### Commands

```bash
vac rulebook list                    # list active rulebooks
vac rulebook validate <file>         # validate rulebook syntax
vac rulebook apply <name>            # activate a rulebook for the session
```

### Rulebook Scope

Rulebooks can restrict:
- Which tools are callable
- Which files / directories are writable
- Which network destinations are reachable
- What execution modes are permitted (read-only, safe-edit, etc.)

Multiple rulebooks stack; the most restrictive constraint wins.

---

## Session Export Signing

Session bundles can be COSE-signed at export time:

```bash
vac export <session-id> --sign
```

The bundle includes a COSE_Sign1 structure over the bundle payload. On import:

```bash
vac import <bundle-file> --require-signed    # reject if unsigned or invalid signature
```

This enables handoff workflows where the receiving party can verify the bundle was not tampered with in transit.

---

## Network Isolation

Agent tool calls that make network requests are subject to the `network_policy` configuration:

| Policy | Behavior |
|--------|---------|
| `none` | No outbound network access from tools |
| `host` | Full host network access |
| `bridge` | Isolated bridge network (container mode) |
| `allowlist` | Only specific destinations permitted |

```toml
[runtime]
network_policy = "none"
network_allowlist = ["api.openai.com", "api.anthropic.com"]
```

This applies to the agent's tool execution environment. LLM API calls from the VAC process itself are not subject to this policy (they use the host network).

---

## MCP Trust Classification

External MCP tool servers are assigned a trust level in config (see `docs/prd/PRD-TOOLS-MCP.md`). Low-trust servers cannot call into the VAC filesystem or spawn processes, regardless of what tools they advertise.

---

## Container Isolation

When isolation mode is `container`, the agent's tool execution runs inside a Docker/OCI container with:

- Read-only root filesystem (except declared mounts)
- Explicit `allowed_mounts` list
- Explicit `allowed_env` allowlist
- Configurable network policy
- No access to host credentials or sockets outside the declared policy

`vac isolation doctor` verifies the isolation setup before any task runs.

---

## Threat Model Summary

| Threat | Mitigation |
|--------|-----------|
| LLM prompt injection via tool results | Secret redaction; approval gate reviews args before execution |
| Malicious MCP server | Trust classification; low-trust servers blocked by default |
| Agent writes sensitive data to disk | Approval gate; `SecretDetector` on file write tool args |
| Session bundle tampered in transit | COSE signing; `--require-signed` import enforcement |
| Agent escapes isolation boundary | Container runtime + `allowed_mounts` enforcement |
| Approval bypass via headless mode | Headless only activates with explicit `--approve`; records still written |

Full details: `docs/THREAT_MODEL.md`, `docs/privacy_architecture.md`.

---

## Auth

`vac auth` manages authentication credentials for VAC cloud features (if configured):

```bash
vac auth login      # interactive login flow
vac auth status     # show current auth state
vac auth logout     # revoke stored credentials
```

Credentials are stored in the OS keychain (not in `.vac/config.toml`) to prevent accidental commits.

---

## Audit Log

Every significant event is written to the session's audit log:

- Tool calls (name, arguments hash, outcome)
- Approval decisions (approved/rejected, by whom)
- Secret redaction events (type, not value)
- Policy gate decisions
- Rulebook constraint triggers

The audit log is included in session exports and is queryable via `vac trajectory observe`.
