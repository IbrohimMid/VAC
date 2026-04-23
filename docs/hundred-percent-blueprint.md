# VAC — 100% Adoption Blueprint

> ⚫ **Superseded.** M1..M14 milestones below are reproduced and
> extended in [`ultraplan-vac-product.md`](ultraplan-vac-product.md)
> §3 (milestone table) and §4 (evidence-anchored deep-dives).
> Start with [`ROADMAP.md`](ROADMAP.md) for the current execution
> plan. This doc is kept for dependency-graph reference.

**Goal:** take the Claude-Code pattern adoption score from **66.7% →
100%** across all 14 goal areas (G1–G14 in `adoption-score.md`).
**Date:** 2026-04-23
**Non-goal:** new product surface. This is the closure plan for the
gaps already enumerated — no scope creep.

Each goal area is treated as a milestone with:
- Current score + delta to 100.
- "Done" definition (what observable behaviour proves the score).
- Concrete implementation steps.
- Tests that lock in the contract.
- Dependencies (what must land first).

At the end (§16) is a sequencing plan with ~30 working-day estimate.

---

## M1 — G1 Bootstrap discipline (62 → 100)

**Done =** Every startup side-effect in `vac_cli` + `vac_tui_runtime`
is explicitly classified as `boot_critical` or `boot_deferred`, with
a timing profile printed under `VAC_BOOT_PROFILE=1`.

### Steps
1. Introduce `crates/vac_cli/src/boot.rs` with an enum
   `BootPhase { Critical, Deferred }` and a `BootProfile` recorder
   (atomic timing map).
2. Replace each top-level side-effect in `main.rs` + `runner.rs` with
   `boot_profile.record(phase, || ...)`. Every block gets a phase.
3. Deferred work (MCP probe, theme watcher, Kitty DCS flush,
   snapshot load, recent-trajectory ingest) moves into a
   `tokio::spawn` that starts *after* first draw.
4. Expose `VAC_BOOT_PROFILE=1` env to emit a one-line table on
   startup (phase, duration, %).
5. SLA test asserts `key_to_first_paint_ms < 500` on a warm tempfs.

### Dependencies
- None. Pure refactor inside `vac_cli` + `vac_tui_runtime`.

### Estimated effort
2 engineering-days.

---

## M2 — G2 Session / query engine (78 → 100)

**Done =** `vac run`, `vac interactive`, and every integration test
route through `vac_session_engine::submit_one`. Legacy direct
`VacEngine::run_task_with_approvals` path removed.

### Steps
1. Finish R0.c: rewrite `vac_tui_runtime::runner` event-loop per
   submit to call `submit_one` with `VacEngineAdapter`.
2. Flip `EngineMode::Legacy` → `EngineMode::Session` as the CLI
   default; keep `--engine legacy` for one release, then delete.
3. Migrate every `crates/vac_cli/tests/*.rs` that drives
   `VacEngine::run_task_*` to the engine path.
4. Delete `run_task_with_approvals` call sites; the legacy path
   atrophies.
5. Contract test: `submit_one` produces the same transcript shape
   for both drivers (`vac run` + `vac interactive`).

### Dependencies
- M1 (bootstrap profile surfaces the engine-init phase).

### Estimated effort
3–4 engineering-days. High-risk — every integration test touches
this surface.

---

## M3 — G3 Formal tool contract (85 → 100)

**Done =** Every `VilTool` has an explicit `spec()` override (not
just the default). `ToolResultEnvelope` round-trips from tool →
engine → transcript → TUI render.

### Steps
1. Audit every `crates/vac_tools/src/builtin/*.rs`. Replace the
   default `VilTool::spec()` with an explicit impl that sets:
   - `capability`, `permission`, `render_hints`
   - `user_facing_name`, `description`
   - `is_read_only`, `is_destructive`, `is_concurrency_safe`
2. Wire `ToolResultEnvelope` into `vac_session_engine::SubmitEvent::
   ToolResult` payload (currently a bare summary string).
