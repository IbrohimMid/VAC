# Runtime Queue Boundary

VAC keeps two queue substrates on purpose:

- `vac_runtime::TaskQueue` stores generic runtime jobs such as autopilot sweeps, diagnostic jobs, patch proposals, and tool-call work. Its persistence file is `.vac/queue.json`.
- `vac_runtime::AgentTaskQueue` stores role-scoped agent work items for the worker scheduler. Its persistence file is `.vac/agent_queue.json`.

The boundary is intentional:

- `TaskQueue` is owned by the runtime/autopilot pipeline.
- `AgentTaskQueue` is owned by the agent scheduler and worker lifecycle.
- The schemas differ enough that a forced merge would require a migration runner, compatibility shims, and a rollback story.

For phase 2 we retained both substrates and added a thin `vac_runtime::RuntimeQueue` adapter so read-only consumers such as the TUI can load snapshots through one abstraction without hard-coding queue-specific file access in multiple places.

