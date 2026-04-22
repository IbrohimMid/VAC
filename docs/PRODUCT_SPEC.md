# VAC — Vastar Agentic CLI: Product Specification

**Version:** 1.0  
**Status:** Production  
**Updated:** 2026-04-22

---

## 1. What is VAC?

VAC (Vastar Agentic CLI) is an **autonomous software engineering agent** designed for VIL (Vastar Infrastructure Layer) developers. It combines a full-featured terminal UI, a multi-agent swarm orchestrator, and deep VIL-native tooling into a single binary that replaces the typical "chat with an LLM" loop with a structured, reviewable, and governable agentic workflow.

VAC is the primary development cockpit for engineers working on VIL-native Rust services, WASM modules, and sidecar components. It is designed to outperform general-purpose tools (Claude Code, Stakpak, Trae) on VIL-specific workflows while matching them on general coding tasks.

---

## 2. Core Value Propositions

| Value | Description |
|-------|-------------|
| **Reviewable autonomy** | Every tool call requires explicit approval or pre-authorized policy. No agent action is invisible. |
| **VIL-native awareness** | Parses VWFD schemas, validates vil-expr, reads VIL IR, and generates handler scaffolds per execution mode. |
| **Truthful state** | UI never renders placeholder values. Boot skeleton, hydration gate, and streaming all surface real state. |
| **Governable** | Rulebook system, policy gate modes (enforce/permit/audit), MCP trust classification, and approval state machine. |
| **Resumable** | Every task creates a checkpoint. Sessions are fully recoverable across restarts, network failures, or agent crashes. |
| **Multi-agent** | Tri-lane (Trigger/Data/Control) swarm orchestrator with Planner, Executor, and Reviewer roles. |

---

## 3. System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  vac CLI (vac_cli)                                          │
│  Commands: run • interactive • autopilot • runtime • ...    │
└────────────────────────┬────────────────────────────────────┘
                         │
         ┌───────────────┴───────────────┐
         │                               │
┌────────▼────────┐            ┌─────────▼────────┐
│  TUI Runtime    │            │  VacEngine        │
│  (vac_tui_      │            │  (vac_core)       │
│   runtime)      │            │                   │
│                 │            │  - Task execution │
│  - Event loop   │            │  - Tool router    │
│  - Workbench    │            │  - Session mgmt   │
│  - Overlays     │            │  - Config system  │
│  - Streaming    │◄──────────►│  - LSP bridge     │
│  - Approvals UI │            │  - Policy gate    │
└─────────────────┘            └─────────┬─────────┘
                                         │
              ┌──────────────────────────┼─────────────────────────┐
              │                          │                          │
    ┌─────────▼────────┐    ┌───────────▼──────────┐  ┌───────────▼──────────┐
    │  vil_swarm        │    │  vac_tools            │  │  VIL Subsystems      │
    │  Multi-agent      │    │  Tool routing         │  │                      │
    │  orchestrator     │    │  + MCP bridge         │  │  vil_llm (providers) │
    │                   │    │  + Approval gate      │  │  vil_rag (search)    │
    │  Planner          │    │  + Sandbox            │  │  vil_ir (code IR)    │
    │  Executor         │    │  + Privacy vault      │  │  vil_validate        │
    │  Reviewer         │    │                       │  │  vil_vwfd (schema)   │
    └───────────────────┘    └───────────────────────┘  │  vil_expr (parser)   │
                                                         │  vil_knowledge       │
                                                         │  vil_context         │
                                                         │  vil_memory          │
                                                         └──────────────────────┘
