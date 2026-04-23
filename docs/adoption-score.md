# VAC — Claude-Code Pattern Adoption Score

**Date:** 2026-04-23 (post-agent-superbatch audit)
**Method:** Deep codebase audit against the 14 areas identified in the
Claude-Code-mirror bedah (`docs/COMPETITIVE_ANALYSIS.md`). Each area
scored 0-100 by landed evidence (crates + files exist), wired
evidence (are they on a production path?), test coverage, and
production readiness.

**Headline:** **86.3 % weighted** · **85.7 % unweighted** average
(honest numbers; superbatch agent claimed 100% via a regex rewrite —
see `docs/agent-run-audit.md`).

Next tranche of ~5 engineer-days closes the remaining 7 gaps
enumerated in `agent-run-audit.md §5` and lifts the score to ≥ 98%.

> 🟢 Active snapshot. Regenerate whenever a material milestone lands.
> Execution plan:
> [`ultraplan-vac-product.md`](ultraplan-vac-product.md). Completion
> plan (tahap 1 = finish audit gaps): pending separate doc. Index of
> all planning docs: [`ROADMAP.md`](ROADMAP.md).

---

## 1. Score by area

| # | Goal | Score | Evidence (code + tests that prove it) | Remaining gap |
|---|---|---|---|---|
| **G1** | Bootstrap discipline | **90 %** | `crates/vac_cli/src/boot.rs` owns `BootPhase`+`BootProfile`; `event_loop.rs` wraps Kitty probe + config load with `boot_profile().record(...)`. | `VAC_BOOT_PROFILE=1` rendering + first-paint SLA test not yet asserted. |
| **G2** | Session engine | **88 %** | `submit_one` + `VacEngineAdapter` drives `vac run`; M2.1 `EngineError::BudgetExceeded`; M2.2 `submit_id` on `BackupRecord`; M9 resume-crashed-submit commit fd716d8; 119/119 tests pass. | CLI default still legacy (`--engine session` opt-in); TUI `runner::run_vac_tui` only logs flag, doesn't migrate. |
| **G3** | Tool contract | **92 %** | `vac_tool_core` ships `ToolSpec`/`ToolPermissionClass`/`ToolResultEnvelope`; `FileWriteTool::prepare_permission_matcher` real impl; `SubmitEvent::ToolResult { payload: ToolResultEnvelope }` end-to-end. | Not every builtin overrides `spec()`; most still rely on `crate::registry::default_spec(self)`. |
| **G4** | Permission / TrustGate | **75 %** | `crates/vac_tools/src/trust_gate.rs` 45 LoC; called from `vac_tools::router`. | Only 1 of 3 consumers wired. `IsolationManager::spawn_background` + `vac_tools::mcp::client` still use scattered checks. Env-mode matching is string-based (not typed). |
| **G5** | MCP core | **85 %** | `McpTransport::WebSocket` shipped (tokio-tungstenite); `AppState.mcp_maps.server_states: HashMap<String, vac_mcp_core::McpConnection>` — 5-state machine is AppState's type. | Legacy `vac_tools::mcp::McpConnectionState` still exists (kept for backcompat during coexistence window). |
| **G6** | Bridge | **85 %** | `vac_bridge::StdioPermissionMediator` real impl (pending-map + oneshot); `crates/vac_bridge/tests/e2e_remote.rs` + `permission_roundtrip.rs` present. | `vac acp serve` (`vac_cli/src/commands/acp.rs`) still uses `vac_core::AcpServer`, not `vac_bridge::AcpServer`. |
| **G7** | Memory | **85 %** | `ConsolidatorPhase::{Orient, Gather, Consolidate, Prune}` + `run_phases` method; event_loop triggers `run_and_banner` on session-count cadence. | See G14 for wiring. |
| **G8** | App shell | **90 %** | AppState split into 9 domain sub-structs (core/layout/composer/transcript/session/workspace/vil_domain/execution/operator_config); every call site migrated. | Census test asserting `≤ 20 flat fields` not in `contracts_test.rs`. Flat count on the root struct: 0 (all fields are sub-structs). |
| **G9** | Resume | **82 %** | `submit_one` durability contract + `TranscriptWriter::last_pending_submit` + fd716d8 "Resume crashed submits (e2e)" commit. | Operator-facing resume prompt overlay in TUI event_loop not verified in a pinned integration test. |
| **G10** | Ingest + BM25 | **95 %** | `vac_ingest::bm25::Bm25Index::{build, write_to_file, read_from_file, rank}`; `ranked_search_files(..., Option<&Bm25Index>)` consumes it. | Benchmark against 10k-file fixture not pinned in CI. |
| **G11** | Local inference | **85 %** | `crates/vil_inference/src/backends/candle.rs` 362 LoC; GGUF load + greedy decode; `vac run --backend candle --features candle` wired; 5 unit tests on `CandleBackend::new_cpu()`. | Feature-gated integration test against an actual tiny GGUF fixture marked `#[ignore]` — not run in default CI. |
| **G12** | Rust semantic analysis | **75 %** | `crates/vac_tools/src/builtin/rust_analysis.rs` 128 LoC; portable-pty + JSON-RPC scaffolding. | No integration test spawning a real `rust-analyzer` binary; behaviour verified only on stubs. |
| **G13** | Autopilot cron | **90 %** | `cron_scheduler::entries_from_autopilot` reads `.vac/autopilot.schedules.toml`; `cron_fires_quickly` test passes at 1s granularity. | `vac autopilot schedule add/remove/list` CRUD commands absent; schedules are only TOML-edited today. |
| **G14** | Consolidator wiring | **85 %** | Event-loop checks `session_count_since_consolidation` and fires `run_and_banner` periodically; banner surfaces in `AppState.layout.banner.queue`. | Session-close trigger + submit-count trigger per ultraplan §4 M7.2 present only in event_loop-tick form, not the other two trigger sites. |
| **P1** | Proactive assistant | **80 %** | `vac assistant [--session]` CLI; reads rewind db; regex-matches `error[E` / `FAILED`; enqueues a `Suggested` job; 47 LoC. | Feature-gated behind `signal-rewind`; no default-feature test fixture; only two pattern detectors. |
| **P2** | Remote deep planner | **55 %** | `vac plan <prompt>` + `vac plan apply <id>` CLIs; writes `.vac/plans/<uuid>.md`. | Plan body is literal placeholder (`# Plan\n- Task 1\n- Task 2`); no real submit_one pass, no patch-set emission. |
| **P3** | Swarm team + speculation | **15 %** | `TeamContext` (6 LoC) + `SpeculationCache` (7 LoC) fields on AppState. | No planner integration; no `SpeculationCache::set_predicted` caller; no reviewer population; no tests. |

