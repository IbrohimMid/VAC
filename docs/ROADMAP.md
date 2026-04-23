# VAC Roadmap — 2026 Q2

> **Authoritative plan** lives in [`cc-parity-blueprint.md`](cc-parity-blueprint.md).
> This file is the short navigation index — refer to the blueprint for detail,
> and to [`cc-parity-plan.md`](cc-parity-plan.md) for milestone execution.

## Where we are

Core engine invariants are landed and honest:

- `vac_session_engine` with transcript-before-query durability, budget gate,
  submit-id plumbing, resume e2e.
- `vac_mcp_core` + stdio/WS transports, `TrustGate` unified across 3 consumers
  (router / MCP client / isolation spawn), `vac_bridge::RemoteSession` with
  stdio transport wiring `vac plan --remote`.
- Memory: `vac_memory` 4-phase Consolidator + `VacMemoryBridge`; retrieval via
  `vac_ingest::Bm25Index` with staleness check.
- Local inference: Candle default when feature on, `StdioLspHost` for
  rust-analyzer.
- Observability: `vac_signal` bounded buffers + scoring + distillation,
  structured tracing spans on submit/infer/consolidator.
- Evaluation: `vac_trace::AgentDecision` + `vac eval --golden` trajectory
  replay.
- CI: layering check, sync-I/O lint, nextest, mutation, fuzz, coverage 70%
  floor, 6-provider smoke matrix.

## Where we're going

After deep-dive on the leaked Claude Code source (sourcemap-exposed,
Mar 2026), the next phase is **closing the UX-surface gap** without
abandoning VAC's positioning: self-hostable, rust-safe, observability-first.

Ten items drive the plan, in priority order:

1. **W1** Fork-based speculation (speculation service spawns a real
   sub-agent with cache-safe params, not heuristic string prediction)
2. **W2** Tool interface richness (per-input capability, deferred loading,
   disk-spill threshold, observable-input backfill)
3. **W3** Skills as first-class composition unit (batch, loop, remember,
   verify, stuck, simplify)
4. **W4** MCP elicitation + channel ACL
5. **W5** Multi-language LSP pool + passive-feedback diagnostics
6. **W6** Subagent coordinator with shared-AppState semantics
7. **W7** Bridge auth stack (OAuth, JWT, capacity-wake)
8. **W8** Slash command breadth (target: +50 commands)
9. **W9** Rate-limit + policy-limit tracking
10. **W10** Idle-time background work (auto-dream, away-summary)

See [`cc-parity-plan.md`](cc-parity-plan.md) for per-milestone scope,
acceptance, and sequencing.