```

---

## 4. Feature Surface Summary

### 4.1 CLI Layer

| Category | Features |
|----------|----------|
| Task execution | `vac run`, multi-priority, profile selection, headless approve |
| Interactive TUI | `vac interactive` with full workbench, streaming, approvals |
| Session management | Resume, checkpoint, restore to pre-agent snapshot |
| VIL operations | `vac vil {init,dev,gen,deploy}` wrapper with task tray integration |
| Governance | `vac rulebook {list,validate,apply}`, policy gate modes |
| Autonomy | `vac autopilot {up,down,status,run}` — 24/7 daemon |
| Runtime | `vac runtime {status,jobs,inspect,cancel,retry}` |
| Isolation | `vac isolation {status,logs,run,wrap,doctor}` |
| MCP | `vac mcp {list,status}` — external tool server management |
| Telemetry | `--otel-endpoint`, `--metrics-addr`, `vac trajectory` |
| Security | `vac auth`, `vac export --signed`, `vac import --require-signed` |
| Config | `vac config {show,set,add-provider}` |

### 4.2 TUI Layer

| Category | Features |
|----------|----------|
| Conversation | Streaming markdown, tok/s meter, message history |
| Input | Rich editor, `@`-mentions, `/`-commands, `!`-shell, context chips |
| Workbench tabs | Review, Approvals, Sessions, Shell, Runtime, VIL, Agents, VWFD |
| Overlays (20+) | File picker, model switcher, profile, rulebook, isolation, task tray, command palette, shortcuts |
| Review | Syntax-highlighted diff, image preview (Kitty), VWFD semantic diff |
| Approvals | Tool call review with args/result, approve/reject/approve-all |
| Session | Resume with snapshot metadata, fuzzy search, date filter |
| VIL workbench | Diagnostics viewer, severity levels, repair proposals |
| VWFD inspector | Workflow tree (40/60 split), execution mode badge, jump-to-source |
| Mouse | Click dispatch on all interactive surfaces, side panel collapse |
| Theme | Multi-preset (light/dark/high-contrast), TOML hot-reload |
| Keybindings | User-defined `.vac/keybindings.toml` with hot-reload |
| Recorder | JSONL session recording + `--replay` replay mode |

### 4.3 Agent Swarm

| Category | Features |
|----------|----------|
| Orchestration | Tri-lane protocol (Trigger / Data / Control) |
| Roles | Planner, Executor, Reviewer agents |
| Context | Budget management, RAG-backed codebase crawler |
| Fault tolerance | Per-agent sandboxing, checkpoint/restore |
| Policy | Pre/post-step hooks, approval gate bridging |
| Subagents | Nested task delegation within an agent |
| Streaming | Token streaming from nested LLM calls |

### 4.4 VIL-Native Tooling

| Category | Features |
|----------|----------|
| VWFD schema | Parse/validate YAML workflow definitions, 3-fixture golden tests |
| vil-expr | Recursive-descent parser, symbol resolution, live linting in TUI |
| VIL IR | Rust source → semantic IR (functions, structs, enums, traits, impls) |
| IR diff | Module-level change detection, rename tracking |
| Validation | Semantic, zero-copy legality, observability, VIL Way compliance |
| Knowledge base | Pattern corpus, best practices, canonical lint |
| Handler scaffold | Code gen for Native/WASM/Sidecar execution modes |
| Parity gate | VWFD ↔ Rust handler cross-verification before `vac vil gen` |

### 4.5 Infrastructure

| Category | Features |
|----------|----------|
| LLM providers | Anthropic, OpenAI (extensible via `LlmProvider` trait) |
| Prompt caching | `CacheControlHint` — minimize re-send of stable context |
| Tools | 25+ built-in tools; MCP bridge for external tool servers |
| Approvals | State machine (Pending→Approved/Rejected), registry, persistence |
| Sessions | Async snapshot/checkpoint, schema versioning, migration |
| Changeset | Per-file lifecycle tracking (Created/Modified/Removed/Reverted) |
| Scheduler | Background job queue, cron scheduler, agent task scheduler |
| Isolation | Host/container/interactive/batch execution modes |
| Telemetry | OpenTelemetry traces, COSE-signed exports, secret redaction |
| Security | Secret detection, redaction, network isolation, trust classification |

---

## 5. Configuration Model

VAC uses layered TOML configuration at:

1. **Global:** `~/.config/vac/config.toml`
2. **Project:** `.vac/config.toml` (created by `vac init`)
3. **Keybindings:** `.vac/keybindings.toml` (hot-reloaded)
4. **Theme:** `.vac/theme.toml` (hot-reloaded)

Key configuration sections:

```toml
[llm]              # Provider routing, model, budget
[tools]            # Tool allow/deny policy
[runtime]          # Task intent mode, environment mode, isolation
[rulebooks]        # Governance constraint files
[vil]              # vil binary path, min version
[mcp]              # MCP server endpoints + trust classification
```

---

## 6. Security Model

| Boundary | Mechanism |
|----------|-----------|
| Tool execution | Approval state machine; policy gate (enforce/permit/audit) |
| Secrets in LLM calls | `SecretDetector` + `SecretSubstitution` before every API call |
| Session exports | COSE signing; `--require-signed` import validation |
| Network isolation | `network_policy` in runtime config |
| MCP servers | Trust classification per server in config |
| Container isolation | Docker/OCI runtime with `allowed_mounts` + `allowed_env` |

Full details: `docs/SECURITY.md`, `docs/THREAT_MODEL.md`, `docs/privacy_architecture.md`.

---

## 7. Observability

- **OpenTelemetry:** structured traces on every task, tool call, and agent step
- **Metrics server:** Prometheus-compatible at `--metrics-addr`
- **Trajectory API:** `vac trajectory observe/explain/why` for post-hoc audit
- **Session recorder:** JSONL trace of all inputs/outputs (replay-capable)
- **Activity log:** In-TUI timestamped activity feed

---

## 8. PRD Index

| Feature Area | Document |
|-------------|----------|
| CLI Commands | `docs/prd/PRD-CLI.md` |
| TUI Interface | `docs/prd/PRD-TUI.md` |
| Multi-Agent Swarm | `docs/prd/PRD-AGENT-SWARM.md` |
| VIL-Native Tooling | `docs/prd/PRD-VIL-NATIVE.md` |
| Approval System | `docs/prd/PRD-APPROVAL-SYSTEM.md` |
| Session Management | `docs/prd/PRD-SESSION-MANAGEMENT.md` |
| Runtime & Scheduler | `docs/prd/PRD-RUNTIME-SCHEDULER.md` |
| Tools & MCP | `docs/prd/PRD-TOOLS-MCP.md` |
| Security & Governance | `docs/prd/PRD-SECURITY-GOVERNANCE.md` |

---

## 9. Crate Map

| Crate | Role |
|-------|------|
| `vac_cli` | CLI binary: command parsing, entry points |
| `vac_core` | Engine: config, task execution, LSP, ACP server |
| `vac_tui_runtime` | TUI: event loop, rendering, all UI features |
| `vac_tools` | Tool routing, built-in tools, MCP bridge |
| `vac_approvals` | Approval state machine + registry |
| `vac_runtime` | Job queue, scheduler, isolation manager |
| `vac_session_control` | Snapshot/checkpoint persistence |
| `vac_changeset` | File lifecycle tracking |
| `vac_trace` | Telemetry recording + export |
| `vac_ingest` | Project context ingestion |
| `vil_swarm` | Multi-agent orchestrator (tri-lane) |
| `vil_llm` | LLM provider abstraction + streaming |
| `vil_rag` | Semantic indexing + retrieval |
| `vil_ir` | Rust source → semantic IR |
| `vil_validate` | Semantic validation passes |
| `vil_vwfd` | VWFD schema + code gen |
| `vil_expr` | vil-expr parser + validator |
| `vil_knowledge` | Pattern corpus + canonical lint |
| `vil_context` | SHM-backed context engine |
| `vil_memory` | Three-tier persistent memory |
| `vil_inference` | Type inference engine |