3. TUI: `services::message::render_tool_result` reads the envelope
   and picks the render mode based on `RenderHints`.
4. Contract test: every tool in the registry has a non-default spec
   (`assert!(tool.spec().user_facing_name != tool.name())`-style
   invariant).

### Dependencies
- M2 (so transcript carries the envelope end-to-end).

### Estimated effort
2–3 engineering-days.

---

## M4 — G4 Permission / trust / isolation (72 → 100)

**Done =** A single `TrustGate` entry point owns every permission
decision. `ApprovalsState`, `IsolationManager`, and MCP
`McpTrustClass` all route through it.

### Steps
1. New `crates/vac_approvals/src/trust_gate.rs` owning
   `TrustGate::check(ctx, spec) -> Decision`.
2. Replace direct `IsolationManager` calls in `vac_runtime::executor`
   with `TrustGate::check`.
3. Replace scattered MCP trust checks in `vac_tools::mcp::client`
   with `TrustGate::check` (using `spec.capability` + scope).
4. `vac_cli::commands::doctor` gains a `vac doctor permissions`
   subcommand that dumps the resolved gate decisions for the current
   project.
5. Test: approval dialog + isolation spawn + MCP connect all produce
   the same `Decision` shape for equivalent inputs.

### Dependencies
- M3 (spec carries capability/permission).

### Estimated effort
2–3 engineering-days.

---

## M5 — G5 MCP core primary swap (68 → 100)

**Done =** `AppState.mcp_maps.server_states` holds
`vac_mcp_core::McpConnection` (the 5-state machine), not the legacy
`vac_tools::mcp::McpConnectionState`. Legacy type is a re-export
alias for backcompat only.

### Steps
1. In `vac_tools::mcp::probe_mcp_server`, return
   `vac_mcp_core::McpConnection` directly (already have the
   `to_core_connection` helper from R3-partial).
2. Change `AppState.mcp_maps.server_states: HashMap<String,
   McpConnection>`; migrate all readers.
3. TUI status panel reads `connection.state.label()` (5-state
   machine), not the legacy `McpConnectionStatus` enum.
4. Legacy `McpConnectionState` marked `#[deprecated]`; delete after
   one release cycle.
5. Integration test: full lifecycle Disabled → Enable → Pending →
   ConnectOk → Connected → Disconnect → Failed → Enable.

### Dependencies
- M4 (TrustGate for connection permission decisions).

### Estimated effort
2 engineering-days.

---

## M6 — G6 Bridge e2e (64 → 100)

**Done =** `vac acp serve` drives `vac_bridge::AcpServer`, not
`vac_core::AcpServer`. Remote client can run a full
submit → tool-approval → result round-trip proven by integration
test.

### Steps
1. Replace `vac_cli/src/commands/acp.rs` contents: instantiate
   `vac_bridge::AcpServer` with `VacEngineAdapter` + a
   `PermissionMediator` that routes the request through stdio to the
   remote client.
2. Implement `StdioPermissionMediator` in `vac_bridge` that sends
   `OutboundEvent::PermissionRequest` + waits on
   `InboundEvent::PermissionResponse` via oneshot.
3. New integration test `crates/vac_bridge/tests/e2e_remote.rs`:
   spawn a mock remote client task, connect via in-memory duplex,
   drive Hello → Submit → receive PermissionRequest → send allow →
   receive ToolResult → assert on final transcript.
4. Document the ACP-over-stdio wire format in
   `docs/acp-protocol.md`.

### Dependencies
- M2 (`submit_one` is the engine spine).
- M4 (TrustGate produces the decision the mediator forwards).

### Estimated effort
3 engineering-days.

---

## M7 — G7 + G14 Memory wired (76 + 15 → 100 + 100)

**Done =** `vac_memory::Consolidator::run_once` is called from three
real triggers: session close, every N submits, and an autopilot cron
entry. `vil_memory` retired (or reduced to a thin adapter).

