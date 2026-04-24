# ADR-002 — Nested Subagents Unsupported Beyond Depth 1

**Status:** Accepted
**Date:** 2026-04-24
**Supersedes:** —
**Superseded by:** —

## Context

`vac_session_engine::SubagentDispatchContext::child_scoped()` strips
`policy`, `gate`, and `dispatcher` from a subagent's compact config
when building a context for **its** children. This is intentional
(audit fix) — a subagent should not silently increment counters the
parent owns — but it means any second-level subagent loses:

- budget enforcement (PolicyGate)
- hook sandbox enforcement (HookGate)
- tool execution plumbing (ToolDispatcher)

A second-level subagent running with stripped context is at best
useless (no tools) and at worst unsafe (no gates).

Reviewer flagged this as a P1 decision: either model nested
delegation properly (quota inheritance, approval propagation,
cancellation tree, transcript tree) or explicitly forbid it.

## Decision

**VAC v1 supports subagent delegation at depth 1 only.**

| Depth | Role | Support |
|---|---|---|
| 0 | Parent agent | Fully supported |
| 1 | First-level subagent (spawned via `agent_run`) | Fully supported |
| ≥ 2 | Nested subagent | **Hard-denied at runtime** |

Enforcement:

1. `ToolContext` gains a `depth: u32` field.
2. `run_via_session_engine_with_broadcast` constructs the parent
   context at `depth = 0` and the first-level subagent's context at
   `depth = 1`.
3. `agent_run` tool inspects `context.depth` and returns an
   explicit error when `depth >= 1`:

   > "Nested subagents are not supported in VAC v1. First-level
   > delegation only — see ADR-002."

4. A regression test (`agent_run_denies_nested_depth`) pins the
   rejection path so a future edit can't quietly weaken it.

## Consequences

### Positive

- Unambiguous semantics. Operators never get "agent_run worked
  yesterday, fails today" from an accidentally-strip-then-restore
  path.
- Matches the existing `child_scoped()` strip — the runtime guard
  surfaces the design intent that was previously buried in code.
- Safe default: unclear semantics under nested delegation are
  avoided rather than documented-as-caveats.

### Negative

- Users who want deep agent trees cannot have them in v1.
- Workflows that today *might* have accidentally nested will get
  an explicit error. Mitigation: the error message points at
  ADR-002 and suggests restructuring.

### Neutral

- Nothing in the API prevents future nesting support; the gate
  lives in one tool (`agent_run`) and can be relaxed via a future
  ADR.

## When this ADR may be superseded

Only when **all** of the following are designed and tested:

- **Quota inheritance model** — how child quota relates to parent.
- **Gate inheritance model** — how gate decisions propagate across
  levels.
- **Approval propagation** — operator approvals in parent vs. child.
- **Cancellation tree** — ctrl-c semantics across arbitrary depth.
- **Transcript / session tree** — how nested sessions relate to
  the parent transcript.
- **Blast-radius policy** — what a depth-N subagent is allowed
  to do that a depth-1 one isn't.

Until those six are designed together, nested delegation stays
hard-denied.

## References

- `crates/vac_session_engine/src/subagent.rs::SubagentDispatchContext::child_scoped` —
  the strip that motivates this ADR.
- `crates/vac_tools/src/registry.rs::ToolContext.depth` — the
  per-submit depth field.
- `crates/vac_tools/src/builtin/agent_run.rs` — the runtime guard.
