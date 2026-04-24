# ADR-001 — VAC v1 Execution Architecture is Hybrid

**Status:** Accepted
**Date:** 2026-04-24
**Supersedes:** —
**Superseded by:** —

## Context

Post-NS-arc audits surfaced a recurring question: is `vac_session_engine`
the eventual single owner of tool-call execution, or does `VacEngine`
(vac_core) retain that role? The answer has been ambiguous in code:

- `vac_session_engine` provides the durable submit lifecycle:
  `TranscriptWriter`, `SubmitStream`, `CompositeGate`, `submit_one`.
- `VacEngine` (in `vac_core::engine`) owns task orchestration: planner,
  VIL-native reasoning, `run_task_with_updates`, approval plumbing,
  concrete tool dispatch.
- `VacEngineAdapter` (in `vac_tui_runtime::runner::engine_adapter`) wraps
  `VacEngine` as an `LlmAdapter`, returning an `LlmResponse` with
  `tool_calls: Vec::new()` — i.e. the engine does its own tool
  dispatch and doesn't hand tool calls back through the
  session-engine loop.

Leaving this unresolved risks drift: two parallel tool executors with
subtly different approval / gate / cancellation semantics. A reviewer
flagged this as a P1 architectural decision that must be explicit.

## Decision

**VAC v1 is hybrid by design:**

| Concern | Owner |
|---|---|
| Transcript durability | `vac_session_engine` |
| Session lifecycle | `vac_session_engine` |
| Event streaming (`SubmitChunk`) | `vac_session_engine` |
| Remote broadcast (`SessionBroadcast`) | `vac_session_engine` + `vac_bridge` |
| Gate composition (`CompositeGate`) | `vac_session_engine` |
| Planner / orchestrator | `VacEngine` (vac_core) |
| **Tool execution semantics** | **`VacEngine`** |
| Approval state machine | `VacEngine` |
| `TaskResult` semantics | `VacEngine` |
| VIL-native execution | `VacEngine` |

Stated plainly:

- `vac_session_engine` is the **control plane / durability plane**.
- `VacEngine` is the **semantic execution plane**.

`VacEngineAdapter` bridges the two — session-engine sees VacEngine as
an `LlmAdapter`; VacEngine sees the adapter as the conduit for
`RuntimeUpdate`s and the oneshot handoff for `TaskResult`.

## Consequences

### Positive

- **Zero duplication of tool-execution business logic.** Approval and
  trust-gate code paths live in one place.
- **Stable contract.** The hybrid boundary has been hardened across
  seven blockers (B1-B7) + three audit passes; attempting to move
  semantic ownership now would reopen all of that work.
- **Honest surface.** `LlmResponse.tool_calls = Vec::new()` is no
  longer a "not yet done" placeholder — it is **by design** because
  VacEngine owns the tool-call round-trip.

### Negative

- Session-engine cannot be used as a standalone tool executor.
- Drivers that want to embed VAC must take the whole stack
  (`vac_session_engine` + `vac_core` + `vac_tools`), not just the
  session engine.

### Neutral

- Future contributors wanting to refactor for "session-engine-owns-
  everything" must open a **new ADR** (ADR-003+) with:
  - a concrete trigger (deep agent trees, recursive delegation,
    session-engine-native tool execution, multi-operator execution).
  - a migration plan that does not produce two parallel executors.

## Non-goals

- This ADR does **not** permanently forbid a future unified-owner
  refactor. It forbids **silent** migration.

## Enforcement

- Reviewers: block PRs that migrate tool-call ownership from
  `VacEngine` into `vac_session_engine` without a superseding ADR.
- Code comments at `engine_adapter.rs::VacEngineAdapter::complete`
  point to this ADR so the `tool_calls: Vec::new()` line is
  readable as "intentional boundary", not "TODO".

## References

- `crates/vac_tui_runtime/src/runner/engine_adapter.rs` —
  `VacEngineAdapter` implementation.
- `crates/vac_session_engine/src/submit.rs` — `submit_one` entry.
- `crates/vac_core/src/engine.rs` — `VacEngine::run_task_with_updates`.
- Post-NS-arc reviewer audit (commit `25795f3` verification pass).