### Steps
1. **Trigger: session close.** Hook `vac_session_control::
   persist_session_snapshot` → call `Consolidator::run_once` with
   the session's raw signal lines as input.
2. **Trigger: submit count.** `submit_one` increments a persisted
   counter; every N (configurable, default 10) submits the post-
   finished event spawns a consolidator task.
3. **Trigger: cron.** `AutopilotConfig.schedules` gets a built-in
   entry for `@hourly memory consolidate`.
4. **Retire vil_memory:** introduce `vil_memory::adapter::
   VacMemoryBridge` that implements the `WorkingMemory` /
   `EpisodicMemory` traits by delegating to `vac_memory::
   MemoryScanner` + `find_relevant`. Flip the vil_swarm wiring to
   the bridge, delete the redb-backed impl, keep the crate shell as
   a re-export.
5. Observability: `ConsolidationReport` pushed to
   `AppState.banner.queue` via the existing
   `push_consolidation_banner` helper at every trigger site.
6. Tests: three triggers each produce a banner + a written memdir
   file. `vil_swarm` tests still pass against the bridge.

### Dependencies
- M2 (submit_one is the hook site).
- M6 (cron fires via autopilot, which depends on M4 TrustGate).

### Estimated effort
4 engineering-days. Heaviest milestone; vil_memory retirement
cascades into vil_swarm.

---

## M8 — G8 App shell (65 → 100)

**Done =** AppState has ≤ 20 flat fields. Everything else lives in a
domain sub-struct under `app/types/`.

### Steps
1. Complete the R1 cut line:
   - `OperatorState` absorbs `theme`, `sessions`, `session_id`,
     `commands`, `context_chips*`, `modified_files`, `todos`.
   - `RenderMetricsState` groups `render_metrics` + `queue_metrics` +
     `pending_user_messages`.
   - `InputState` groups `input`, `input_tx`, `paste_state`.
2. Contract test: macro that enumerates `AppState` top-level fields
   and asserts `count <= 20`.
3. Update `docs/architecture.md` state diagram.

### Dependencies
- None (mechanical refactor, 41 call sites already migrated for R1).

### Estimated effort
1–2 engineering-days.

---

## M9 — G9 Resume end-to-end (70 → 100)

**Done =** Crash mid-submit → next `vac interactive` boots → detects
pending submit via `TranscriptWriter::last_pending_submit` → prompts
operator → re-drives submit. Integration test proves it.

### Steps
1. `vac_tui_runtime::event_loop` boot reads
   `TranscriptWriter::last_pending_submit` for the active session.
2. If `Some(entry_id)`, pushes a blocking overlay: "Session crashed
   mid-submit (N hours ago). Resume? [Y/n]".
3. Yes → call `submit_one` with the stored `Accepted` row's content.
   No → append an `Aborted` row with reason "operator-declined
   resume".
4. Schema versioning: `SessionSnapshot::schema_version: u32`;
   bump + add migration hook.
5. Integration test: drive submit, kill process between Accepted
   and Finished, boot again, assert the overlay fires and
   resume produces a matching Finished.

### Dependencies
- M2.

### Estimated effort
2 engineering-days.

---

## M10 — G10 Ingest bench + cache (82 → 100)

**Done =** `vac_ingest::bootstrap` runs under 200ms on a warm cache
for a 10k-file project. BM25 index persisted to
`.vac/ingest/index.bin`.

### Steps
1. `vac_ingest::bm25::Bm25Index` struct that pre-tokenizes + caches
   DF/avgdl. Rebuild only when a file mtime changes since index
   write.
2. Persistence via `bincode` to `.vac/ingest/index.bin`.
3. Integration test: build index on 10k-file fixture, assert second
   load is <50ms.
4. Criterion bench in `crates/vac_ingest/benches/bootstrap.rs`.

