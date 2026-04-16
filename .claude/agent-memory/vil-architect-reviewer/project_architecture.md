---
name: VAC CLI Architecture Overview
description: Architecture of vastar-agentic-cli Rust CLI - 3-layer crate structure with approval flow, TUI, and swarm orchestration
type: project
---

vastar-agentic-cli is a Rust CLI with a 3-layer crate architecture:

1. **vil_swarm** (crates/vil_swarm) - Core orchestration: SwarmOrchestrator, AgentRunState, tool_executor, stream_processor, context_budget. Owns the agent loop. Exports ApprovalResponse. Two-stage pipeline: Planner (no approvals) -> Coder (with approvals).

2. **vac_core** (crates/vac_core) - Engine layer: VacEngine bridges TUI to swarm. Threads approval_rx from runner through to swarm. RuntimeUpdate enum is the event bus between engine and TUI. Contains deprecated approve_tool_call/reject_tool_call methods (dead code since structured approval flow was implemented).

3. **vac_cli** (crates/vac_cli) - TUI layer: runner.rs spawns engine tasks, manages ActiveApprovalTx shared handle. Services module contains message.rs (rendering), bash_block.rs (code block detection), markdown_renderer.rs.

**Why:** Understanding the crate boundaries is essential for reviewing dependency direction and coupling in future audits.

**How to apply:** When reviewing changes, verify that dependencies flow TUI -> Engine -> Swarm (never reverse). ApprovalResponse originates in vil_swarm and flows outward correctly.
