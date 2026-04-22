# PRD — Approval System

**Feature Area:** `crates/vac_approvals`  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

Every tool call made by the agent swarm is gated by an approval state machine. The approval system ensures no agent action is invisible: users can review, approve, reject, or pre-authorize every tool invocation via policy.

---

## State Machine

An `ApprovalRecord` transitions through the following states:

```
Pending → Approved
        → Rejected
        → TimedOut   (auto-rejected after deadline)
        → Stale      (session ended before resolution)
```

State transitions are append-only events applied by `ApprovalStateMachine`. The record is immutable after reaching a terminal state.

---

## ApprovalRecord

Each record captures:

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Unique approval identifier |
| `session_id` | UUID | Parent session |
| `tool_call_id` | String | LLM tool call reference |
| `tool_name` | String | Name of the tool being called |
| `arguments` | JSON | Tool call arguments |
| `explanation` | String | Agent's stated reason for the call |
| `state` | Enum | Current FSM state |
| `created_at` | Timestamp | When the record was created |
| `resolved_at` | Option<Timestamp> | When it was approved/rejected |
| `intent` | Option<String> | High-level user intent label |

---

## ApprovalStore

The store persists records to `.vac/approvals/` as JSON files, one per record. Operations:

| Method | Description |
|--------|-------------|
| `create(record)` | Write new pending record to disk |
| `resolve(id, state)` | Apply state transition + persist |
| `get(id)` | Load record by ID |
| `list_by_session(session_id)` | All records for a session |
| `cleanup_stale(max_age)` | Auto-reject records older than max_age |
| `bulk_cleanup(session_id)` | Remove all records for a completed session |

File watch enables real-time sync: when an external process (e.g., a CLI approval) resolves a record, the TUI picks it up without polling.

---

## Intent System

Before the swarm begins a task, an `ApprovalIntent` record can be created to represent the user's high-level authorization:

- The user can pre-authorize a class of actions ("allow all file writes in `src/`")
- Individual tool calls matching the intent are auto-approved without TUI interruption
- Intents have a TTL; they expire after the declared duration

This enables semi-headless workflows without full `--approve` mode.

---

## Policy Gate Modes

The approval system integrates with the policy gate (`vac_core`):

| Mode | Behavior |
|------|---------|
| `enforce` | Every tool call requires explicit approval; unknown calls blocked |
| `permit` | Tool calls proceed unless a matching deny rule exists |
| `audit` | All calls proceed; approvals recorded for post-hoc review |

The active mode is set in `.vac/config.toml` under `[runtime]` and can be overridden per-rulebook.

---

## TUI Approvals Tab

The Approvals workbench tab shows all pending records for the active session:

| Column | Content |
|--------|---------|
| Tool | Tool name |
| Summary | First line of arguments |
| Explanation | Agent's stated reason |
| Age | Time since created |

Actions available per-record:

- **Approve** (`a`) — approve this call
- **Reject** (`r`) — reject this call
- **Approve all** (`A`) — approve all pending calls
- **Inspect** (`Enter`) — expand full arguments + explanation

---

## Headless Mode

With `vac run --approve`, the approval state machine bypasses the TUI and auto-approves every call according to the active policy gate. The records are still written to disk for audit purposes.

---

## Exit Code Integration

If the user rejects a tool call, the agent receives a rejection result. The agent may retry with a different approach or terminate. If the task terminates due to a rejection, `vac run` exits with code `4` (Approval denied).
