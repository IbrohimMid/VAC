# Runtime Queue Boundary

## Decision: Keep Two Separate Queues

After Phase 2 audit, the decision is to **Keep Two** separate queue substrates.

### Rationale

VAC keeps two queue substrates on purpose because they serve fundamentally different lifecycles and ownership boundaries:

- `vac_runtime::TaskQueue` stores generic runtime jobs such as autopilot sweeps, diagnostic jobs, patch proposals, and tool-call work. Its persistence file is `.vac/queue.json`. It is owned by the runtime/autopilot pipeline.
- `vac_runtime::AgentTaskQueue` stores role-scoped agent work items for the worker scheduler. Its persistence file is `.vac/agent_queue.json`. It is owned by the agent scheduler and worker lifecycle.

The schemas differ enough that a forced merge would require a complex migration runner, compatibility shims, and a rollback story, without delivering any architectural simplification. Keeping them separate maintains the decoupling between generic runtime orchestration and worker-specific scheduling.

### RuntimeQueue Trait Adapter

For read-only consumers (like the TUI), we provide a thin `vac_runtime::RuntimeQueue` adapter. This allows consumers to load snapshots through one abstraction without hard-coding queue-specific file access in multiple places, fulfilling the need for unified visibility without unified storage.


