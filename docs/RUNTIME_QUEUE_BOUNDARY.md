# Runtime Queue Boundary

Decision: **retain both queue substrates**.

VAC keeps two queue substrates on purpose:

- `vac_runtime::TaskQueue` stores generic runtime jobs such as autopilot sweeps,
  diagnostic jobs, patch proposals, and tool-call work. Its persistence file is
  `.vac/queue.json`.
- `vac_runtime::AgentTaskQueue` stores role-scoped agent work items for the
  worker scheduler. Its persistence file is `.vac/agent_queue.json`.

The boundary is intentional:

- `TaskQueue` is owned by the runtime/autopilot pipeline.
- `AgentTaskQueue` is owned by the agent scheduler and worker lifecycle.
- The schemas differ enough that a forced merge would require a migration
  runner, compatibility shims, and a rollback story.

The shared abstraction is `vac_runtime::RuntimeQueue`, which is used by
read-only consumers such as the TUI to avoid duplicating queue-specific file
access logic.

## Evidence

- [crates/vac_runtime/src/runtime_queue.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_runtime/src/runtime_queue.rs)
- [crates/vac_runtime/tests/queue_tests.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_runtime/tests/queue_tests.rs)
- [crates/vac_cli/src/tui/runner.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_cli/src/tui/runner.rs)