### Dependencies
- M1 (deferred boot phase so reindex doesn't block first paint).

### Estimated effort
2 engineering-days.

---

## M11 — G11 Local inference (35 → 100)

**Done =** One real backend (Candle + small GGUF) loads a model,
produces tokens on a known prompt, passes an integration test.

### Steps
1. Pick a small open GGUF (TinyLlama 1.1B or Phi-3-mini 3.8B Q4).
2. `vil_inference::backends::candle` — load the GGUF via
   `candle-transformers`, run a greedy decode loop with temperature
   0.0, return the decoded text.
3. Feature-gated: `cargo build -p vil_inference --features candle`.
4. Integration test under `crates/vil_inference/tests/candle.rs`:
   loads a small dummy GGUF from a test fixture, asserts the
   first-10-token output is deterministic.
5. `vac_cli` exposes `--backend candle` on `vac run` for local runs.

### Dependencies
- M3 (tool contract so the backend is selectable via spec).

### Estimated effort
4 engineering-days. Single-thread CPU inference first; accelerators
(CUDA/Metal) a separate follow-up.

---

## M12 — G12 Rust semantic analysis (22 → 100)

**Done =** `vac_tools::rust_analysis::AnalysisHost` spawns a real
`rust-analyzer` subprocess via `portable-pty`, answers symbol lookup
+ diagnostic queries.

### Steps
1. Pin `ra_ap_*` to a specific Cargo-compatible revision (or skip
   and drive `rust-analyzer` binary via LSP JSON-RPC over stdio —
   simpler).
2. `PortablePtyHost` impl of `AnalysisHost`: spawn `rust-analyzer`,
   send `initialize`, keep the connection alive, route
   `textDocument/hover` + `textDocument/definition` +
   `textDocument/diagnostic` requests.
3. Expose as a tool in `vac_tools::builtin::rust_analysis.rs`:
   `rust_symbol_lookup`, `rust_diagnostics`.
4. Integration test: spawn the binary (skip if missing from PATH),
   hover on a known symbol in the workspace, assert reply shape.

### Dependencies
- M3 (tool contract for the new builtin tools).

### Estimated effort
3–4 engineering-days.

---

## M13 — G13 Autopilot fires schedules (75 → 100)

**Done =** `AutopilotConfig.schedules` cron entries actually enqueue
jobs when their cron fires. `vac autopilot schedule list/add/remove`
CLI commands work.

### Steps
1. `vac_runtime::autopilot_controller` owns the `CronScheduler`
   lifecycle. On tick, it calls `entries_from_autopilot` + pushes
   matched tasks into `TaskQueue`.
2. `vac_cli::commands::autopilot::schedule` adds the three CLI
   subcommands, persisting to `.vac/autopilot.toml` via the existing
   `schedule_cron` tool.
3. Integration test: define a `@every 1s` schedule in a test
   autopilot config, run the controller for 3s, assert at least 2
   jobs enqueued.

### Dependencies
- M4 (TrustGate evaluates cron-triggered tasks).

### Estimated effort
2 engineering-days.

---

## M14 — G14 Consolidator wired (15 → 100)

**Subsumed by M7.** G14 closes when M7 lands all three triggers +
the banner surfacing.

---

## 15. Dependency graph

```
  M1 (bootstrap)
       │
       ▼
  M2 (session engine) ──▶ M3 (tool contract) ──▶ M4 (trust gate)
       │                                                │
       ▼                                                ▼
  M9 (resume e2e)                                M5 (MCP swap)
                                                        │
  M10 (ingest cache)                                    ▼
                                                 M6 (bridge e2e)
  M11 (Candle backend)                                  │
                                                        ▼
  M12 (rust analysis)                            M7 + M14 (memory wired)
                                                        │
                                                        ▼
                                                 M13 (autopilot fires)

  M8 (app shell) ─ parallel, no deps
```

- M1 gates M2 because per-phase timing reveals engine init cost.
- M2 gates M3/M7/M9 because transcript is the carrier for envelopes,
  consolidator triggers, and resume state.
- M3 gates M4 because TrustGate consumes tool spec.
- M4 gates M5/M6/M13 because each needs the unified permission
  decision.
- M7 gates nothing downstream but is the heaviest milestone.
- M8, M10, M11, M12 are independent and can run in parallel.

---

## 16. Sequencing + effort

### Wave 1 — foundation (8 days)

| M | Area | Days | Parallel? |
|---|---|---|---|
| M1 | Bootstrap | 2 | — |
| M8 | App shell | 1.5 | ✓ |
| M10 | Ingest cache | 2 | ✓ |
| M2 | Session engine spine | 3.5 | — |

End of wave 1: **G1 / G2 / G8 / G10 at 100%.**

### Wave 2 — contracts + subsystem consolidation (9 days)

| M | Area | Days | Parallel? |
|---|---|---|---|
| M3 | Tool contract | 2.5 | — |
| M9 | Resume e2e | 2 | ✓ with M3 |
| M4 | TrustGate | 2.5 | — |
| M5 | MCP swap | 2 | ✓ with M6 |

End of wave 2: **G3 / G4 / G5 / G9 at 100%.**

### Wave 3 — integration + backends (13 days)

| M | Area | Days | Parallel? |
|---|---|---|---|
| M6 | Bridge e2e | 3 | — |
| M7+M14 | Memory wired | 4 | — (depends on M2/M6) |
| M11 | Candle backend | 4 | ✓ |
| M12 | rust-analyzer | 3 | ✓ |
| M13 | Autopilot fires | 2 | ✓ |

End of wave 3: **G6 / G7 / G11 / G12 / G13 / G14 at 100%.**

### Total: **~30 engineering-days** for one senior engineer.

With two engineers working parallel tracks where the graph allows,
realistic calendar time is **~4 weeks**.

---

## 17. Acceptance criteria (100 % claim)

Score moves to 100 % only when ALL of these hold:

1. Every area's headline test passes in CI (not opt-in features).
2. `docs/adoption-score.md` is regenerated and every row shows 100 %.
3. `docs/architecture.md` diagram has zero "scaffold" / "deferred"
   badges.
4. `cargo check --workspace --tests` clean on main.
5. `cargo nextest run --workspace` green except for a small
   allowlist of known-slow tests (criterion, full-RA spawn).
6. The lychee doc link-check (`.github/workflows/doc-links.yml`) is
   flipped from `continue-on-error: true` to required.
7. `check_layering.sh` extended to enforce the five layers in
   `architecture.md`; a synthetic regression PR proves the gate bites.

---

## 18. Risks + mitigations

| Risk | Mitigation |
|---|---|
| M2 engine cutover breaks TUI integration tests | Feature flag `VAC_ENGINE=session` coexists for 2 weeks; retire only after green. |
| M7 vil_memory retirement cascades into vil_swarm | `VacMemoryBridge` adapter flips implementation atomically; vil_swarm signature unchanged. |
| M11 Candle GGUF loader fails on large models | Ship TinyLlama 1.1B as the reference test model; larger models remain user-provided. |
| M12 ra_ap_* API churn | Pick the LSP-over-stdio route first; avoid pinning to unstable crates. |
| Critical-path wave too long | Split waves 2+3 across two engineers; M6 + M7 + M11 can land out of order if M2/M4 gates hold. |

---

## 19. What this doesn't claim

- This plan takes VAC to 100 % of the *Claude-Code pattern adoption*
  axis, not to 100 % of VIL product goals. VIL-specific value props
  (VWFD parity, VIL policy engine, VIL IR validation) are already
  load-bearing — see `docs/PRODUCT_SPEC.md`.
- "100 %" is scoped to the 14 areas in `adoption-score.md`. A future
  audit may surface a G15.
- Calendar time estimates assume no external blockers (provider API
  changes, OS upgrades, hardware).

---

## 20. Tracking

Each milestone gets a `refactor(M<N>): <short>` commit prefix
matching the ring discipline from `completion-blueprint.md`.
`docs/adoption-score.md` is regenerated at the end of each wave; the
header date tracks the score trajectory.