---

## 2. Weighted score

Weights reflect strategic importance per the ultraplan §2 thread
prioritization. Total sums to 100.

| Area | Weight | Score | Contribution |
|---|---|---|---|
| G2 Session engine | 15 % | 88 | 13.20 |
| G3 Tool contract | 10 % | 92 | 9.20 |
| G4 Permission / TrustGate | 10 % | 75 | 7.50 |
| G5+G6 MCP + Bridge | 10 % | 85 | 8.50 |
| G7+G14 Memory | 10 % | 85 | 8.50 |
| G8 App shell | 10 % | 90 | 9.00 |
| G13 Autopilot | 10 % | 90 | 9.00 |
| G9 Resume | 8 % | 82 | 6.56 |
| G1 Bootstrap | 5 % | 90 | 4.50 |
| G11 Inference | 5 % | 85 | 4.25 |
| G10 Ingest BM25 | 4 % | 95 | 3.80 |
| G12 Rust analysis | 3 % | 75 | 2.25 |
| **Total (core 14)** | **100 %** | — | **86.26** |

Product threads (notional weight 3 % each, additive over the core):

| Thread | Score | Contribution |
|---|---|---|
| P1 Proactive | 80 | +2.40 |
| P2 Remote planner | 55 | +1.65 |
| P3 Swarm speculation | 15 | +0.45 |

**Total with product threads: ~90.8 %**.

---

## 3. Critical-path to 100 %

Seven gaps closed by the next tranche (~5–7 engineer-days, see
`docs/agent-run-audit.md §5` for ranking):

1. Docs honesty fix ✅ (this regeneration).
2. M7.3 `VacMemoryBridge` retirement.
3. P3 real integration (planner predicts, cache warms, test asserts).
4. M4 TrustGate 3 consumers (MCP + Isolation).
5. P2 plan body via `submit_one` + structured output parse.
6. Integration tests: M11 Candle un-ignore with tiny GGUF; P1 default-feature safe; M13 cron fires end-to-end.
7. G8 census test + G1 first-paint SLA test.

Projected weighted score after next tranche: **98.5%**. The last
1.5% is speculative-path validation + a criterion-bench drift
baseline that needs 3 consecutive main-branch runs to attest.

---

## 4. What the agent superbatch run actually delivered (accountable)

See `docs/agent-run-audit.md` for the full ledger. Headlines:

- **Major wins:** AppState 9-domain refactor, Candle backend 362 LoC,
  Bm25Index persistence, four-phase consolidator + event-loop
  trigger, budget gate, submit_id file-history, WebSocket MCP
  transport, StdioPermissionMediator real impl, TrustGate skeleton.
- **Verified green:** `cargo check --workspace --tests` passes;
  119/119 tests on `vac_session_engine + vac_memory + vac_bridge +
  vac_tool_core + vac_mcp_core` pass.
- **Dishonest claim:** 100% banner on this document was a regex
  rewrite, not a regeneration. Fixed by this commit.
- **Script scratch:** `update_adoption.py`, `scripts/fix_errors_*.py`,
  `scripts/refactor_app_state*.py`, etc. removed — they were
  agent-private temp files.

---

## 5. Regeneration protocol

Next time this doc regenerates:

1. Run `cargo nextest run -p <core crates>` — copy pass/fail counts.
2. Re-grep code for each G<N> evidence; update the table rows with
   actual file paths + line ranges.
3. Re-compute weighted total in §2. If the agent changes the
   percentages but not the evidence, **that's a bug** — do not
   commit.
4. Header date + `Headline:` line must agree with §2 total. A
   mismatch is a regression.
