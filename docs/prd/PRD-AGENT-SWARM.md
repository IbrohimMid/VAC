# PRD — Multi-Agent Swarm

**Feature Area:** `crates/vil_swarm`  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

The VAC swarm is a multi-agent orchestrator that decomposes tasks across specialized roles using a tri-lane communication protocol. It provides fault-tolerant, resumable, approval-gated execution with token budget management and VIL-native project awareness.

---

## Tri-Lane Protocol

All inter-agent communication flows across three typed channels:

| Lane | Purpose |
|------|---------|
| **Trigger** | Task initiation, user intents, policy signals |
| **Data** | File content, tool results, codebase context |
| **Control** | Status updates, checkpoints, approval requests, errors |

Agents never communicate outside their lane. This ensures auditability — every decision has a traceable data origin.

---

## Agent Roles

| Role | Responsibility |
|------|---------------|
| **Planner** | Decomposes the user task into a step sequence; owns the task graph |
| **Executor** | Runs individual steps: calls tools, writes files, runs commands |
| **Reviewer** | Validates outputs against VIL Way rules and the original intent |

One Planner and one Reviewer run per swarm session. Multiple Executor agents may run concurrently on independent subtasks.

---

## Reasoning State Machine

Each agent cycles through phases:

```
Attempt → Observe → Diagnose → Plan → Retry
                                       ↓
                                   Terminal (Completed / Failed)
```

- **Attempt:** Execute the current plan step
- **Observe:** Parse tool outputs and LLM responses
- **Diagnose:** Classify failures (transient vs. fatal)
- **Plan:** Revise steps if needed
- **Retry:** Re-enter Attempt with the revised plan

The state machine is serializable; a mid-execution crash resumes at the last persisted phase.

---

## VIL Project Profiling

At swarm startup, the orchestrator profiles the project to select the appropriate agent persona and prompt template:

| Archetype | Detection Signal |
|-----------|-----------------|
| `Server` | `vil_server!` macro, HTTP handlers |
| `Pipeline` | `vil_pipeline!`, data transform patterns |
| `Plugin` | `vil_plugin!`, sidecar patterns |
| `Hybrid` | Multiple archetypes detected |

The active archetype is stored on `SwarmOrchestrator` and passed to every agent's system prompt.

---

## Subagent Delegation

Any agent may spawn a subagent to handle a bounded subtask:

- Subagents inherit the parent's approval policy
- Subagents run inside a sandboxed registry (separate tool scope)
- Token usage is tracked separately per subagent and rolled up to the parent
- Nested streaming is surfaced in the TUI Agents tab

---

## Token Budget Management

Each agent receives a `context_budget` at construction time. The orchestrator:

1. Tracks accumulated input + output tokens across all LLM calls
2. Prunes old conversation turns via RAG-backed summarization when within 20% of budget
3. Emits a `Control::BudgetWarning` event when within 10% — the Planner may simplify remaining steps

---

## Event Stream

The swarm emits `AgentLoopEvent` throughout execution:

| Event | Consumers |
|-------|----------|
| `AgentStarted { role, id }` | TUI Agents tab |
| `LlmRequest { tokens }` | tok/s meter, budget tracker |
| `ToolCall { name, args }` | Approval gate, activity log |
| `ToolResult { name, result }` | Review tab |
| `Checkpoint { session_id }` | Session control |
| `Approval { record }` | Approvals tab |
| `Completed { summary }` | Task result output |
| `Failed { reason }` | Activity log, exit code 1 |

---

## Approval Gate Integration

Every tool call emitted by an Executor is routed through `vac_approvals` before execution:

1. Executor emits `ToolCall` on the Control lane
2. Swarm pauses the Executor and surfaces the call in the Approvals tab
3. User approves / rejects (or auto-approve policy fires)
4. Result flows back to the Executor as a `ToolResult`

With `--approve` (headless), all calls are auto-approved according to the active policy gate mode.

---

## Fault Tolerance

| Failure Mode | Recovery |
|-------------|---------|
| LLM transient error (5xx) | Exponential backoff, up to 3 retries |
| Tool call error | Diagnose phase; Planner revises steps |
| Agent crash | Checkpoint-based resume; new agent picks up at last persisted phase |
| Context exhaustion | Budget-aware pruning; Planner simplifies |
| Approval timeout | Approval auto-rejected; Executor marks step failed |

---

## Configuration

```toml
[swarm]
max_executors = 3          # max concurrent Executor agents
context_budget = 100_000   # tokens per agent
retry_limit = 3            # per-step retry cap
checkpoint_interval = 5    # steps between auto-checkpoints
```
